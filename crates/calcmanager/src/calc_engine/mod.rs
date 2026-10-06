// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Header Files/CalcEngine.h` and `CEngine/calc.cpp`
//! (`CCalcEngine`). The member functions defined in the other `CEngine`
//! source files live in the sibling modules (`scicomm`, `scidisp`,
//! `scifunc`, `scioper`, `sciset`), one per original file.
//!
//! Process-wide statics of the C++ engine (`s_engineStrings` and the
//! `gldPrevious` display cache in scidisp.cpp) are thread-local here, which
//! matches the thread-local ratpack context: all engines created on one
//! thread share them, exactly like all engines in one C++ process do.

mod random;
mod scicomm;
mod scidisp;
mod scifunc;
mod scioper;
mod sciset;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ratpack::{AngleType, CalcResult, NumberFormat, Rational, rational_math};

use crate::calc_display::{CalcDisplayRef, HistoryDisplayRef};
use crate::calc_input::CalcInput;
use crate::calc_utils::{is_bin_op_code, is_digit_op_code, is_unary_op_code};
use crate::ccommand::*;
use crate::engine_strings::*;
use crate::expression_command::{ExpressionCommand, OpndCommand};
use crate::history::{HistoryCollector, MAXPRECDEPTH};
use crate::radix_type::RadixType;
use crate::resource::ResourceProvider;

/**************************************************************************/
/*** Global variable declarations and initializations                   ***/
/**************************************************************************/

const DEFAULT_MAX_DIGITS: i32 = 32;
const DEFAULT_PRECISION: i32 = 32;
const DEFAULT_RADIX: u32 = 10;

const DEFAULT_DEC_SEPARATOR: char = '.';
const DEFAULT_GRP_SEPARATOR: char = ',';
const DEFAULT_GRP_STR: &str = "3;0";
const DEFAULT_NUMBER_STR: &str = "0";

/// This is expected to be in same order as IDM_QWORD, IDM_DWORD etc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumWidth {
    /// Number width of 64 bits mode (default)
    QwordWidth = 0,
    /// Number width of 32 bits mode
    DwordWidth = 1,
    /// Number width of 16 bits mode
    WordWidth = 2,
    /// Number width of 8 bits mode
    ByteWidth = 3,
}

impl NumWidth {
    /// `(NUM_WIDTH)i` for `i` in `0..=3`.
    pub fn from_index(i: i32) -> Option<NumWidth> {
        match i {
            0 => Some(NumWidth::QwordWidth),
            1 => Some(NumWidth::DwordWidth),
            2 => Some(NumWidth::WordWidth),
            3 => Some(NumWidth::ByteWidth),
            _ => None,
        }
    }
}

pub const NUM_WIDTH_LENGTH: usize = 4;

