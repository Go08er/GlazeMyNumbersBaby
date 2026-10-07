// Rust port: GMNB contributors.

//! Extension: randomized save/restore comparisons. Each script is a random
//! sequence of what a user can do in the apps (every key of the mode shown,
//! the History and Memory panels, pastes, mode switches, coming back from
//! another page); the state it leaves is saved, and a calculator restored
//! from it must show the same, save the same, hold the same engine state
//! and continue the same way as the original for every continuation below.
//! A restore that falls back to a new calculation from the shown value (see
//! the snapshot module docs) is counted separately and must show that value.
//!
//! Comparing what is saved can't find state the snapshot leaves out, and a
//! continuation only finds it if it reads it. So the restored engine's
//! state is compared with the original's too (`CalcEngine::state`: its
//! flags, pending operators, parentheses, carry, the kind of key that came
//! last, the number being typed and every value a later key reads, and the
//! memory slots).
//!
//! Values that only come back to the digits they showed (a memory slot, a
//! shown result, an operand of the expression) can differ from the
//! original's beyond those digits: that is the documented precision of a
//! restore. Such a value must agree with the original's as the engine
//! writes it out (or to 14 significant digits; a Programmer memory slot,
//! as the word size shows it); the restore is then counted as rounded, as
//! it is when the expression holds an operand that isn't the number its
//! digits type (1/3 shown as 0.3333333333333333, or −0), and a later
//! result that depends on what was lost (`1 ÷ 3 =`, then −
//! 0.3333333333333333) is counted too, not failed. Continuations of a
//! restore that kept every value exactly must agree but for the last
//! digits of numbers.
//!
//! The default test runs a few seconds' worth; the long run is
//! `cargo test -p calcvm --release -- --ignored random_sessions`
//! (`RESTORE_FUZZ_SEED`, `RESTORE_FUZZ_SCRIPTS`, and `RESTORE_FUZZ_OUT`, a
//! file the failures are written to, with the fallbacks and the
//! continuations of rounded restores that differed beyond the last digits
//! in `.fallbacks` and `.rounded` beside it).

use std::fmt::Write as _;

use calcmanager::{EngineState, Rational};

use crate::{Button, CalcMode, CalculatorViewModel, Radix, ShiftMode};

