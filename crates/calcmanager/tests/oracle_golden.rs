// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Differential test against the real C++ CalcManager.
//!
//! `tests/data/golden_*.txt` were produced by the C++ oracle in
//! `tools/oracle/calcmanager` (driver.cpp, regenerate with `gen.sh`). Each
//! sequence lists the operations that were executed (`> ...` lines) and
//! every display callback / query result the C++ engine produced. This test
//! replays the operations against the Rust port, re-serializes the callbacks
//! in the same format and requires exact equality.
//!
//! Every sequence runs on a fresh thread so that the thread-local engine and
//! ratpack state is pristine, matching the oracle's fork-per-sequence.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use calcmanager::*;

// ---------------------------------------------------------------------------
// Serialization (must match driver.cpp byte for byte)
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '|' => out.push_str("\\p"),
            ';' => out.push_str("\\s"),
            _ => out.push(c),
        }
    }
    out
}

fn join_ints(v: &[i32]) -> String {
    v.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Operand command lists are mostly digit ops; encoded compactly:
/// IDC_0..IDC_F -> 0-9A-F, IDC_PNT -> '.', IDC_EXP -> 'e', IDC_SIGN -> '-', else `{n}`.
fn compact_opnd(v: &[i32]) -> String {
    let mut s = String::new();
    for &c in v {
        match c {
            130..=145 => s.push(b"0123456789ABCDEF"[(c - 130) as usize] as char),
            84 => s.push('.'),
            127 => s.push('e'),
            80 => s.push('-'),
            _ => {
                let _ = write!(s, "{{{c}}}");
            }
        }
    }
    s
}

fn serialize_command(cmd: &ExpressionCommand) -> String {
    match cmd {
        ExpressionCommand::Parentheses(p) => format!("({}", p.get_command()),
        ExpressionCommand::Unary(u) => format!("U{}", join_ints(u.get_commands())),
        ExpressionCommand::Binary(b) => format!("B{}", b.get_command()),
        ExpressionCommand::Operand(o) => {
            let mut o = o.clone();
            format!(
                "O{}:{}{}{}:{}",
                compact_opnd(o.get_commands()),
                if o.is_negative() { '1' } else { '0' },
                if o.is_decimal_present() { '1' } else { '0' },
                if o.is_sci_fmt() { '1' } else { '0' },
                esc(&o.get_token('.'))
            )
        }
    }
}

fn serialize_commands(cmds: &[ExpressionCommand]) -> String {
    cmds.iter()
        .map(serialize_command)
        .collect::<Vec<_>>()
        .join(";")
}

fn serialize_tokens(tokens: &[ExpressionToken]) -> String {
    tokens
        .iter()
        .map(|(t, i)| format!("{}@{}", esc(t), i))
        .collect::<Vec<_>>()
        .join("|")
}

// ---------------------------------------------------------------------------
// Recording display
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RecordingDisplay {
    out: String,
    is_in_error: bool,
}

impl CalcDisplay for RecordingDisplay {
    fn set_primary_display(&mut self, text: &str, is_error: bool) {
        let _ = writeln!(
            self.out,
            "P\t{}\t{}",
            esc(text),
            if is_error { 1 } else { 0 }
        );
    }
    fn set_is_in_error(&mut self, is_in_error: bool) {
        self.is_in_error = is_in_error;
        let _ = writeln!(self.out, "E\t{}", if is_in_error { 1 } else { 0 });
    }
    fn set_expression_display(
        &mut self,
        tokens: &[ExpressionToken],
        commands: &[ExpressionCommand],
    ) {
        let _ = writeln!(
            self.out,
            "X\t{}\t{}",
            serialize_tokens(tokens),
            serialize_commands(commands)
        );
    }
    fn set_parenthesis_number(&mut self, count: u32) {
        let _ = writeln!(self.out, "N\t{count}");
    }
    fn on_no_right_paren_added(&mut self) {
        self.out.push_str("R\n");
    }
    fn max_digits_reached(&mut self) {
        self.out.push_str("D\n");
    }
    fn binary_operator_received(&mut self) {
        self.out.push_str("B\n");
    }
    fn on_history_item_added(&mut self, added_item_index: u32) {
        let _ = writeln!(self.out, "H\t{added_item_index}");
    }
    fn set_memorized_numbers(&mut self, memorized_numbers: &[String]) {
        let s = memorized_numbers
            .iter()
            .map(|n| esc(n))
            .collect::<Vec<_>>()
            .join("|");
        let _ = writeln!(self.out, "M\t{s}");
    }
    fn memory_item_changed(&mut self, index_of_memory: u32) {
        let _ = writeln!(self.out, "C\t{index_of_memory}");
    }
    fn input_changed(&mut self) {
        self.out.push_str("I\n");
    }
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

struct Session {
    display: Rc<RefCell<RecordingDisplay>>,
    mgr: CalculatorManager,
}

impl Session {
    fn new(suite: &str) -> Self {
        let display = Rc::new(RefCell::new(RecordingDisplay::default()));
        let mut provider = EngineResourceProvider::default();
        if suite == "loc" {
            // matches driver.cpp's OracleResourceProvider for the "loc" suite
            provider.decimal_separator = ",".to_string();
            provider.thousands_separator = ".".to_string();
            provider.grouping = "3;2;0".to_string();
        }
        let provider: Rc<dyn ResourceProvider> = Rc::new(provider);
        let mgr = CalculatorManager::new(display.clone(), provider);
        Session { display, mgr }
    }

    fn emit(&self, line: &str) {
        let mut d = self.display.borrow_mut();
        d.out.push_str(line);
        d.out.push('\n');
    }

    fn history_dump(&self, items: &[Rc<HistoryItem>]) {
        let mut s = format!("=\t{}\n", items.len());
        for item in items {
            let v = &item.history_item_vector;
            let _ = writeln!(
                s,
                "=\t{}\t{}\t{}\t{}",
                esc(&v.expression),
                esc(&v.result),
                serialize_tokens(&v.tokens),
                serialize_commands(&v.commands)
            );
        }
        self.display.borrow_mut().out.push_str(&s);
    }

    fn exec(&mut self, op: &[&str]) -> CalcResult<()> {
        let arg = |i: usize| -> i64 { op[i].parse().expect("numeric op argument") };
        let mgr = &mut self.mgr;
        match op[0] {
            "SC" => mgr.send_command(Command(arg(1) as i32))?,
            "MS" => mgr.memorize_number()?,
            "ML" => mgr.memorized_number_load(arg(1) as u32)?,
            "MA" => mgr.memorized_number_add(arg(1) as u32)?,
            "MSUB" => mgr.memorized_number_subtract(arg(1) as u32)?,
            "MC" => mgr.memorized_number_clear(arg(1) as u32),
            "MCA" => mgr.memorized_number_clear_all()?,
            "RESET" => mgr.reset(arg(1) != 0)?,
            "STD" => mgr.set_standard_mode()?,
            "SCI" => mgr.set_scientific_mode()?,
            "PROG" => mgr.set_programmer_mode()?,
            "RADIX" => mgr.set_radix(RadixType::from_index(arg(1) as i32).expect("radix"))?,
            "PREC" => mgr.set_precision(arg(1) as i32),
            "UMID" => mgr.update_max_int_digits(),
            "MNS" => mgr.set_memorized_numbers_string()?,
            "HLOAD" => mgr.set_in_history_item_load_mode(arg(1) != 0),
            "HRM" => {
                let removed = mgr.remove_history_item(arg(1) as u32);
                self.emit(&format!("=\t{}", if removed { 1 } else { 0 }));
            }
            "HCLR" => mgr.clear_history(),
            "HSET" => {
                let mode = if arg(1) == 0 {
                    CalculatorMode::Standard
                } else {
                    CalculatorMode::Scientific
                };
                let items = mgr.get_history_items_for_mode(mode);
                mgr.set_history_items(&items);
            }
            "PASTEERR" => mgr.display_paste_error(),
            "GRR" => {
                let r = mgr.get_result_for_radix(arg(1) as u32, arg(2) as i32, arg(3) != 0)?;
                self.emit(&format!("=\t{}", esc(&r)));
            }
            "PANEL" => {
                // StandardCalculatorViewModel::UpdateProgrammerPanelDisplay
                let precision = 64;
                let (mut hex, mut dec, mut oct, mut bin) =
                    (String::new(), String::new(), String::new(), String::new());
                let in_error = self.display.borrow().is_in_error;
                if !in_error {
                    hex = mgr.get_result_for_radix(16, precision, true)?;
                    if !hex.is_empty() {
                        dec = mgr.get_result_for_radix(10, precision, true)?;
                        oct = mgr.get_result_for_radix(8, precision, true)?;
                        bin = mgr.get_result_for_radix(2, precision, true)?;
                    }
                }
                let raw = mgr.get_result_for_radix(2, precision, false)?;
                self.emit(&format!(
                    "=\t{}\t{}\t{}\t{}\t{}",
                    esc(&hex),
                    esc(&dec),
                    esc(&oct),
                    esc(&bin),
                    esc(&raw)
                ));
            }
            "REC" => {
                let v = mgr.is_engine_recording();
                self.emit(&format!("=\t{}", if v { 1 } else { 0 }));
            }
            "EMPTY" => {
                let v = mgr.is_input_empty();
                self.emit(&format!("=\t{}", if v { 1 } else { 0 }));
            }
            "HIST" => {
                let items = mgr.get_history_items();
                self.history_dump(&items);
            }
            "HISTM" => {
                let mode = if arg(1) == 0 {
                    CalculatorMode::Standard
                } else {
                    CalculatorMode::Scientific
                };
                let items = mgr.get_history_items_for_mode(mode);
                self.history_dump(&items);
            }
            "SNAP" => {
                let snap = mgr.get_display_commands_snapshot();
                self.emit(&format!("=\t{}", serialize_commands(&snap)));
            }
            "DEG" => {
                let d = mgr.get_current_degree_mode();
                self.emit(&format!("=\t{}", d.0));
            }
            "DSEP" => {
                let d = mgr.decimal_separator();
                self.emit(&format!("=\t{}", esc(&d.to_string())));
            }
            "MAXH" => {
                let m = mgr.max_history_size();
                self.emit(&format!("=\t{m}"));
            }
            other => panic!("unknown op {other}"),
        }
        Ok(())
    }

    fn run(&mut self, op_line: &str) {
        self.emit(op_line);
        let op: Vec<&str> = op_line[2..].split(' ').collect();
        if let Err(e) = self.exec(&op) {
            self.emit(&format!("!\t{e:08x}"));
        }
    }
}

struct Sequence {
    header: String,
    /// Lines after the header (ops and expected events).
    body: Vec<String>,
}

fn parse(text: &str) -> Vec<Sequence> {
    let mut seqs: Vec<Sequence> = Vec::new();
    for line in text.lines() {
        if line.starts_with("#S ") {
            seqs.push(Sequence {
                header: line.to_string(),
                body: Vec::new(),
            });
        } else if let Some(seq) = seqs.last_mut() {
            seq.body.push(line.to_string());
        }
    }
    seqs
}

/// Replays one sequence; returns `None` on success or a mismatch report.
fn replay(seq: &Sequence) -> Option<String> {
    let suite = seq.header.split(' ').nth(1).unwrap_or("");
    let mut session = Session::new(suite);
    for line in seq.body.iter().filter(|l| l.starts_with("> ")) {
        session.run(line);
    }
    let actual_text = std::mem::take(&mut session.display.borrow_mut().out);
    let actual: Vec<&str> = actual_text.lines().collect();
    let expected: Vec<&str> = seq.body.iter().map(|s| s.as_str()).collect();
    if actual == expected {
        return None;
    }

    let first_diff = actual
        .iter()
        .zip(expected.iter())
        .position(|(a, e)| a != e)
        .unwrap_or(actual.len().min(expected.len()));
    // Context: the ops executed up to the mismatch, then a window of lines.
    let ops_before: Vec<&str> = expected[..first_diff.min(expected.len())]
        .iter()
        .filter(|l| l.starts_with("> "))
        .copied()
        .collect();
    let mut report = format!(
        "{}\n  ops: {}\n",
        seq.header,
        ops_before
            .iter()
            .map(|o| &o[2..])
            .collect::<Vec<_>>()
            .join(" ; ")
    );
    let lo = first_diff.saturating_sub(3);
    let hi = (first_diff + 4).min(expected.len().max(actual.len()));
    for i in lo..hi {
        let e = expected.get(i).copied().unwrap_or("<none>");
        let a = actual.get(i).copied().unwrap_or("<none>");
        let mark = if i == first_diff { ">>" } else { "  " };
        let _ = writeln!(report, "  {mark} line {i}: expected {e:?}");
        if e != a {
            let _ = writeln!(report, "  {mark}         actual   {a:?}");
        }
    }
    Some(report)
}

fn run_golden(name: &str) {
    let path = format!("{}/tests/data/{}", env!("CARGO_MANIFEST_DIR"), name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let seqs = parse(&text);
    assert!(!seqs.is_empty(), "{path} contains no sequences");

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 16);
    let mut failures: Vec<String> = Vec::new();
    for chunk in seqs.chunks(workers) {
        let results: Vec<Option<String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|seq| {
                    // One fresh thread per sequence => pristine thread-local state.
                    std::thread::Builder::new()
                        .stack_size(16 * 1024 * 1024)
                        .spawn_scoped(scope, move || replay(seq))
                        .expect("spawn")
                })
                .collect();
            handles
                .into_iter()
                .zip(chunk.iter())
                .map(|(h, seq)| match h.join() {
                    Ok(r) => r,
                    Err(p) => {
                        let msg = p
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_default();
                        Some(format!("{}\n  PANIC: {msg}", seq.header))
                    }
                })
                .collect()
        });
        failures.extend(results.into_iter().flatten());
    }

    if !failures.is_empty() {
        let shown: Vec<&String> = failures.iter().take(12).collect();
        panic!(
            "{}: {} of {} sequences differ from the C++ oracle. First failures:\n{}",
            name,
            failures.len(),
            seqs.len(),
            shown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    eprintln!("{name}: {} sequences match the C++ oracle", seqs.len());
}

#[test]
fn golden_standard() {
    run_golden("golden_std.txt");
}

#[test]
fn golden_scientific() {
    run_golden("golden_sci.txt");
}

#[test]
fn golden_programmer() {
    run_golden("golden_prog.txt");
}

#[test]
fn golden_mixed() {
    run_golden("golden_mix.txt");
}

#[test]
fn golden_localized_separators() {
    run_golden("golden_loc.txt");
}