thread_local! {
    /// `CCalcEngine::s_engineStrings` — the string table shared across all instances.
    static S_ENGINE_STRINGS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Extension: what a saved session needs, beyond
/// [`CalcEngine::get_history_collector_commands_snapshot`], to continue a
/// calculation as it would have (see [`CalcEngine::continuation`]).
#[derive(Clone, Debug, Default)]
pub struct Continuation {
    /// The display shows a value those commands don't produce.
    pub shown: Option<ShownValue>,
    /// After `=`: the operator and right operand another `=` repeats.
    pub repeat: Option<(i32, OpndCommand)>,
    /// How the number being entered stands, where replaying those commands
    /// wouldn't leave it so.
    pub entry: Option<Entry>,
    /// After `=` with nothing to repeat (`(` came next): the next key
    /// clears the expression line, as it does after `=`.
    pub clears: bool,
    /// Replaying the commands wouldn't rebuild the calculation (see
    /// `HistoryCollector::mark_unreplayable`).
    pub unreplayable: bool,
    /// The carry bit RoL and RoR through carry shift in next (`m_carryBit`,
    /// set only by them in Programmer mode; C clears it). No command sets
    /// it, so it is restored with [`CalcEngine::set_carry`].
    pub carry: bool,
    /// Standard mode with no operator pending: the left operand
    /// (`m_lastVal`), which `%` multiplies by. After `=` it is the result
    /// (`2 + 3 = 7 %` is 7 × 5%), which neither the commands nor what `=`
    /// repeats rebuild, so it is restored with
    /// [`CalcEngine::set_left_operand`]. With an operator pending it is
    /// the operand before it, in the commands; the other modes have no `%`,
    /// the one key that reads it with no operator pending.
    pub left: Option<OpndCommand>,
    /// No number is being typed and the input holds none. Otherwise the
    /// number typed last stays in it, unseen, until the next is begun;
    /// whenever 0 is shown (`56 − 56 =`) that makes the engine's
    /// `IsInputEmpty` false, so Scientific and Programmer mode show their C
    /// key as CE and press CE with it. What the restore types needn't leave
    /// the input so, so it is set with [`CalcEngine::set_input_empty`].
    pub empty_input: bool,
}

/// Extension: see [`Continuation::entry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Nothing typed since C, CE or the start: the commands' last operand is
    /// the empty input's 0, which replayed would be a typed digit (that `(`
    /// then multiplies).
    Empty,
    /// No number is being entered and the commands have nothing that ends
    /// one: the empty input was ended by a radix, word size or angle
    /// switch, MS, M+ or M− (± then negates 0 rather than typing a sign).
    Ended,
    /// The commands' last operand is a `%` result, added to the expression
    /// rather than typed: the next digit starts a new number (see
    /// [`CalcEngine::add_entry_as_percent_result`]).
    Percent,
    /// The number being typed had its sign changed last; the commands'
    /// operand carries the sign after its first digit, where `(` would
    /// multiply it.
    Signed,
}

/// Extension: where a shown value (see [`Continuation::shown`]) came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShownValue {
    /// A result, a recalled value or a constant: the next digit replaces it.
    Result,
    /// A typed number whose entry a command that isn't recorded ended
    /// (F-E, MS, a radix switch): the next digit replaces it too, but the
    /// last command is still the digit (`(` multiplies it).
    EndedEntry,
    /// The same, but the number's sign was changed last: the last command
    /// is ± (`(` doesn't multiply it).
    EndedSign,
}

/// `CCalcEngine`
pub struct CalcEngine {
    f_precedence: bool,
    /// This is true if engine is explicitly called to be in integer mode. All bases are restricted to be in integers only
    f_integer_mode: bool,
    calc_display: Option<CalcDisplayRef>,
    resource_provider: Rc<dyn ResourceProvider>,
    /// ID value of operation.
    n_op_code: i32,
    /// opcode which computed the number in m_currentVal. 0 if it is already bracketed or plain number or
    /// if it hasn't yet been computed
    n_prev_op_code: i32,
    /// Flag for changing operation
    b_change_op: bool,
    /// Global mode: recording or displaying
    b_record: bool,
    /// Flag for setting the engine result state
    b_set_calc_state: bool,
    /// Global calc input object for decimal strings
    input: CalcInput,
    /// Scientific notation conversion flag
    n_fe: NumberFormat,
    max_trigonometric_num: Rational,
    /// Current memory value (`None` after `persisted_mem_object()` moved it out).
    memory_value: Option<Rational>,

    /// For holding the second operand in repetitive calculations ( pressing "=" continuously)
    hold_val: Rational,

    /// Currently displayed number used everywhere.
    current_val: Rational,
    /// Number before operation (left operand).
    last_val: Rational,
    /// Holding array for parenthesis values.
    paren_vals: [Rational; MAXPRECDEPTH],
    /// Holding array for precedence values.
    precedence_vals: [Rational; MAXPRECDEPTH],
    /// Error flag.
    b_error: bool,
    /// Inverse on/off flag.
    b_inv: bool,
    /// Flag for previous equals.
    b_no_prev_equ: bool,

    radix: u32,
    precision: i32,
    c_int_digits_sav: i32,
    /// Holds the decimal digit grouping number
    dec_grouping: Vec<u32>,

    number_string: String,