#[derive(Clone, Copy, Debug)]
enum Act {
    Key(Button),
    Recall(usize),
    ClearHistory,
    RemoveHistory(usize),
    Mode(CalcMode),
    Reactivate,
    MemoryItem(usize),
    SlotAdd(usize),
    SlotSubtract(usize),
    SlotClear(usize),
    Paste(&'static str),
    /// A shift-mode radio button (Programmer).
    Shift(ShiftMode),
    /// The left (`true`) or right shift key, which sends what the shift
    /// mode selects.
    ShiftKey(bool),
    /// A key of the bit-flip keypad.
    Flip(u32),
}

/// The two shift keys' buttons in a shift mode (`appcore::keys::shift_keys`,
/// which the apps and their `<`/`>` keys use).
fn shift_key(mode: ShiftMode, left: bool) -> Button {
    let (l, r) = match mode {
        ShiftMode::Arithmetic => (Button::Lsh, Button::Rsh),
        ShiftMode::Logical => (Button::Lsh, Button::RshL),
        ShiftMode::Rotate => (Button::Rol, Button::Ror),
        ShiftMode::RotateThroughCarry => (Button::RolC, Button::RorC),
    };
    if left { l } else { r }
}

fn act(vm: &mut CalculatorViewModel, acts: &[Act]) {
    for a in acts {
        match *a {
            // Outside Standard mode one key is C or, while a number is
            // being typed, CE; Esc presses it too.
            Act::Key(Button::Clear)
                if vm.mode() != CalcMode::Standard && vm.shows_clear_entry() =>
            {
                vm.press(Button::ClearEntry)
            }
            // Both apps ignore a key whose button is disabled.
            Act::Key(b) if !vm.is_enabled(b) => {}
            Act::Key(b) => vm.press(b),
            Act::Recall(i) => vm.history_recall(i),
            Act::ClearHistory => vm.history_clear(),
            Act::RemoveHistory(i) => vm.history_remove(i),
            // Both apps, as upstream's checkDefaultBitShift: the calculator
            // turned to Programmer from another mode starts on Arithmetic
            // shift (shown again, or after another page, it keeps its own).
            Act::Mode(m) => {
                if m == CalcMode::Programmer && vm.mode() != m {
                    vm.set_shift_mode(ShiftMode::Arithmetic);
                }
                vm.set_mode(m)
            }
            Act::Reactivate => vm.set_mode(vm.mode()),
            Act::MemoryItem(i) => vm.memory_recall(i),
            Act::SlotAdd(i) => vm.memory_add(i),
            Act::SlotSubtract(i) => vm.memory_subtract(i),
            Act::SlotClear(i) => vm.memory_clear(i),
            Act::Paste(text) => {
                vm.paste(text);
            }
            Act::Shift(s) => vm.set_shift_mode(s),
            Act::ShiftKey(left) => {
                let b = shift_key(vm.shift_mode(), left);
                if vm.is_enabled(b) {
                    vm.press(b);
                }
            }
            Act::Flip(bit) => vm.flip_bit(bit),
        }
    }
}

fn observed(vm: &CalculatorViewModel) -> String {
    let mut s = format!(
        "{:?} {:?} expression {:?} error {} parens {} ce {} fe {} {} {:?} {:?} {:?} {:?}\n\
         memory {:?}\nhistory {:?}",
        vm.mode(),
        vm.display_value(),
        vm.expression(),
        vm.is_error(),
        vm.open_parens(),
        vm.shows_clear_entry(),
        vm.is_fe(),
        vm.is_enabled(Button::FToE),
        vm.angle_unit(),
        vm.radix(),
        vm.word_size(),
        vm.shift_mode(),
        vm.memory(),
        vm.history(),
    );
    if vm.mode() == CalcMode::Programmer {
        for r in [Radix::Hex, Radix::Dec, Radix::Oct, Radix::Bin] {
            let _ = write!(s, " {}", vm.radix_value(r));
        }
    }
    s
}

/// The engine state a later key can read (see the module docs).
fn engine_state(vm: &CalculatorViewModel) -> EngineState {
    vm.vm.standard_calculator_manager.state()
}

/// Whether `a` and `b` agree to 14 significant digits.
fn close(a: &Rational, b: &Rational) -> bool {
    let zero = Rational::from(0);
    let abs = |r: &Rational| if *r < zero { -r } else { r.clone() };
    let Ok(difference) = a.sub(b) else {
        return false;
    };
    let larger = if abs(a) < abs(b) { abs(b) } else { abs(a) };
    abs(&difference)
        .mul(&Rational::from(10u64.pow(14)))
        .is_ok_and(|d| d <= larger)
}

/// Compares the restored engine's state with the original's: an error if
/// they differ in anything but values within [`close`], otherwise whether
/// a value came back rounded.
fn compare_states(original: &EngineState, restored: &EngineState) -> Result<bool, String> {
    if original.exact != restored.exact {
        let differences: Vec<String> = original
            .exact
            .iter()
            .zip(&restored.exact)
            .filter(|(o, r)| o != r)
            .map(|(o, r)| format!("{} {} (restored {} {})", o.0, o.1, r.0, r.1))
            .collect();
        return Err(format!(
            "engine state differs: {}{}",
            differences.join(", "),
            if original.exact.len() == restored.exact.len() {
                String::new()
            } else {
                format!(" {:?} / {:?}", original.exact, restored.exact)
            }
        ));
    }
    let names = |s: &EngineState| s.values.iter().map(|v| v.0.clone()).collect::<Vec<_>>();
    if names(original) != names(restored) {
        return Err(format!(
            "engine values differ: {:?} (restored {:?})",
            names(original),
            names(restored)
        ));
    }
    // Operands the expression only holds as shown make what was worked out
    // from them differ as far as it may (see `EngineState::inexact`).
    let mut rounded = original.inexact > 0;
    let precision: i32 = original
        .exact
        .iter()
        .find(|(name, _)| name == "precision")
        .and_then(|(_, p)| p.parse().ok())
        .unwrap_or(32);
    // As the engine writes it out (the digits a display can show).
    let written = |v: &Rational| {
        v.to_string_radix(10, calcmanager::NumberFormat::Float, precision)
            .ok()
    };
    for ((name, o, o_shown), (_, r, r_shown)) in original.values.iter().zip(&restored.values) {
        if o == r {
            continue;
        }
        // Rounded to the digits shown, or (Programmer mode) a memory slot
        // stored in a larger word size, back as the word size showed it.
        if !close(o, r)
            && written(o) != written(r)
            && !(name.starts_with("memory") && o_shown == r_shown)
            && original.inexact == 0
        {
            let show = |v: &Rational| {
                v.to_string_radix(10, calcmanager::NumberFormat::Scientific, 20)
                    .unwrap_or_default()
            };
            return Err(format!(
                "engine value {name} differs: {} (restored {})",
                show(o),
                show(r)
            ));
        }
        rounded = true;
    }
    Ok(rounded)
}

/// A saved state, without the display commands if it is an error the
/// engine is in: the restore replays them only up to the error (what came
/// after it in the original is dropped), and every key clears it anyway.
fn without_engine_error_commands(state: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(state).unwrap();
    let engine_error =
        v["s"]["p"]["e"] == serde_json::Value::Bool(true) && v["x"]["k"]["ev"].is_null();
    if engine_error {
        v["s"]["c"] = serde_json::Value::Null;
    }
    v.to_string()
}

/// The numbers in `s` (digits with "." and group separators, and an
/// exponent), each with the text before it.
fn numbers(s: &str) -> (Vec<String>, Vec<f64>) {
    let chars: Vec<char> = s.chars().collect();
    let (mut texts, mut values) = (vec![String::new()], Vec::new());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | ',')) {
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == 'e' && matches!(chars[i + 1], '+' | '-') {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let n: String = chars[start..i].iter().filter(|&&c| c != ',').collect();
            values.push(n.parse().unwrap_or(f64::NAN));
            texts.push(String::new());
        } else {
            texts.last_mut().unwrap().push(chars[i]);
            i += 1;
        }
    }
    (texts, values)
}

