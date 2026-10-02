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
use crate::ccommand::*;
use crate::engine_strings::*;
use crate::expression_command::ExpressionCommand;
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
}