    /// Holding place for the last command.
    n_temp_com: i32,
    /// Number of open parentheses.
    open_paren_count: usize,
    /// Holding array for parenthesis operations.
    n_op: [i32; MAXPRECDEPTH],
    /// Holding array for precedence  operations.
    n_prec_op: [i32; MAXPRECDEPTH],
    /// Current number of precedence ops in holding.
    precedence_op_count: usize,
    /// Last command entered.
    n_last_com: i32,
    /// Current Angle type when in dec mode. one of deg, rad or grad
    angletype: AngleType,
    /// one of qword, dword, word or byte mode.
    numwidth: NumWidth,
    /// # of bits in currently selected word size
    dw_word_bit_width: i32,

    random_generator: Option<random::Mt19937>,

    carry_bit: u64,

    /// Accumulator of each line of history as various commands are processed
    history_collector: HistoryCollector,

    /// word size enforcement
    chop_numbers: [Rational; NUM_WIDTH_LENGTH],
    /// maximum values represented by a given word width based off m_chopNumbers
    max_decimal_value_strings: [String; NUM_WIDTH_LENGTH],
    decimal_separator: char,
    group_separator: char,
}

/// `pi` from the ratpack context (`ratpack::pi()`, a copy of the `pi` global).
pub(crate) fn ratpack_pi() -> CalcResult<Rational> {
    Ok(ratpack::pi())
}

/// `two_pi` from the ratpack context.
///
/// ratpack exposes no `two_pi` accessor, so it is rebuilt exactly: in
/// `ChangeConstants` `two_pi` is `DUPRAT(pi)` followed by `_addrat(two_pi, pi)`,
/// and `Rational::add` (`addrat`) takes the same equal-denominator `_addrat`
/// path (doubling the numerator); `_snaprat` never fires for `pi + pi`.
pub(crate) fn ratpack_two_pi() -> CalcResult<Rational> {
    let pi = ratpack_pi()?;
    pi.add(&pi)
}

fn rational_array<const N: usize>() -> [Rational; N] {
    std::array::from_fn(|_| Rational::default())
}

impl CalcEngine {
    /// `CCalcEngine::LoadEngineStrings`
    fn load_engine_strings(resource_provider: &dyn ResourceProvider) {
        S_ENGINE_STRINGS.with(|strings| {
            let mut strings = strings.borrow_mut();
            for sid in G_SIDS.iter() {
                let loc_string = resource_provider.get_cengine_string(sid);
                if !loc_string.is_empty() {
                    strings.insert(sid.to_string(), loc_string);
                }
            }
        });
    }

    /// `CCalcEngine::InitialOneTimeOnlySetup` — once per load time to call to
    /// initialize all shared global variables.
    pub fn initial_one_time_only_setup(resource_provider: &dyn ResourceProvider) {
        Self::load_engine_strings(resource_provider);

        // we must now set up all the ratpak constants and our arrayed pointers
        // to these constants.
        Self::change_base_constants(DEFAULT_RADIX, DEFAULT_MAX_DIGITS, DEFAULT_PRECISION);
    }