/// Whether `a` and `b` differ only in the last digits of numbers: a value
/// that only came back to the digits it showed (a memory slot, a shown
/// result) can round differently later. That is the documented precision
/// of a restore, not a different continuation.
fn differ_in_precision_only(a: &str, b: &str) -> bool {
    let ((ta, va), (tb, vb)) = (numbers(a), numbers(b));
    ta == tb
        && va.len() == vb.len()
        && va
            .iter()
            .zip(&vb)
            .all(|(x, y)| x == y || (x - y).abs() <= 1e-12 * x.abs().max(y.abs()))
}

/// What a user can do in `mode`: every key the apps show in it
/// (`appcore::keys`: the keypad of the mode, the 2nd functions, the
/// trigonometry, function and bitwise flyouts, the angle, F-E, radix, word
/// size and shift-mode buttons, the bit-flip keypad), the memory buttons
/// (their Ctrl shortcuts work in every mode), the Memory panel and, but in
/// Programmer mode, the History panel, pastes, the mode switches and coming
/// back from another page. `act` skips a key the apps disable at the time
/// (a digit the radix doesn't have, "." in Programmer mode, operators in an
/// error, F-E right after a History selection), as the apps ignore it.
///
/// Left out: Rand, whose value is random, so the uninterrupted run each
/// continuation is compared with (the script played again) can't repeat
/// it. Neither app sends Hyp (its toggle picks the hyperbolic button), and
/// `%` is a Standard key only (elsewhere it types Mod).
///
/// The keys that type and combine numbers appear twice in the larger
/// pools, so scripts still build numbers between the functions.
fn pool(mode: CalcMode) -> Vec<Act> {
    use Act::*;
    use Button::*;
    let digits = [Zero, One, Two, Three, Four, Five, Six, Seven, Eight, Nine];
    let mut core: Vec<Act> = digits.iter().map(|&d| Key(d)).collect();
    core.extend(
        [
            Add, Subtract, Multiply, Divide, Equals, Equals, Negate, Backspace,
        ]
        .into_iter()
        .map(Key),
    );
    if mode != CalcMode::Programmer {
        core.push(Key(Decimal));
    } else {
        core.extend([A, B, C, D, E, F].into_iter().map(Key));
    }
    let mut p = core.clone();
    if mode != CalcMode::Standard {
        p.extend(core);
    }
    p.extend([
        Key(ClearEntry),
        Key(Clear),
        Key(Memory),
        Key(MemoryRecall),
        Key(MemoryAdd),
        Key(MemorySubtract),
        Key(MemoryClear),
        MemoryItem(0),
        MemoryItem(1),
        SlotAdd(1),
        SlotSubtract(0),
        SlotClear(0),
        Paste("12"),
        Paste("zz"),
        Paste("3+4"),
        Reactivate,
    ]);
    if mode != CalcMode::Programmer {
        p.extend([Recall(0), Recall(1), ClearHistory, RemoveHistory(0)]);
    }
    match mode {
        CalcMode::Standard => p.extend([
            Key(Percent),
            Key(Invert),
            Key(XPower2),
            Key(Sqrt),
            Paste("-0.5"),
            Paste("1e5"),
            Mode(CalcMode::Scientific),
            Mode(CalcMode::Programmer),
        ]),
        CalcMode::Scientific => {
            p.extend(
                [
                    OpenParenthesis,
                    CloseParenthesis,
                    Mod,
                    Exp,
                    Factorial,
                    XPower2,
                    Cube,
                    Invert,
                    Abs,
                    Sqrt,
                    CubeRoot,
                    XPowerY,
                    YRootX,
                    TenPowerX,
                    TwoPowerX,
                    LogBase10,
                    LogBaseY,
                    LogBaseE,
                    EPowerX,
                    Pi,
                    Euler,
                    FToE,
                    Degree,
                    Radians,
                    Grads,
                    Sin,
                    Cos,
                    Tan,
                    Sec,
                    Csc,
                    Cot,
                    InvSin,
                    InvCos,
                    InvTan,
                    InvSec,
                    InvCsc,
                    InvCot,
                    Sinh,
                    Cosh,
                    Tanh,
                    Sech,
                    Csch,
                    Coth,
                    InvSinh,
                    InvCosh,
                    InvTanh,
                    InvSech,
                    InvCsch,
                    InvCoth,
                    Floor,
                    Ceil,
                    DMS,
                    Degrees,
                ]
                .into_iter()
                .map(Key),
            );
            p.extend([
                Paste("(1+2"),
                Paste("2^3"),
                Mode(CalcMode::Standard),
                Mode(CalcMode::Programmer),
            ]);
        }
        CalcMode::Programmer => {
            p.extend(
                [
                    OpenParenthesis,
                    CloseParenthesis,
                    Mod,
                    And,
                    Or,
                    Xor,
                    Not,
                    Nand,
                    Nor,
                    HexButton,
                    DecButton,
                    OctButton,
                    BinButton,
                    Qword,
                    Dword,
                    Word,
                    Byte,
                ]
                .into_iter()
                .map(Key),
            );
            p.extend([
                ShiftKey(true),
                ShiftKey(false),
                ShiftKey(true),
                ShiftKey(false),
                Shift(ShiftMode::Arithmetic),
                Shift(ShiftMode::Logical),
                Shift(ShiftMode::Rotate),
                Shift(ShiftMode::RotateThroughCarry),
                Flip(0),
                Flip(1),
                Flip(7),
                Flip(15),
                Flip(31),
                Flip(63),
                Paste("(1+2"),
                Paste("FF"),
                Mode(CalcMode::Standard),
                Mode(CalcMode::Scientific),
            ]);
        }
    }
    p
}

