// Rust port: GMNB contributors.

//! Extension: randomized save/restore comparisons. Each script is a random
//! sequence of what a user can do in the apps (keys of the mode shown, the
//! History and Memory panels, pastes, mode switches, coming back from
//! another page); the state it leaves is saved, and a calculator restored
//! from it must show the same, save the same and continue the same way as
//! the original for every continuation below. A restore that falls back to
//! a new calculation from the shown value (see the snapshot module docs) is
//! counted separately and must show that value.
//!
//! Values that only come back to the digits they showed (a memory slot, a
//! shown result) can round differently later: that is the documented
//! precision of a restore, so continuations that differ only in the last
//! digits of numbers are counted, not failed, and the scripts avoid the
//! keys that make such values common (π, e, roots, trigonometry, division
//! outside Programmer mode).
//!
//! The default test runs a few seconds' worth; the long run is
//! `cargo test -p calcvm --release -- --ignored random_sessions`
//! (`RESTORE_FUZZ_SEED`, `RESTORE_FUZZ_SCRIPTS`, and `RESTORE_FUZZ_OUT`, a
//! file the failures and fallbacks are written to).

use std::fmt::Write as _;

use crate::{Button, CalcMode, CalculatorViewModel, Radix};

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
}

fn act(vm: &mut CalculatorViewModel, acts: &[Act]) {
    for a in acts {
        match *a {
            // Both apps ignore a key whose button is disabled.
            Act::Key(b) if !vm.is_enabled(b) => {}
            Act::Key(b) => vm.press(b),
            Act::Recall(i) => vm.history_recall(i),
            Act::ClearHistory => vm.history_clear(),
            Act::RemoveHistory(i) => vm.history_remove(i),
            Act::Mode(m) => vm.set_mode(m),
            Act::Reactivate => vm.set_mode(vm.mode()),
            Act::MemoryItem(i) => vm.memory_recall(i),
            Act::SlotAdd(i) => vm.memory_add(i),
            Act::SlotSubtract(i) => vm.memory_subtract(i),
            Act::SlotClear(i) => vm.memory_clear(i),
            Act::Paste(text) => {
                vm.paste(text);
            }
        }
    }
}

fn observed(vm: &CalculatorViewModel) -> String {
    let mut s = format!(
        "{:?} {:?} expression {:?} error {} parens {} fe {} {} {:?} {:?} {:?} {:?}\n\
         memory {:?}\nhistory {:?}",
        vm.mode(),
        vm.display_value(),
        vm.expression(),
        vm.is_error(),
        vm.open_parens(),
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

/// What a user can do in `mode`.
fn pool(mode: CalcMode) -> Vec<Act> {
    use Act::*;
    use Button::*;
    let mut p = vec![
        Key(Zero),
        Key(One),
        Key(Two),
        Key(Three),
        Key(Five),
        Key(Nine),
        Key(Add),
        Key(Subtract),
        Key(Multiply),
        Key(Equals),
        Key(Equals),
        Key(Negate),
        Key(Backspace),
        Key(ClearEntry),
        Key(Clear),
        Key(Memory),
        Key(MemoryRecall),
        Key(MemoryAdd),
        Key(MemorySubtract),
        MemoryItem(0),
        MemoryItem(1),
        SlotAdd(1),
        SlotSubtract(0),
        SlotClear(0),
        Paste("12"),
        Paste("zz"),
        Paste("3+4"),
        Reactivate,
    ];
    match mode {
        CalcMode::Standard | CalcMode::Scientific => {
            p.extend([
                Key(Decimal),
                Paste("(1+2"),
                Key(Percent),
                Key(XPower2),
                Recall(0),
                Recall(1),
                ClearHistory,
                RemoveHistory(0),
            ]);
            let other = if mode == CalcMode::Standard {
                CalcMode::Scientific
            } else {
                CalcMode::Standard
            };
            p.extend([Mode(other), Mode(CalcMode::Programmer)]);
            if mode == CalcMode::Scientific {
                p.extend([
                    Key(OpenParenthesis),
                    Key(CloseParenthesis),
                    Key(FToE),
                    Key(XPowerY),
                    Key(Mod),
                    Key(Factorial),
                    Key(Exp),
                    Key(Radians),
                    Key(Degree),
                ]);
            }
        }
        CalcMode::Programmer => p.extend([
            Key(A),
            Key(F),
            Key(Divide),
            Key(Mod),
            Key(And),
            Key(Or),
            Key(Xor),
            Key(Not),
            Key(Lsh),
            Key(Rsh),
            Key(OpenParenthesis),
            Key(CloseParenthesis),
            Key(HexButton),
            Key(DecButton),
            Key(BinButton),
            Key(Byte),
            Key(Qword),
            Mode(CalcMode::Standard),
            Mode(CalcMode::Scientific),
        ]),
    }
    p
}

/// The continuations every saved state is checked with.
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
        CalcMode::Standard => c.push(vec![Recall(0), Key(Equals)]),
        CalcMode::Scientific => c.extend([
            vec![Recall(0), Key(Equals)],
            vec![Key(OpenParenthesis), Key(Two), Key(Equals)],
            vec![Key(CloseParenthesis), Key(Equals)],
            vec![Key(FToE), Key(Equals)],
        ]),
        CalcMode::Programmer => c.extend([
            vec![Key(OpenParenthesis), Key(Two), Key(Equals)],
            vec![Key(CloseParenthesis), Key(Equals)],
            vec![Key(HexButton), Key(Equals)],
            vec![Key(A), Key(Equals)],
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
    fallbacks: Vec<String>,
    failures: Vec<String>,
}

fn run(mode: CalcMode, acts: &[Act], more: &[Act]) -> (String, String, String) {
    let mut vm = CalculatorViewModel::new();
    vm.set_mode(mode);
    act(&mut vm, acts);
    let saved = vm.save_state();
    let before = observed(&vm);
    act(&mut vm, more);
    (saved, before, observed(&vm))
}

/// Checks one script; the original finishes before the restored
/// calculator is made (calculators on one thread share the engine's
/// display cache), as in the apps, which restore at startup.
fn check(start: CalcMode, acts: &[Act], counts: &mut Counts) {
    counts.scripts += 1;
    let (saved, before, _) = run(start, acts, &[]);
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
    let mode = restored.mode();
    for more in continuations(mode) {
        let (_, _, expected) = run(start, acts, &more);
        let mut restored = CalculatorViewModel::new();
        restored.restore_state(&saved);
        act(&mut restored, &more);
        let actual = observed(&restored);
        if actual != expected && differ_in_precision_only(&actual, &expected) {
            counts.precision += 1;
        } else if actual != expected {
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
        total.failures.extend(c.failures);
        total.fallbacks.extend(c.fallbacks);
    }
    total
}

fn assert_clean(counts: &Counts) {
    if let Ok(path) = std::env::var("RESTORE_FUZZ_OUT") {
        let _ = std::fs::write(&path, counts.failures.join("\n\n"));
        let _ = std::fs::write(path + ".fallbacks", counts.fallbacks.join("\n\n"));
    }
    println!(
        "{} scripts: {} restored exactly ({} continuations differing only in \
         the last digits), {} fell back to the shown value, {} failed",
        counts.scripts,
        counts.exact,
        counts.precision,
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