    /// `CCalcEngine::CCalcEngine`
    pub fn new(
        f_precedence: bool,
        f_integer_mode: bool,
        resource_provider: Rc<dyn ResourceProvider>,
        calc_display: Option<CalcDisplayRef>,
        history_display: Option<HistoryDisplayRef>,
    ) -> CalcResult<CalcEngine> {
        let history_collector =
            HistoryCollector::new(calc_display.clone(), history_display, DEFAULT_DEC_SEPARATOR);
        let mut engine = CalcEngine {
            f_precedence,
            f_integer_mode,
            calc_display,
            resource_provider,
            n_op_code: 0,
            n_prev_op_code: 0,
            b_change_op: false,
            b_record: false,
            b_set_calc_state: false,
            input: CalcInput::new(DEFAULT_DEC_SEPARATOR),
            n_fe: NumberFormat::Float,
            max_trigonometric_num: Rational::default(),
            memory_value: Some(Rational::default()),
            hold_val: Rational::default(),
            current_val: Rational::default(),
            last_val: Rational::default(),
            paren_vals: rational_array(),
            precedence_vals: rational_array(),
            b_error: false,
            b_inv: false,
            b_no_prev_equ: true,
            radix: DEFAULT_RADIX,
            precision: DEFAULT_PRECISION,
            c_int_digits_sav: DEFAULT_MAX_DIGITS,
            dec_grouping: Vec::new(),
            number_string: DEFAULT_NUMBER_STR.to_string(),
            n_temp_com: 0,
            open_paren_count: 0,
            n_op: [0; MAXPRECDEPTH],
            n_prec_op: [0; MAXPRECDEPTH],
            precedence_op_count: 0,
            n_last_com: 0,
            angletype: AngleType::Degrees,
            numwidth: NumWidth::QwordWidth,
            dw_word_bit_width: 0,
            random_generator: None,
            // Not initialized by the C++ constructor; IDC_CLEAR resets it.
            carry_bit: 0,
            history_collector,
            chop_numbers: rational_array(),
            max_decimal_value_strings: Default::default(),
            // Not initialized by the C++ constructor; set by SettingsChanged.
            decimal_separator: '\0',
            group_separator: DEFAULT_GRP_SEPARATOR,
        };

        engine.init_chop_numbers()?;

        engine.dw_word_bit_width = engine.dw_word_bit_width_from_num_width(engine.numwidth);

        engine.max_trigonometric_num =
            rational_math::pow(&Rational::from(10), &Rational::from(100))?;

        engine.set_radix_type_and_num_width(Some(RadixType::Decimal), Some(engine.numwidth))?;
        engine.settings_changed()?;
        engine.display_num()?;

        Ok(engine)
    }

    fn init_chop_numbers(&mut self) -> CalcResult<()> {
        // these rat numbers are set only once and then never change regardless of
        // base or precision changes
        self.chop_numbers[0] = ratpack::rat_qword();
        self.chop_numbers[1] = ratpack::rat_dword();
        self.chop_numbers[2] = ratpack::rat_word();
        self.chop_numbers[3] = ratpack::rat_byte();

        // initialize the max dec number you can support for each of the supported bit lengths
        // this is basically max num in that width / 2 in integer
        for i in 0..self.chop_numbers.len() {
            let max_val = self.chop_numbers[i].div(&Rational::from(2))?;
            let max_val = rational_math::integer(&max_val)?;

            self.max_decimal_value_strings[i] =
                max_val.to_string_radix(10, NumberFormat::Float, self.precision)?;
        }
        Ok(())
    }

    fn get_chop_number(&self) -> Rational {
        self.chop_numbers[self.numwidth as usize].clone()
    }

    fn get_max_decimal_value_string(&self) -> String {
        self.max_decimal_value_strings[self.numwidth as usize].clone()
    }

    /// Gets the number in memory for UI to keep it persisted and set it again to a different instance
    /// of CCalcEngine. Otherwise it will get destructed with the CalcEngine.
    /// (Moves the value out, like the C++ `std::move` of the `unique_ptr`.)
    pub fn persisted_mem_object(&mut self) -> Option<Rational> {
        self.memory_value.take()
    }

    /// `PersistedMemObject(Rational const&)`
    pub fn set_persisted_mem_object(&mut self, mem_object: &Rational) {
        self.memory_value = Some(mem_object.clone());
    }

    pub fn f_in_error_state(&self) -> bool {
        self.b_error
    }

    pub fn is_input_empty(&self) -> bool {
        self.input.is_empty() && (self.number_string.is_empty() || self.number_string == "0")
    }

    pub fn f_in_recording_state(&self) -> bool {
        self.b_record
    }