/// The continuations every saved state is checked with: each reads some of
/// the state a restore must bring back (the number being typed, the
/// pending operators and parentheses, what "=" repeats, memory, the left
/// operand `%` reads, the carry, the modes).
fn continuations(mode: CalcMode) -> Vec<Vec<Act>> {
    use Act::*;
    use Button::*;
    let mut c: Vec<Vec<Act>> = vec![
        vec![Key(Seven)],
        vec![Key(Seven), Key(Equals)],
        vec![Key(Equals)],
        vec![Key(Equals), Key(Equals)],
        vec![Key(Add), Key(Two), Key(Equals)],
        vec![Key(Multiply), Key(Equals)],
        vec![Key(Subtract), Key(Two), Key(Multiply), Key(Equals)],
        vec![Key(Backspace), Key(Seven), Key(Equals)],
        vec![Key(MemoryRecall), Key(Equals)],
        vec![Key(Negate), Key(Equals)],
        vec![Key(Seven), Key(Negate), Key(Equals)],
        vec![MemoryItem(1), Key(Equals)],
        vec![Paste("12"), Key(Equals)],
        vec![Reactivate, Key(Equals)],
        vec![Key(Memory), MemoryItem(0), Key(Equals)],
        vec![Key(ClearEntry), Key(Equals)],
    ];
    match mode {
        CalcMode::Standard => c.extend([
            vec![Recall(0), Key(Equals)],
            vec![Key(Percent)],
            vec![Key(Seven), Key(Percent)],
            vec![Key(Percent), Key(Equals)],
            vec![Key(Decimal), Key(Seven), Key(Equals)],
            vec![Key(Sqrt), Key(Equals)],
        ]),
        CalcMode::Scientific => c.extend([
            vec![Recall(0), Key(Equals)],
            vec![Key(OpenParenthesis), Key(Two), Key(Equals)],
            vec![Key(CloseParenthesis), Key(Equals)],
            vec![Key(FToE), Key(Equals)],
            vec![Key(Decimal), Key(Seven), Key(Equals)],
            vec![Key(Exp), Key(Seven), Key(Equals)],
            vec![Key(XPowerY), Key(Two), Key(Equals)],
            vec![Key(Sin), Key(Equals)],
        ]),
        CalcMode::Programmer => c.extend([
            vec![Key(OpenParenthesis), Key(Two), Key(Equals)],
            vec![Key(CloseParenthesis), Key(Equals)],
            vec![Key(HexButton), Key(Equals)],
            vec![Key(A), Key(Equals)],
            vec![ShiftKey(true), Key(One), Key(Equals)],
            vec![Shift(ShiftMode::RotateThroughCarry), ShiftKey(false)],
            vec![
                Shift(ShiftMode::RotateThroughCarry),
                ShiftKey(true),
                ShiftKey(true),
            ],
            vec![Flip(0), Key(Equals)],
            vec![Key(Byte), Key(Equals)],
            vec![Key(Qword), Key(Equals)],
        ]),
    }
    c
}

/// xorshift64: the same scripts on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Script `index` of the run seeded `seed`: a starting mode and 2–13
/// actions (switching modes changes what the following ones can be).
fn script(seed: u64, index: u64) -> (CalcMode, Vec<Act>) {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0x1000_0000_01B3) ^ index);
    for _ in 0..4 {
        rng.next();
    }
    let start = [
        CalcMode::Standard,
        CalcMode::Scientific,
        CalcMode::Programmer,
    ][rng.below(3)];
    let mut mode = start;
    let len = 2 + rng.below(12);
    let mut acts = Vec::with_capacity(len);
    for _ in 0..len {
        let p = pool(mode);
        let a = p[rng.below(p.len())];
        if let Act::Mode(m) = a {
            mode = m;
        }
        acts.push(a);
    }
    (start, acts)
}

#[derive(Default, Debug)]
struct Counts {
    scripts: usize,
    exact: usize,
    fell_back: usize,
    precision: usize,
    /// Restores that brought a value back rounded to the digits it showed.
    rounded: usize,
    /// Continuations of those that differed by more than the last digits.
    rounding_dependent: Vec<String>,
    fallbacks: Vec<String>,
    failures: Vec<String>,
}

fn run(mode: CalcMode, acts: &[Act], more: &[Act]) -> (String, String, EngineState, String) {
    let mut vm = CalculatorViewModel::new();
    vm.set_mode(mode);
    act(&mut vm, acts);
    let saved = vm.save_state();
    let before = observed(&vm);
    let state = engine_state(&vm);
    act(&mut vm, more);
    (saved, before, state, observed(&vm))
}