    pub fn settings_changed(&mut self) -> CalcResult<()> {
        let last_dec = self.decimal_separator;
        let dec_str = self.resource_provider.get_cengine_string("sDecimal");
        self.decimal_separator = dec_str.chars().next().unwrap_or(DEFAULT_DEC_SEPARATOR);
        // Until it can be removed, continue to set ratpak decimal here
        ratpack::set_decimal_separator(self.decimal_separator);

        let last_sep = self.group_separator;
        let sep_str = self.resource_provider.get_cengine_string("sThousand");
        self.group_separator = sep_str.chars().next().unwrap_or(DEFAULT_GRP_SEPARATOR);

        let last_dec_grouping = self.dec_grouping.clone();
        let grp_str = self.resource_provider.get_cengine_string("sGrouping");
        self.dec_grouping = Self::digit_grouping_string_to_grouping_vector(if grp_str.is_empty() {
            DEFAULT_GRP_STR
        } else {
            &grp_str
        });

        let mut num_changed = false;

        // if the grouping pattern or thousands symbol changed we need to refresh the display
        if self.dec_grouping != last_dec_grouping || self.group_separator != last_sep {
            num_changed = true;
        }

        // if the decimal symbol has changed we always do the following things
        if self.decimal_separator != last_dec {
            // Re-initialize member variables' decimal point.
            self.input.set_decimal_symbol(self.decimal_separator);
            self.history_collector
                .set_decimal_symbol(self.decimal_separator);

            // put the new decimal symbol into the table used to draw the decimal key
            let dec = self.decimal_separator.to_string();
            S_ENGINE_STRINGS.with(|s| {
                s.borrow_mut()
                    .insert(SIDS_DECIMAL_SEPARATOR.to_string(), dec)
            });

            // we need to redraw to update the decimal point button
            num_changed = true;
        }

        if num_changed {
            self.display_num()?;
        }
        Ok(())
    }

    pub fn decimal_separator(&self) -> char {
        self.decimal_separator
    }

    /// Extension: reports the running expression to the display again (see
    /// `CalculatorManager::end_deferred_display`).
    pub(crate) fn refresh_expression_display(&mut self) {
        self.history_collector.refresh_expression_display();
    }

    pub fn get_history_collector_commands_snapshot(&self) -> Vec<ExpressionCommand> {
        let mut commands = self.history_collector.get_commands();
        if !self.history_collector.f_opnd_added_to_history() && self.b_record {
            commands.push(ExpressionCommand::Operand(
                self.history_collector
                    .get_operand_commands_from_string_rat(&self.number_string, &self.current_val),
            ));
        }
        commands
    }

    /// Extension: the state a saved session needs to continue as this one
    /// would. The display commands cover the pending expression and an
    /// operand being typed; they don't cover a value shown without being
    /// recorded (MR, π, a result, a typed number ended by F-E or MS), nor
    /// what another `=` repeats. Nothing in an error.
    pub fn continuation(&self) -> Continuation {
        if self.b_error {
            return Continuation::default();
        }
        // The current value is pending (not yet in the history) and not
        // being typed, and the last command neither started the expression
        // over (C, `(`: 0) nor was an operator (whose operand is recorded).
        // `(` keeps the value before it unless an operator came first: that
        // value isn't recorded either (the commands end with the `(`).
        let pending = !self.b_record && !self.history_collector.f_opnd_added_to_history();
        let kept_by_paren = self.n_temp_com == 0
            && !self.current_val.p().is_zero()
            && matches!(
                self.history_collector.last_command(),
                Some(ExpressionCommand::Parentheses(p)) if p.get_command() == IDC_OPENP
            );
        let shown = (pending
            && (kept_by_paren || (self.n_temp_com != 0 && !is_bin_op_code(self.n_temp_com))))
        .then(|| {
            if is_digit_op_code(self.n_temp_com) || self.n_temp_com == IDC_PNT {
                ShownValue::EndedEntry
            } else if self.n_temp_com == IDC_SIGN {
                ShownValue::EndedSign
            } else {
                ShownValue::Result
            }
        });
        let repeat = if !self.b_no_prev_equ && self.n_op_code != 0 {
            self.operand_for_text(&self.hold_val)
                .map(|operand| (self.n_op_code, operand))
        } else {
            None
        };
        let left = (!self.f_precedence && !self.b_change_op)
            .then(|| self.operand_for_text(&self.last_val))
            .flatten();
        let opnd_added = self.history_collector.f_opnd_added_to_history();
        let last = self.history_collector.last_command();
        let entry = if self.b_record {
            if opnd_added {
                None
            } else if self.is_input_empty()
                && !is_digit_op_code(self.n_temp_com)
                && self.n_temp_com != IDC_PNT
            {
                // The empty input's 0 (a leading 0 typed isn't kept either,
                // but leaves a digit as the last command; ⌫ doesn't change
                // the last command).
                Some(Entry::Empty)
            } else {
                (self.n_temp_com == IDC_SIGN).then_some(Entry::Signed)
            }
        } else if shown.is_some() {
            None
        } else if last.is_none() {
            Some(Entry::Ended)
        } else {
            (opnd_added
                && self.n_temp_com == IDC_PERCENT
                && matches!(last, Some(ExpressionCommand::Operand(_))))
            .then_some(Entry::Percent)
        };
        Continuation {
            shown,
            repeat,
            entry,
            clears: !self.b_no_prev_equ && self.n_op_code == 0,
            // A number typed after C or CE that starts with `Exp` keeps C
            // or CE as the last command, which typing it again can't.
            // Standard mode completes an equation at an operator even inside
            // parentheses (pasted ones), leaving them open in the engine but
            // not in the expression.
            unreplayable: self.history_collector.is_unreplayable()
                || self.history_collector.open_parentheses() != self.open_paren_count as i64
                || (self.b_record
                    && !opnd_added
                    && !self.is_input_empty()
                    && !is_digit_op_code(self.n_temp_com)
                    && self.n_temp_com != IDC_PNT
                    && self.n_temp_com != IDC_SIGN),
            carry: self.carry_bit != 0,
            left,
            empty_input: !self.b_record && self.input.is_empty(),
        }
    }

    /// Extension: empties the input, or keeps a number in it, while no
    /// number is being typed (see [`Continuation::empty_input`]), for a
    /// restored session. The kept number's digits are never read: the next
    /// digit or point starts the input over. Nothing is displayed.
    pub fn set_input_empty(&mut self, empty: bool) {
        if self.b_record || empty == self.input.is_empty() {
            return;
        }
        if empty {
            self.input.clear();
        } else {
            self.input.try_add_decimal_pt();
        }
    }

    /// Extension: `value` as the operand that typing it would give, for
    /// [`Continuation`]. Written out without F-E's e-notation where the
    /// number allows: typed as "1.21e+2", 121 isn't the integer it was
    /// ((−7)^x).
    fn operand_for_text(&self, value: &Rational) -> Option<OpndCommand> {
        let text = if self.f_integer_mode {
            self.get_string_for_display(value, self.radix)
        } else {
            value.to_string_radix(self.radix, NumberFormat::Float, self.precision)
        };
        text.ok().map(|text| {
            self.history_collector
                .get_operand_commands_from_string_rat(&text, value)
        })
    }

    /// Extension: sets the carry bit RoL and RoR through carry use (see
    /// [`Continuation::carry`]), for a restored session. Nothing else
    /// changes, and nothing is displayed.
    pub fn set_carry(&mut self, carry: bool) {
        self.carry_bit = u64::from(carry);
    }

    /// Extension: sets the left operand (see [`Continuation::left`]) to
    /// `operand` as typed (its digits, point, exponent and sign entered as
    /// keys would enter them), for a restored session. Nothing else
    /// changes, and nothing is displayed. Returns false, changing nothing,
    /// if the keys aren't a number this engine takes.
    pub fn set_left_operand(&mut self, operand: &OpndCommand) -> CalcResult<bool> {
        let mut input = CalcInput::new(self.decimal_separator);
        let max = self.get_max_decimal_value_string();
        // The sign follows the first command that isn't 0, as when the
        // view model replays an operand.
        let mut need_sign = operand.is_negative();
        for &command in operand.get_commands() {
            let typed = match command {
                IDC_PNT => input.try_add_decimal_pt(),
                IDC_EXP => !self.f_integer_mode && input.try_begin_exponent(),
                IDC_SIGN => input.try_toggle_sign(self.f_integer_mode, &max),
                digit if is_digit_op_code(digit) && ((digit - IDC_0) as u32) < self.radix => input
                    .try_add_digit(
                        (digit - IDC_0) as u32,
                        self.radix,
                        self.f_integer_mode,
                        &max,
                        self.dw_word_bit_width,
                        self.c_int_digits_sav,
                    ),
                _ => false,
            };
            let signed =
                !need_sign || command == IDC_0 || input.try_toggle_sign(self.f_integer_mode, &max);
            if !typed || !signed {
                return Ok(false);
            }
            need_sign &= command == IDC_0;
        }
        self.last_val = input.to_rational(self.radix, self.precision)?;
        Ok(true)
    }