/// Checks one script; the original finishes before the restored
/// calculator is made (calculators on one thread share the engine's
/// display cache), as in the apps, which restore at startup.
fn check(start: CalcMode, acts: &[Act], counts: &mut Counts) {
    counts.scripts += 1;
    let (saved, before, original_state, _) = run(start, acts, &[]);
    let (display, error) = {
        let mut original = CalculatorViewModel::new();
        original.set_mode(start);
        act(&mut original, acts);
        (original.display_value(), original.is_error())
    };
    let mut restored = CalculatorViewModel::new();
    let exact = restored.restore_state_checked(&saved);
    let fail = |why: String| format!("{start:?} {acts:?}\n{why}\nsaved {saved}");
    if !exact {
        // A new calculation from the value shown: the same display, an
        // empty expression; "=" repeats nothing and a digit (1, valid in
        // every radix) replaces it.
        counts.fell_back += 1;
        counts
            .fallbacks
            .push(format!("{start:?} {acts:?}\nsaved {saved}"));
        let shown = observed(&restored);
        let mut why = None;
        if restored.display_value() != display
            || restored.is_error() != error
            || !restored.expression().is_empty()
        {
            why = Some("shows");
        } else if !error {
            restored.press(Button::Equals);
            // The same value (F-E may write it differently once entered).
            if numbers(&restored.display_value()).1 != numbers(&display).1 {
                why = Some("= gives");
            }
            drop(restored);
            let mut restored = CalculatorViewModel::new();
            restored.restore_state(&saved);
            restored.press(Button::One);
            if numbers(&restored.display_value()).1 != [1.0] {
                why = Some("1 gives");
            }
        }
        if let Some(why) = why {
            counts.failures.push(fail(format!(
                "fell back wrongly ({why})\n  fell back to {shown}\n  original {before}"
            )));
        }
        return;
    }
    let restored_now = observed(&restored);
    if restored_now != before {
        counts.failures.push(fail(format!(
            "restored {restored_now}\n  original {before}"
        )));
        return;
    }
    let resaved = restored.save_state();
    if without_engine_error_commands(&resaved) != without_engine_error_commands(&saved) {
        counts.failures.push(fail(format!("re-saved {resaved}")));
        return;
    }
    let rounded = match compare_states(&original_state, &engine_state(&restored)) {
        Ok(rounded) => rounded,
        Err(why) => {
            counts.failures.push(fail(why));
            return;
        }
    };
    counts.rounded += usize::from(rounded);
    let mode = restored.mode();
    drop(restored);
    for more in continuations(mode) {
        let (_, _, _, expected) = run(start, acts, &more);
        let mut restored = CalculatorViewModel::new();
        restored.restore_state(&saved);
        act(&mut restored, &more);
        let actual = observed(&restored);
        if actual == expected {
            continue;
        }
        if differ_in_precision_only(&actual, &expected) {
            counts.precision += 1;
        } else if rounded {
            counts.rounding_dependent.push(fail(format!(
                "then {more:?}\n  expected {expected}\n  restored {actual}"
            )));
        } else {
            counts.failures.push(fail(format!(
                "then {more:?}\n  expected {expected}\n  restored {actual}"
            )));
            return;
        }
    }
    counts.exact += 1;
}

/// Runs scripts `0..n` of `seed` on `threads` threads.
fn fuzz(seed: u64, n: u64, threads: u64) -> Counts {
    let results: Vec<Counts> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                scope.spawn(move || {
                    let mut counts = Counts::default();
                    let mut i = t;
                    while i < n {
                        let (start, acts) = script(seed, i);
                        check(start, &acts, &mut counts);
                        i += threads;
                    }
                    counts
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut total = Counts::default();
    for c in results {
        total.scripts += c.scripts;
        total.exact += c.exact;
        total.fell_back += c.fell_back;
        total.precision += c.precision;
        total.rounded += c.rounded;
        total.rounding_dependent.extend(c.rounding_dependent);
        total.failures.extend(c.failures);
        total.fallbacks.extend(c.fallbacks);
    }
    total
}

fn assert_clean(counts: &Counts) {
    if let Ok(path) = std::env::var("RESTORE_FUZZ_OUT") {
        let _ = std::fs::write(&path, counts.failures.join("\n\n"));
        let _ = std::fs::write(path.clone() + ".fallbacks", counts.fallbacks.join("\n\n"));
        let _ = std::fs::write(path + ".rounded", counts.rounding_dependent.join("\n\n"));
    }
    println!(
        "{} scripts: {} restored exactly ({} with a value rounded to the digits it \
         showed; {} continuations differing only in the last digits, {} more of the \
         rounded ones differing beyond), {} fell back to the shown value, {} failed",
        counts.scripts,
        counts.exact,
        counts.rounded,
        counts.precision,
        counts.rounding_dependent.len(),
        counts.fell_back,
        counts.failures.len()
    );
    assert!(
        counts.failures.is_empty(),
        "{} of {} failed; the first:\n{}",
        counts.failures.len(),
        counts.scripts,
        counts.failures[0]
    );
}

fn threads() -> u64 {
    std::thread::available_parallelism().map_or(4, |n| n.get() as u64)
}

/// A few seconds' worth of scripts on every test run.
#[test]
fn random_sessions_restore_and_continue() {
    assert_clean(&fuzz(1, 96, threads()));
}

/// The long run (`cargo test -p calcvm --release -- --ignored`): 20,000
/// scripts. `RESTORE_FUZZ_SEED` and `RESTORE_FUZZ_SCRIPTS` change them.
#[test]
#[ignore]
fn random_sessions_restore_and_continue_long() {
    let env = |k: &str, d: u64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let counts = fuzz(
        env("RESTORE_FUZZ_SEED", 2),
        env("RESTORE_FUZZ_SCRIPTS", 20_000),
        threads(),
    );
    assert_clean(&counts);
}