    /// Extension: ends the number being typed and adds it to the expression
    /// the way `%` adds its result (`ProcessCommandWorker`'s unary branch),
    /// so a restored session whose display commands end with a `%` result
    /// continues as it would have. Does nothing unless a number is being
    /// typed.
    pub fn add_entry_as_percent_result(&mut self) -> CalcResult<()> {
        if !self.b_record || self.b_error {
            return Ok(());
        }
        self.b_record = false;
        self.current_val = self.input.to_rational(self.radix, self.precision)?;
        self.display_num()?;
        self.check_and_add_last_bin_op_to_history(true)?;
        self.history_collector
            .add_opnd_to_history(&self.number_string, &self.current_val, true);
        self.n_last_com = self.n_temp_com;
        self.n_temp_com = IDC_PERCENT;
        Ok(())
    }

    /// Extension: see `CalculatorManager::set_history_suppressed`.
    pub fn set_history_suppressed(&mut self, suppressed: bool) {
        self.history_collector.set_history_suppressed(suppressed);
    }

    pub fn change_precision(&mut self, precision: i32) {
        self.precision = precision;
        ratpack::change_constants(self.radix, precision);
    }

    // ------------------------------------------------------------------
    // Static string table access
    // ------------------------------------------------------------------

    /// `GetString(int ids)`
    pub fn get_string_id(ids: i32) -> String {
        Self::get_string(&ids.to_string())
    }

    /// `GetString(std::wstring_view ids)` — missing keys read back as `""`.
    pub fn get_string(ids: &str) -> String {
        S_ENGINE_STRINGS.with(|s| s.borrow().get(ids).cloned().unwrap_or_default())
    }

    /// returns the ptr to string representing the operator. Mostly same as the button, but few special cases for x^y etc.
    pub fn op_code_to_string(n_op_code: i32) -> String {
        Self::get_string_id(Self::id_str_from_cmd_id(n_op_code))
    }

    fn id_str_from_cmd_id(id: i32) -> i32 {
        id - IDC_FIRSTCONTROL + IDS_ENGINESTR_FIRST
    }

    /// Accessor for the radix (`m_radix`) — same as [`CalcEngine::get_current_radix`].
    pub fn radix(&self) -> u32 {
        self.radix
    }

    /// `m_precision`
    pub fn precision(&self) -> i32 {
        self.precision
    }

    /// `m_angletype`
    pub fn angle_type(&self) -> AngleType {
        self.angletype
    }

    /// `m_numwidth`
    pub fn num_width(&self) -> NumWidth {
        self.numwidth
    }

    /// `m_bInv`
    pub fn is_inv(&self) -> bool {
        self.b_inv
    }

    /// `m_nFE`
    pub fn number_format(&self) -> NumberFormat {
        self.n_fe
    }

    /// `m_openParenCount`
    pub fn open_paren_count(&self) -> usize {
        self.open_paren_count
    }

    /// Extension, for tests: the state a later key can read (see
    /// [`EngineState`]), with `memory` (the manager's slots) among the
    /// values. In an error only the error is reported: every key but C and
    /// CE is ignored, and they clear the rest.
    #[doc(hidden)]
    pub fn state(&self, memory: &[Rational]) -> EngineState {
        let mut state = EngineState::default();
        let mut exact = |name: &str, value: String| state.exact.push((name.to_string(), value));
        exact("error", self.b_error.to_string());
        if self.b_error {
            return state;
        }
        let opnd_added = self.history_collector.f_opnd_added_to_history();
        exact("carry", self.carry_bit.to_string());
        exact("operator", self.n_op_code.to_string());
        exact("change_op", self.b_change_op.to_string());
        exact("no_prev_equ", self.b_no_prev_equ.to_string());
        exact("record", self.b_record.to_string());
        exact("inv", self.b_inv.to_string());
        exact("radix", self.radix.to_string());
        exact("precision", self.precision.to_string());
        exact("width", format!("{:?}", self.numwidth));
        exact("angle", format!("{:?}", self.angletype));
        exact("fe", format!("{:?}", self.n_fe));
        exact("max_digits", self.c_int_digits_sav.to_string());
        exact("parens", self.open_paren_count.to_string());
        exact(
            "paren_ops",
            format!("{:?}", &self.n_op[..self.open_paren_count]),
        );
        exact(
            "precedence_ops",
            format!("{:?}", &self.n_prec_op[..self.precedence_op_count]),
        );
        exact("operand_added", opnd_added.to_string());
        // The last command as the code that reads it tells it apart: an
        // operator, a digit or point, ")", a unary operator (`%` separately,
        // see `Continuation::entry`), ± where a new number would complete
        // the expression before it (`CheckAndAddLastBinOpToHistory`), or
        // anything else (C, "(" and CE alike).
        let last = match self.n_temp_com {
            c if is_bin_op_code(c) => format!("binary {c}"),
            c if is_digit_op_code(c) || c == IDC_PNT => "digit".to_string(),
            IDC_SIGN if opnd_added && !self.b_change_op => "sign".to_string(),
            IDC_CLOSEP => "close".to_string(),
            IDC_PERCENT => "percent".to_string(),
            c if is_unary_op_code(c) => "unary".to_string(),
            _ => "other".to_string(),
        };
        exact("last_command", last);
        if self.b_record {
            exact("input", self.input.to_string(self.radix));
        } else {
            exact("input_empty", self.input.is_empty().to_string());
        }
        // Each value as held (Programmer mode: in 64 bits, so -1 and 2^64 - 1
        // are one value) and as the word size shows it.
        let qword = &self.chop_numbers[0];
        let held = |v: &Rational| -> Rational {
            if !self.f_integer_mode {
                return v.clone();
            }
            let in_64_bits = || -> CalcResult<Rational> {
                let mut r = rational_math::integer(v)?;
                if r < Rational::from(0) {
                    r = (-&r).sub(&Rational::from(1))?.bitxor(qword)?;
                }
                r.bitand(qword)
            };
            in_64_bits().unwrap_or_else(|_| v.clone())
        };
        let mut value = |name: String, v: &Rational| {
            let shown = self
                .truncate_num_for_int_math(v)
                .unwrap_or_else(|_| v.clone());
            state.values.push((name, held(v), shown));
        };
        if !self.b_record {
            value("current".into(), &self.current_val);
        }
        // `%` reads it in Standard mode (see `Continuation::left`); otherwise
        // only a pending operator does.
        if !self.f_precedence || self.b_change_op {
            value("left".into(), &self.last_val);
        }
        if !self.b_no_prev_equ {
            value("repeat".into(), &self.hold_val);
        }
        for i in 0..self.open_paren_count {
            if self.n_op[i] != 0 {
                value(format!("paren {i}"), &self.paren_vals[i]);
            }
        }
        for i in 0..self.precedence_op_count {
            if self.n_prec_op[i] != 0 {
                value(format!("precedence {i}"), &self.precedence_vals[i]);
            }
        }
        for (i, slot) in memory.iter().enumerate() {
            value(format!("memory {i}"), slot);
        }
        state
    }
}

/// Extension, for tests: what [`CalcEngine::state`] reports, to compare a
/// restored engine with the one saved. `exact` (flags, operators, modes,
/// the number being typed, what kind of key came last) must match;
/// `values` (the shown value, the operands the engine holds where a later
/// key reads them, the memory slots) may differ in the digits a restore
/// doesn't keep: each is the value held and the value as shown (in
/// Programmer mode, truncated to the word size).
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct EngineState {
    pub exact: Vec<(String, String)>,
    pub values: Vec<(String, Rational, Rational)>,
}
