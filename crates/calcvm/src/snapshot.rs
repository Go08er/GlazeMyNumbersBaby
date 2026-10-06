// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of `Calculator.ViewModels/Snapshots.cs`, the JSON aliases of
//! `Calculator.ViewModels/Utils/JsonUtils.cs`,
//! `Common/ExpressionCommandSerializer.cs` / `ExpressionCommandDeserializer.cs`
//! and the snapshot parts of `StandardCalculatorViewModel` /
//! `ApplicationViewModel` (`Snapshot` getter/setter, `RestoreFromSnapshot`).
//!
//! # Format
//!
//! The JSON uses the upstream property names (`ApplicationSnapshotAlias`
//! etc.), so an upstream snapshot can be restored and upstream can read ours
//! (it ignores unknown properties):
//!
//! ```text
//! { "m": <ViewMode: 0 Standard, 1 Scientific, 2 Programmer>,
//!   "s": { "m": { "h": [<history item>…] | null },      // current history, oldest first
//!          "p": { "d": <display value>, "e": <is error> },
//!          "e": { "t": [<token>…], "c": [<command>…] },   // optional: expression line
//!          "c": [<command>…] },                           // engine's display commands
//!   "x": { … } }                                          // gmnb extension, see below
//! history item: { "t": [<token>…], "c": [<command>…], "e": <expression>, "r": <result> }
//! token:        { "t": <text>, "c": <command index or -1> }
//! command:      { "$t": 0, "c": [<op>, <op>?] }                           unary
//!               { "$t": 1, "c": <op> }                                    binary
//!               { "$t": 2, "n": <neg>, "d": <dec>, "s": <sci>, "c": [<op>…] } operand
//!               { "$t": 3, "c": <op> }                                    parentheses
//! ```
//!
//! Upstream deliberately starts a recalled session with empty memory, the
//! other mode's history dropped and the angle unit, F-E, radix and word size
//! reset. The gmnb contract round-trips those, so they travel in the
//! extension object `"x"`: `"hs"`/`"hc"` (Standard/Scientific history, oldest
//! first), `"mem"` (memory as displayed, newest first), `"r"` (radix),
//! `"w"` (word size in bits), `"a"` (angle unit), `"fe"`, `"sh"` (shift
//! mode). Memory is restored by re-entering the displayed strings, so a value
//! comes back with the precision it was displayed with, and each slot is
//! shown in the form it was saved in (e-notation or not).
//!
//! `"k"` (absent before 0.2) holds what the display commands don't, so a
//! restored calculation continues as the saved one would: `"dv"` says the
//! engine shows a value they don't produce, `"result"` (a result, a
//! recalled value, a constant), `"entry"` (a typed number whose entry F-E,
//! MS or a radix switch ended) or `"sign"` (the same, with ± its last key);
//! `"eq"` is `[<binary command>, <operand>]`,
//! what another `=` repeats. Upstream restores neither: it shows the last
//! operand instead of a recalled value, "0" instead of a result after MS,
//! and re-opens an evaluated expression, so `=` evaluates it again. `"in"`
//! says how the number being entered stands where replaying the commands
//! wouldn't leave it so: `"empty"` (nothing typed since C or CE, though
//! the commands end with the empty input's 0), `"ended"` (that empty input
//! ended by a radix or angle switch, MS or M+), `"percent"` (the
//! commands' last operand is a `%` result, added to the expression rather
//! than typed) or `"signed"` (the number being typed had its sign changed
//! last, which the operand records after its first digit). Upstream's
//! restore replays the display commands of each of these states as a
//! number typed digit by digit.
//!
//! The display isn't always the engine's. Selecting a History item shows
//! the item's expression and result while the engine holds the item
//! replayed without `=`, its last operand typed (so `=` evaluates the item
//! again, and a digit replaces that operand), and some later keys leave the
//! display the item's. After a paste error only the view model is in error
//! (`OnPaste`), and the engine's calculation goes on under it: a memory
//! slot or a paste continues it, and a page change shows it again. `"ev"`
//! (absent when the display is the engine's) is the value the engine shows:
//! the engine is restored to it and the saved display, or error, shown over
//! it. `"hl": true` says a History item was the last thing loaded, which
//! keeps F-E disabled until the next key. The expression line comes back
//! as saved (`"s"."e"`), whether or not it is the engine's. Upstream
//! restores none of this: after a selection it replays the display commands
//! only, which show the item's first operand, and `=` then adds that
//! operand to itself.
//!
//! `"cl": true` says the next key clears the expression line although `=`
//! has nothing to repeat (`=` then `(`). Operands are replayed in the form
//! (F-E or not) they were written into the expression in, and a value a
//! trailing `(` kept (`2 + 3 = (`, `5 (`) is set up before it.
//!
//! `"cy": true` (Programmer only) is the carry bit RoL and RoR through
//! carry shift in next: in BYTE, `1 RoR 1` leaves the carry 1, so the next
//! RoR gives −128 rather than 0. No key sets it, so it is set directly.
//! `"lv"` (Standard only, with no operator pending) is the left operand,
//! an operand like `"eq"`'s: after `=` it is the result, which `%`
//! multiplies by (`2 + 3 = 7 %` is 0.35), while what `"eq"` sets up leaves
//! another; it is set directly where the restore didn't leave it so.
//! `"ie": true` says no number is being typed and the input holds none;
//! otherwise it keeps the number typed last, unseen, and whenever 0 is
//! shown (`56 − 56 =`) Scientific and Programmer mode's C key is then CE
//! (`IsInputEmpty`). It is set directly too. Upstream restores none of
//! these.
//!
//! # The contract
//!
//! A restored calculation shows what was saved and continues as the saved
//! one would have, key for key, except that values come back as they were
//! shown: memory, a shown result and the operands of the expression return
//! with the digits the display gave them (a Programmer memory slot as the
//! word size shows it, −0 as 0), so a later result that depends on more
//! can differ, in the last digits or, after cancellation, wholly. A state
//! the restore can't rebuild that way comes back as a new calculation from
//! the saved value instead: the expression is cleared, the value shown as
//! a result (the next digit replaces it, `=` repeats nothing, the carry is
//! clear, as after C), and memory, the histories and the modes are kept.
//! Such a state is either marked when saved (`"nr": true`: the engine
//! knows its commands won't rebuild it: a number typed right after `)`, a
//! word size switched mid-expression, or after `=` with what it repeats
//! wider than the new one, a number begun with Exp after C or CE,
//! parentheses the expression doesn't hold), or found when restored: the
//! restored calculation is saved again and compared with what was loaded
//! (the display, the expression line, the display commands, `"k"` and the
//! modes). An error the engine is in is restored as an error without that
//! check, since every key clears it. Snapshots without `"k"` are restored
//! as before, unchecked. The check compares what is saved; the randomized
//! restore tests (`tests/restore_fuzz.rs`) also compare the engine's own
//! state.

use std::rc::Rc;

use calcmanager::{
    BinaryCommand, CalculatorMode, Entry, ExpressionCommand, ExpressionToken, HistoryItem,
    HistoryItemVector, OpndCommand, Parentheses, ShownValue, UnaryCommand,
};
use serde_json::{Map, Value, json};

use crate::standard_vm::{StandardCalculatorViewModel, cmd, paste_command_id};
use crate::{AngleUnit, CalcMode, Radix, ShiftMode, WordSize};

// ---------------------------------------------------------------------------
// Snapshot classes (Snapshots.cs)
// ---------------------------------------------------------------------------

/// `ExpressionCommandWrapper` (the interop value type of an engine command).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExpressionCommandWrapper {
    Unary(Vec<i32>),
    Binary(i32),
    Operand {
        commands: Vec<i32>,
        is_negative: bool,
        is_decimal_present: bool,
        is_sci_fmt: bool,
    },
    Parentheses(i32),
}

impl ExpressionCommandWrapper {
    pub(crate) fn from_command(command: &ExpressionCommand) -> Self {
        match command {
            ExpressionCommand::Unary(u) => {
                ExpressionCommandWrapper::Unary(u.get_commands().to_vec())
            }
            ExpressionCommand::Binary(b) => ExpressionCommandWrapper::Binary(b.get_command()),
            ExpressionCommand::Operand(o) => ExpressionCommandWrapper::Operand {
                commands: o
                    .get_commands()
                    .iter()
                    .map(|&c| normalize_operand_digit(c))
                    .collect(),
                is_negative: o.is_negative(),
                is_decimal_present: o.is_decimal_present(),
                is_sci_fmt: o.is_sci_fmt(),
            },
            ExpressionCommand::Parentheses(p) => {
                ExpressionCommandWrapper::Parentheses(p.get_command())
            }
        }
    }

    /// Builds the engine command. Callers validate first (a unary command
    /// needs one or two op codes).
    pub(crate) fn to_command(&self) -> ExpressionCommand {
        match self {
            ExpressionCommandWrapper::Unary(c) => {
                if c.len() == 2 {
                    ExpressionCommand::Unary(UnaryCommand::new2(c[0], c[1]))
                } else {
                    ExpressionCommand::Unary(UnaryCommand::new(c.first().copied().unwrap_or(0)))
                }
            }
            ExpressionCommandWrapper::Binary(c) => {
                ExpressionCommand::Binary(BinaryCommand::new(*c))
            }
            ExpressionCommandWrapper::Operand {
                commands,
                is_negative,
                is_decimal_present,
                is_sci_fmt,
            } => ExpressionCommand::Operand(OpndCommand::new(
                commands.clone(),
                *is_negative,
                *is_decimal_present,
                *is_sci_fmt,
            )),
            ExpressionCommandWrapper::Parentheses(c) => {
                ExpressionCommand::Parentheses(Parentheses::new(*c))
            }
        }
    }
}

/// The engine's `GetOperandCommandsFromString` (faithful to upstream)
/// encodes a hex digit `A`–`F` of a Programmer operand as
/// `Command0 + (ch - '0')`, i.e. 147–152, which `SnapshotValidator` rejects
/// and the engine would not replay. Map them to `CommandA`–`CommandF`.
fn normalize_operand_digit(command: i32) -> i32 {
    const BOGUS_A: i32 = cmd::ZERO + ('A' as i32 - '0' as i32);
    const BOGUS_F: i32 = cmd::ZERO + ('F' as i32 - '0' as i32);
    if (BOGUS_A..=BOGUS_F).contains(&command) {
        command - BOGUS_A + cmd::A
    } else {
        command
    }
}

/// `CalcManagerToken`
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CalcManagerToken {
    pub(crate) op_code_name: String,
    pub(crate) command_index: i32,
}

/// `CalcManagerHistoryItem`
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct CalcManagerHistoryItem {
    pub(crate) tokens: Vec<CalcManagerToken>,
    pub(crate) commands: Vec<ExpressionCommandWrapper>,
    pub(crate) expression: String,
    pub(crate) result: String,
}

impl CalcManagerHistoryItem {
    fn from_history_item(item: &HistoryItem) -> Self {
        let v = &item.history_item_vector;
        CalcManagerHistoryItem {
            tokens: v
                .tokens
                .iter()
                .map(|(t, c)| CalcManagerToken {
                    op_code_name: t.clone(),
                    command_index: *c,
                })
                .collect(),
            commands: v
                .commands
                .iter()
                .map(ExpressionCommandWrapper::from_command)
                .collect(),
            expression: v.expression.clone(),
            result: v.result.clone(),
        }
    }

    fn to_history_item(&self) -> Rc<HistoryItem> {
        Rc::new(HistoryItem {
            history_item_vector: HistoryItemVector {
                tokens: tokens_to_engine(&self.tokens),
                commands: self
                    .commands
                    .iter()
                    .map(ExpressionCommandWrapper::to_command)
                    .collect(),
                expression: self.expression.clone(),
                result: self.result.clone(),
            },
        })
    }
}

/// `CalcManagerSnapshot` (`None` preserves the distinction between
/// uncaptured and empty history).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct CalcManagerSnapshot {
    pub(crate) history_items: Option<Vec<CalcManagerHistoryItem>>,
}

/// `PrimaryDisplaySnapshot`
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct PrimaryDisplaySnapshot {
    pub(crate) display_value: String,
    pub(crate) is_error: bool,
}

/// `ExpressionDisplaySnapshot`
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct ExpressionDisplaySnapshot {
    pub(crate) tokens: Vec<CalcManagerToken>,
    pub(crate) commands: Vec<ExpressionCommandWrapper>,
}

/// `StandardCalculatorSnapshot`
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct StandardCalculatorSnapshot {
    pub(crate) calc_manager: CalcManagerSnapshot,
    pub(crate) primary_display: PrimaryDisplaySnapshot,
    pub(crate) expression_display: Option<ExpressionDisplaySnapshot>,
    pub(crate) display_commands: Vec<ExpressionCommandWrapper>,
}

/// The gmnb extension (`"x"`), see the module docs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct SnapshotExtension {
    pub(crate) standard_history: Option<Vec<CalcManagerHistoryItem>>,
    pub(crate) scientific_history: Option<Vec<CalcManagerHistoryItem>>,
    pub(crate) memory: Vec<String>,
    pub(crate) radix: Option<Radix>,
    pub(crate) word_size: Option<WordSize>,
    pub(crate) angle: Option<AngleUnit>,
    pub(crate) fe: bool,
    pub(crate) shift_mode: Option<ShiftMode>,
    /// `"k"`; `None` in snapshots that predate it.
    pub(crate) continuation: Option<ContinuationSnapshot>,
}

/// The gmnb extension's `"k"`, see the module docs.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct ContinuationSnapshot {
    /// `"dv"`
    pub(crate) shown: Option<ShownValue>,
    /// `"eq"`: empty, or a binary command and an operand.
    pub(crate) repeat: Vec<ExpressionCommandWrapper>,
    /// `"ev"`: the engine's value, when the display shows another.
    pub(crate) engine_value: Option<String>,
    /// `"hl"`
    pub(crate) history_load: bool,
    /// `"in"`
    pub(crate) entry: Option<Entry>,
    /// `"cl"`
    pub(crate) clears: bool,
    /// `"nr"`
    pub(crate) unreplayable: bool,
    /// `"cy"`: the carry bit of RoL/RoR through carry (Programmer only).
    pub(crate) carry: bool,
    /// `"lv"`: the left operand, an operand (Standard only, with no
    /// operator pending).
    pub(crate) left: Option<ExpressionCommandWrapper>,
    /// `"ie"`: no number is being typed and the input holds none.
    pub(crate) empty_input: bool,
}

/// `ApplicationSnapshot`
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct ApplicationSnapshot {
    pub(crate) mode: i64,
    pub(crate) standard_calculator: Option<StandardCalculatorSnapshot>,
    pub(crate) extension: Option<SnapshotExtension>,
}

fn tokens_to_engine(tokens: &[CalcManagerToken]) -> Vec<ExpressionToken> {
    tokens
        .iter()
        .map(|t| (t.op_code_name.clone(), t.command_index))
        .collect()
}

fn tokens_from_engine(tokens: &[ExpressionToken]) -> Vec<CalcManagerToken> {
    tokens
        .iter()
        .map(|(t, c)| CalcManagerToken {
            op_code_name: t.clone(),
            command_index: *c,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ExpressionCommandSerializer / ExpressionCommandDeserializer (JSON form of
// JsonUtils' ICalcManagerIExprCommandAlias)
// ---------------------------------------------------------------------------

/// `ExpressionCommandSerializer`: writes commands into JSON values.
pub(crate) struct ExpressionCommandSerializer;

impl ExpressionCommandSerializer {
    pub(crate) fn serialize_operand(
        is_negative: bool,
        is_decimal_present: bool,
        is_sci_fmt: bool,
        commands: &[i32],
    ) -> Value {
        json!({ "$t": 2, "n": is_negative, "d": is_decimal_present, "s": is_sci_fmt, "c": commands })
    }

    pub(crate) fn serialize_unary(commands: &[i32]) -> Value {
        json!({ "$t": 0, "c": commands })
    }

    pub(crate) fn serialize_binary(command: i32) -> Value {
        json!({ "$t": 1, "c": command })
    }

    pub(crate) fn serialize_parentheses(command: i32) -> Value {
        json!({ "$t": 3, "c": command })
    }

    pub(crate) fn serialize(command: &ExpressionCommandWrapper) -> Value {
        match command {
            ExpressionCommandWrapper::Unary(c) => Self::serialize_unary(c),
            ExpressionCommandWrapper::Binary(c) => Self::serialize_binary(*c),
            ExpressionCommandWrapper::Operand {
                commands,
                is_negative,
                is_decimal_present,
                is_sci_fmt,
            } => Self::serialize_operand(*is_negative, *is_decimal_present, *is_sci_fmt, commands),
            ExpressionCommandWrapper::Parentheses(c) => Self::serialize_parentheses(*c),
        }
    }
}

/// `ExpressionCommandDeserializer` + `Helpers.MapCommandAlias`.
pub(crate) struct ExpressionCommandDeserializer;

type SnapResult<T> = Result<T, String>;

impl ExpressionCommandDeserializer {
    pub(crate) fn deserialize(value: &Value) -> SnapResult<ExpressionCommandWrapper> {
        let obj = value.as_object().ok_or("command is not an object")?;
        let tag = obj
            .get("$t")
            .and_then(Value::as_i64)
            .ok_or("command has no type discriminator")?;
        match tag {
            0 => {
                // Unary commands require one or two command codes.
                let commands = int_list(obj.get("c"))?;
                if commands.len() != 1 && commands.len() != 2 {
                    return Err("ill-formed unary command".into());
                }
                Ok(ExpressionCommandWrapper::Unary(commands))
            }
            1 => Ok(ExpressionCommandWrapper::Binary(int(obj.get("c"))?)),
            2 => Ok(ExpressionCommandWrapper::Operand {
                is_negative: boolean(obj.get("n"))?,
                is_decimal_present: boolean(obj.get("d"))?,
                is_sci_fmt: boolean(obj.get("s"))?,
                commands: int_list(obj.get("c"))?,
            }),
            3 => Ok(ExpressionCommandWrapper::Parentheses(int(obj.get("c"))?)),
            _ => Err("unhandled command alias type".into()),
        }
    }
}

fn int(v: Option<&Value>) -> SnapResult<i32> {
    match v {
        None => Ok(0),
        Some(v) => v
            .as_i64()
            .and_then(|i| i32::try_from(i).ok())
            .ok_or_else(|| "expected an int32".to_string()),
    }
}

fn boolean(v: Option<&Value>) -> SnapResult<bool> {
    match v {
        None => Ok(false),
        Some(v) => v.as_bool().ok_or_else(|| "expected a boolean".to_string()),
    }
}

fn string(v: Option<&Value>) -> SnapResult<Option<String>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err("expected a string".into()),
    }
}

fn int_list(v: Option<&Value>) -> SnapResult<Vec<i32>> {
    match v {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => a.iter().map(|x| int(Some(x))).collect(),
        Some(_) => Err("expected an array of int32".into()),
    }
}

fn list<T>(v: Option<&Value>, f: impl Fn(&Value) -> SnapResult<T>) -> SnapResult<Option<Vec<T>>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a.iter().map(f).collect::<SnapResult<Vec<T>>>().map(Some),
        Some(_) => Err("expected an array".into()),
    }
}

fn object(v: &Value) -> SnapResult<&Map<String, Value>> {
    v.as_object()
        .ok_or_else(|| "expected an object".to_string())
}

// ---------------------------------------------------------------------------
// JSON mapping (JsonUtils aliases)
// ---------------------------------------------------------------------------

fn token_to_json(t: &CalcManagerToken) -> Value {
    json!({ "t": t.op_code_name, "c": t.command_index })
}

fn token_from_json(v: &Value) -> SnapResult<CalcManagerToken> {
    let o = object(v)?;
    Ok(CalcManagerToken {
        op_code_name: string(o.get("t"))?.unwrap_or_default(),
        command_index: int(o.get("c"))?,
    })
}

fn commands_to_json(c: &[ExpressionCommandWrapper]) -> Value {
    Value::Array(
        c.iter()
            .map(ExpressionCommandSerializer::serialize)
            .collect(),
    )
}

fn history_item_to_json(h: &CalcManagerHistoryItem) -> Value {
    json!({
        "t": h.tokens.iter().map(token_to_json).collect::<Vec<_>>(),
        "c": commands_to_json(&h.commands),
        "e": h.expression,
        "r": h.result,
    })
}

fn history_item_from_json(v: &Value) -> SnapResult<CalcManagerHistoryItem> {
    let o = object(v)?;
    Ok(CalcManagerHistoryItem {
        tokens: list(o.get("t"), token_from_json)?.unwrap_or_default(),
        commands: list(o.get("c"), ExpressionCommandDeserializer::deserialize)?.unwrap_or_default(),
        expression: string(o.get("e"))?.unwrap_or_default(),
        result: string(o.get("r"))?.unwrap_or_default(),
    })
}

fn history_to_json(h: &Option<Vec<CalcManagerHistoryItem>>) -> Value {
    match h {
        None => Value::Null,
        Some(items) => Value::Array(items.iter().map(history_item_to_json).collect()),
    }
}

fn radix_name(r: Radix) -> &'static str {
    match r {
        Radix::Hex => "hex",
        Radix::Dec => "dec",
        Radix::Oct => "oct",
        Radix::Bin => "bin",
    }
}

fn angle_name(a: AngleUnit) -> &'static str {
    match a {
        AngleUnit::Degrees => "deg",
        AngleUnit::Radians => "rad",
        AngleUnit::Gradians => "grad",
    }
}

fn shift_name(s: ShiftMode) -> &'static str {
    match s {
        ShiftMode::Arithmetic => "arithmetic",
        ShiftMode::Logical => "logical",
        ShiftMode::Rotate => "rotate",
        ShiftMode::RotateThroughCarry => "rotate-carry",
    }
}

fn shown_name(s: ShownValue) -> &'static str {
    match s {
        ShownValue::Result => "result",
        ShownValue::EndedEntry => "entry",
        ShownValue::EndedSign => "sign",
    }
}

fn entry_name(e: Entry) -> &'static str {
    match e {
        Entry::Empty => "empty",
        Entry::Ended => "ended",
        Entry::Percent => "percent",
        Entry::Signed => "signed",
    }
}

impl ApplicationSnapshot {
    pub(crate) fn to_json(&self) -> Value {
        let mut root = Map::new();
        root.insert("m".into(), json!(self.mode));
        root.insert(
            "s".into(),
            match &self.standard_calculator {
                None => Value::Null,
                Some(s) => {
                    let mut o = Map::new();
                    o.insert("m".into(), json!({ "h": history_to_json(&s.calc_manager.history_items) }));
                    o.insert("p".into(), json!({ "d": s.primary_display.display_value, "e": s.primary_display.is_error }));
                    o.insert(
                        "e".into(),
                        match &s.expression_display {
                            None => Value::Null,
                            Some(e) => json!({
                                "t": e.tokens.iter().map(token_to_json).collect::<Vec<_>>(),
                                "c": commands_to_json(&e.commands),
                            }),
                        },
                    );
                    o.insert("c".into(), commands_to_json(&s.display_commands));
                    Value::Object(o)
                }
            },
        );
        if let Some(x) = &self.extension {
            let mut o = Map::new();
            o.insert("hs".into(), history_to_json(&x.standard_history));
            o.insert("hc".into(), history_to_json(&x.scientific_history));
            o.insert("mem".into(), json!(x.memory));
            if let Some(r) = x.radix {
                o.insert("r".into(), json!(radix_name(r)));
            }
            if let Some(w) = x.word_size {
                o.insert("w".into(), json!(w.bits()));
            }
            if let Some(a) = x.angle {
                o.insert("a".into(), json!(angle_name(a)));
            }
            o.insert("fe".into(), json!(x.fe));
            if let Some(s) = x.shift_mode {
                o.insert("sh".into(), json!(shift_name(s)));
            }
            if let Some(k) = &x.continuation {
                let mut c = Map::new();
                if let Some(shown) = k.shown {
                    c.insert("dv".into(), json!(shown_name(shown)));
                }
                if !k.repeat.is_empty() {
                    c.insert("eq".into(), commands_to_json(&k.repeat));
                }
                if let Some(v) = &k.engine_value {
                    c.insert("ev".into(), json!(v));
                }
                if k.history_load {
                    c.insert("hl".into(), json!(true));
                }
                if let Some(e) = k.entry {
                    c.insert("in".into(), json!(entry_name(e)));
                }
                if k.clears {
                    c.insert("cl".into(), json!(true));
                }
                if k.unreplayable {
                    c.insert("nr".into(), json!(true));
                }
                if k.carry {
                    c.insert("cy".into(), json!(true));
                }
                if let Some(left) = &k.left {
                    c.insert("lv".into(), ExpressionCommandSerializer::serialize(left));
                }
                if k.empty_input {
                    c.insert("ie".into(), json!(true));
                }
                o.insert("k".into(), Value::Object(c));
            }
            root.insert("x".into(), Value::Object(o));
        }
        Value::Object(root)
    }

    pub(crate) fn from_json(text: &str) -> SnapResult<ApplicationSnapshot> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let root = object(&v)?;
        let mode = root
            .get("m")
            .map(|m| m.as_i64().ok_or("mode is not an integer"))
            .transpose()?
            .unwrap_or(0);

        let standard_calculator = match root.get("s") {
            None | Some(Value::Null) => None,
            Some(s) => {
                let s = object(s)?;
                let calc_manager = match s.get("m") {
                    None | Some(Value::Null) => CalcManagerSnapshot::default(),
                    Some(m) => CalcManagerSnapshot {
                        history_items: list(object(m)?.get("h"), history_item_from_json)?,
                    },
                };
                let primary_display = match s.get("p") {
                    None | Some(Value::Null) => {
                        return Err("Primary display state is missing.".into());
                    }
                    Some(p) => {
                        let p = object(p)?;
                        PrimaryDisplaySnapshot {
                            display_value: string(p.get("d"))?
                                .ok_or("Primary display state is missing.")?,
                            is_error: boolean(p.get("e"))?,
                        }
                    }
                };
                let expression_display = match s.get("e") {
                    None | Some(Value::Null) => None,
                    Some(e) => {
                        let e = object(e)?;
                        Some(ExpressionDisplaySnapshot {
                            tokens: list(e.get("t"), token_from_json)?
                                .ok_or("expression has no token list")?,
                            commands: list(e.get("c"), ExpressionCommandDeserializer::deserialize)?
                                .ok_or("expression has no command list")?,
                        })
                    }
                };
                let display_commands =
                    list(s.get("c"), ExpressionCommandDeserializer::deserialize)?
                        .unwrap_or_default();
                Some(StandardCalculatorSnapshot {
                    calc_manager,
                    primary_display,
                    expression_display,
                    display_commands,
                })
            }
        };

        let extension = match root.get("x") {
            None | Some(Value::Null) => None,
            Some(x) => {
                let x = object(x)?;
                let memory = list(x.get("mem"), |v| {
                    v.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "memory entry is not a string".to_string())
                })?
                .unwrap_or_default();
                let radix = match string(x.get("r"))?.as_deref() {
                    None => None,
                    Some("hex") => Some(Radix::Hex),
                    Some("dec") => Some(Radix::Dec),
                    Some("oct") => Some(Radix::Oct),
                    Some("bin") => Some(Radix::Bin),
                    Some(_) => return Err("unknown radix".into()),
                };
                let word_size = match x.get("w") {
                    None | Some(Value::Null) => None,
                    Some(w) => Some(match w.as_i64() {
                        Some(64) => WordSize::Qword,
                        Some(32) => WordSize::Dword,
                        Some(16) => WordSize::Word,
                        Some(8) => WordSize::Byte,
                        _ => return Err("unknown word size".into()),
                    }),
                };
                let angle = match string(x.get("a"))?.as_deref() {
                    None => None,
                    Some("deg") => Some(AngleUnit::Degrees),
                    Some("rad") => Some(AngleUnit::Radians),
                    Some("grad") => Some(AngleUnit::Gradians),
                    Some(_) => return Err("unknown angle unit".into()),
                };
                let shift_mode = match string(x.get("sh"))?.as_deref() {
                    None => None,
                    Some("arithmetic") => Some(ShiftMode::Arithmetic),
                    Some("logical") => Some(ShiftMode::Logical),
                    Some("rotate") => Some(ShiftMode::Rotate),
                    Some("rotate-carry") => Some(ShiftMode::RotateThroughCarry),
                    Some(_) => return Err("unknown shift mode".into()),
                };
                let continuation = match x.get("k") {
                    None | Some(Value::Null) => None,
                    Some(k) => {
                        let k = object(k)?;
                        let shown = match string(k.get("dv"))?.as_deref() {
                            None => None,
                            Some("result") => Some(ShownValue::Result),
                            Some("entry") => Some(ShownValue::EndedEntry),
                            Some("sign") => Some(ShownValue::EndedSign),
                            Some(_) => return Err("unknown display value kind".into()),
                        };
                        let repeat = list(k.get("eq"), ExpressionCommandDeserializer::deserialize)?
                            .unwrap_or_default();
                        Some(ContinuationSnapshot {
                            shown,
                            repeat,
                            engine_value: string(k.get("ev"))?,
                            history_load: boolean(k.get("hl"))?,
                            entry: match string(k.get("in"))?.as_deref() {
                                None => None,
                                Some("empty") => Some(Entry::Empty),
                                Some("ended") => Some(Entry::Ended),
                                Some("percent") => Some(Entry::Percent),
                                Some("signed") => Some(Entry::Signed),
                                Some(_) => return Err("unknown entry state".into()),
                            },
                            clears: boolean(k.get("cl"))?,
                            unreplayable: boolean(k.get("nr"))?,
                            carry: boolean(k.get("cy"))?,
                            left: k
                                .get("lv")
                                .filter(|v| !v.is_null())
                                .map(ExpressionCommandDeserializer::deserialize)
                                .transpose()?,
                            empty_input: boolean(k.get("ie"))?,
                        })
                    }
                };
                Some(SnapshotExtension {
                    standard_history: list(x.get("hs"), history_item_from_json)?,
                    scientific_history: list(x.get("hc"), history_item_from_json)?,
                    memory,
                    radix,
                    word_size,
                    angle,
                    fe: boolean(x.get("fe"))?,
                    shift_mode,
                    continuation,
                })
            }
        };

        Ok(ApplicationSnapshot {
            mode,
            standard_calculator,
            extension,
        })
    }
}

// ---------------------------------------------------------------------------
// SnapshotValidator
// ---------------------------------------------------------------------------

/// `SnapshotValidator`
pub(crate) struct SnapshotValidator;

/// Extension: the most engine keys one command list (the display commands,
/// the expression, a history item) may replay; a snapshot with a longer one
/// is rejected as a whole, like any other invalid snapshot. Hand-typed
/// calculations stay far below it, and 40 pastes of a 100-term sum (about
/// 12,000 keys) still fit. With display updates deferred, replay is linear:
/// at the limit, cheap keys restore in about 30 ms (Programmer, whose number
/// conversions cost more per key, about 0.2 s). Expensive arithmetic is
/// bounded by [`REPLAY_WORK`] instead.
pub(crate) const MAX_RESTORED_KEYS: usize = 16_384;

/// Extension: the longest primary display a snapshot may carry (the longest
/// the engine shows is a padded 64-bit binary number, 79 characters).
pub(crate) const MAX_DISPLAY_LENGTH: usize = copypaste::MAX_PASTEABLE_LENGTH as usize;

/// Extension: memory slots restored (`CalculatorManager`'s
/// `MAXIMUM_MEMORY_SIZE`; the manager drops older ones anyway).
pub(crate) const MAX_RESTORED_MEMORY: usize = 100;

/// Extension: arithmetic budgets for replaying saved state, in units of
/// `ratpack::work_done` (digit operations). Keys are cheap, but one operator
/// can cost milliseconds (√, ln, x^y, n! of a fraction, anything near
/// 10^±9999: up to 180 ms for one n!), and a crafted chain of them at the
/// key limit replayed for half a minute. A replay that goes past its budget
/// stops, and only what it was restoring is dropped: the pending expression
/// (the displayed value is kept), a history item's replay (its result is
/// kept), a memory slot (and, once the slots together are past theirs, the
/// newer ones).
///
/// In a release build heavy arithmetic runs at 1–4.5 ns a unit, so the
/// costliest crafted states (n! of fractions, after 100 memory slots near
/// 10^-9999) restore in under a second. The costliest of 1,506
/// app-produced states replays 166 million units (an n! of a fraction among
/// others, 0.35 s); 40 pastes of a 100-term sum, 59 million.
pub(crate) const REPLAY_WORK: u64 = 180_000_000;
/// All memory slots together, and one slot (one displayed number: a
/// legible one costs well under a million units, one near 10^±9999 about
/// three million). The displayed value, and the operation "=" repeats (one
/// operand and one operator), get one slot's budget each; past it, the
/// value is cleared, or "=" repeats nothing.
pub(crate) const MEMORY_WORK: u64 = 30_000_000;
pub(crate) const VALUE_WORK: u64 = 8_000_000;

impl SnapshotValidator {
    /// The calculator view mode of a snapshot (only Standard, Scientific and
    /// Programmer are handled by this view model).
    pub(crate) fn mode(snapshot: &ApplicationSnapshot) -> SnapResult<CalcMode> {
        match snapshot.mode {
            0 => Ok(CalcMode::Standard),
            1 => Ok(CalcMode::Scientific),
            2 => Ok(CalcMode::Programmer),
            _ => Err("Invalid calculator mode.".into()),
        }
    }

    /// `Validate(ApplicationSnapshot)`
    pub(crate) fn validate(snapshot: &ApplicationSnapshot) -> SnapResult<()> {
        Self::mode(snapshot)?;
        let Some(standard) = &snapshot.standard_calculator else {
            return Err("Calculator snapshot state is missing.".into());
        };

        let mut all_history: Vec<(&str, &CalcManagerHistoryItem)> = Vec::new();
        for h in standard.calc_manager.history_items.iter().flatten() {
            all_history.push(("history item", h));
        }
        if let Some(x) = &snapshot.extension {
            for h in x
                .standard_history
                .iter()
                .flatten()
                .chain(x.scientific_history.iter().flatten())
            {
                all_history.push(("history item", h));
            }
        }
        for (i, (location, item)) in all_history.iter().enumerate() {
            Self::validate_token_indexes(&item.tokens, &item.commands, &format!("{location} {i}"))?;
        }

        if let Some(expression) = &standard.expression_display {
            Self::validate_token_indexes(&expression.tokens, &expression.commands, "expression")?;
        }
        Ok(())
    }

    /// `ValidateProtocol(ApplicationSnapshot)` — the checks for untrusted
    /// input.
    ///
    /// Extension: commands are checked against the mode they replay in
    /// (upstream accepts the Programmer-only bitwise operators everywhere,
    /// and outside Programmer mode a shift by a huge count never finishes),
    /// and the replayed key count and the display length are bounded.
    pub(crate) fn validate_protocol(snapshot: &ApplicationSnapshot) -> SnapResult<()> {
        Self::validate(snapshot)?;
        let mode = Self::mode(snapshot)?;
        let standard = snapshot
            .standard_calculator
            .as_ref()
            .ok_or("Calculator snapshot state is missing.")?;

        let mut histories: Vec<(CalcMode, &CalcManagerHistoryItem)> = standard
            .calc_manager
            .history_items
            .iter()
            .flatten()
            .map(|h| (mode, h))
            .collect();
        if let Some(x) = &snapshot.extension {
            histories.extend(
                x.standard_history
                    .iter()
                    .flatten()
                    .map(|h| (CalcMode::Standard, h)),
            );
            histories.extend(
                x.scientific_history
                    .iter()
                    .flatten()
                    .map(|h| (CalcMode::Scientific, h)),
            );
        }
        for (i, (mode, item)) in histories.iter().enumerate() {
            Self::validate_commands(&item.commands, *mode, &format!("history item {i}"))?;
        }

        if let Some(expression) = &standard.expression_display {
            Self::validate_commands(&expression.commands, mode, "expression")?;
        }

        if let Some(repeat) = snapshot
            .extension
            .as_ref()
            .and_then(|x| x.continuation.as_ref())
            .map(|k| &k.repeat[..])
            .filter(|r| !r.is_empty())
        {
            let [
                ExpressionCommandWrapper::Binary(_),
                ExpressionCommandWrapper::Operand { .. },
            ] = repeat
            else {
                return Err("the repeated operation is not an operator and an operand".into());
            };
            Self::validate_commands(repeat, mode, "repeated operation")?;
        }

        let continuation = snapshot
            .extension
            .as_ref()
            .and_then(|x| x.continuation.as_ref());
        // Extension: only the Programmer engine rotates through a carry.
        if mode != CalcMode::Programmer && continuation.is_some_and(|k| k.carry) {
            return Err("a carry outside Programmer mode".into());
        }
        // Extension: the left operand is saved in Standard mode only, and is
        // a decimal number.
        if let Some(left) = continuation.and_then(|k| k.left.as_ref()) {
            let ExpressionCommandWrapper::Operand { commands, .. } = left else {
                return Err("the left operand is not an operand".into());
            };
            if mode != CalcMode::Standard
                || commands.iter().any(|&c| (cmd::A..=cmd::F).contains(&c))
            {
                return Err("the left operand is not a Standard mode number".into());
            }
            Self::validate_commands(std::slice::from_ref(left), mode, "left operand")?;
        }

        let display = &standard.primary_display.display_value;
        let engine_value = snapshot
            .extension
            .as_ref()
            .and_then(|x| x.continuation.as_ref())
            .and_then(|k| k.engine_value.as_ref());
        if std::iter::once(display)
            .chain(engine_value)
            .any(|v| v.encode_utf16().count() > MAX_DISPLAY_LENGTH)
        {
            return Err("display value is too long".into());
        }

        Self::validate_commands(&standard.display_commands, mode, "display")
    }

    fn validate_token_indexes(
        tokens: &[CalcManagerToken],
        commands: &[ExpressionCommandWrapper],
        location: &str,
    ) -> SnapResult<()> {
        for (i, token) in tokens.iter().enumerate() {
            if token.command_index < -1 || token.command_index >= commands.len() as i32 {
                return Err(format!(
                    "{location} token {i} does not reference a command."
                ));
            }
        }
        Ok(())
    }

    fn validate_commands(
        commands: &[ExpressionCommandWrapper],
        mode: CalcMode,
        location: &str,
    ) -> SnapResult<()> {
        let mut keys = 0usize;
        for (i, command) in commands.iter().enumerate() {
            let is_valid = match command {
                ExpressionCommandWrapper::Unary(c) => Self::is_valid_unary_command(c, mode),
                ExpressionCommandWrapper::Binary(c) => Self::is_valid_binary_command(*c, mode),
                ExpressionCommandWrapper::Operand { commands, .. } => {
                    Self::is_valid_operand_command(commands)
                }
                ExpressionCommandWrapper::Parentheses(c) => *c == cmd::OPENP || *c == cmd::CLOSEP,
            };
            if !is_valid {
                return Err(format!("{location} command {i} is invalid."));
            }
            keys += match command {
                ExpressionCommandWrapper::Unary(c) => c.len(),
                // The digits, plus a sign key.
                ExpressionCommandWrapper::Operand { commands, .. } => commands.len() + 1,
                _ => 1,
            };
            if keys > MAX_RESTORED_KEYS {
                return Err(format!("{location} is too long."));
            }
        }
        Ok(())
    }

    fn is_valid_unary_command(commands: &[i32], mode: CalcMode) -> bool {
        match commands {
            [c] => Self::is_unary_operator(*c, mode),
            [angle, op] => Self::is_angle_command(*angle) && Self::is_unary_operator(*op, mode),
            _ => false,
        }
    }

    /// Extension: the operators only Programmer mode has (ROL, ROR, NOT,
    /// ROL/ROR through carry; AND, OR, XOR, the shifts, NAND, NOR).
    fn is_programmer_only(command: i32) -> bool {
        (99..=101).contains(&command) // CommandROL..=CommandCOM
            || (416..=417).contains(&command) // CommandROLC..=CommandRORC
            || (86..=90).contains(&command) // CommandAnd..=CommandRSHF
            || (501..=502).contains(&command) // CommandNand..=CommandNor
            || command == 505 // CommandRSHFL
    }

    fn is_unary_operator(command: i32, mode: CalcMode) -> bool {
        let is_unary = command == cmd::SIGN
            || (98..=118).contains(&command) // CommandCHOP..=CommandPERCENT
            || (202..=208).contains(&command) // CommandASIN..=CommandATANH
            || command == 324 // NumbersAndOperatorsEnum.Degrees
            || (400..=417).contains(&command); // CommandSEC..=CommandRORC
        is_unary && (mode == CalcMode::Programmer || !Self::is_programmer_only(command))
    }

    fn is_angle_command(command: i32) -> bool {
        (cmd::DEG..=cmd::GRAD).contains(&command)
    }

    fn is_valid_binary_command(command: i32, mode: CalcMode) -> bool {
        let is_binary = (86..=97).contains(&command) // CommandAnd..=CommandPWR
            || (500..=502).contains(&command) // CommandLogBaseY..=CommandNor
            || command == 505; // CommandRSHFL
        is_binary && (mode == CalcMode::Programmer || !Self::is_programmer_only(command))
    }

    fn is_valid_operand_command(commands: &[i32]) -> bool {
        !commands.is_empty()
            && commands.iter().all(|&c| {
                c == cmd::SIGN
                    || c == cmd::PNT
                    || c == cmd::EXP
                    || (cmd::ZERO..=cmd::F).contains(&c)
            })
    }
}

// ---------------------------------------------------------------------------
// StandardCalculatorViewModel.Snapshot
// ---------------------------------------------------------------------------

/// Extension: `text` in e-notation ("-1.21e+2") written out ("-121"), if
/// that takes at most `max_digits` digits; `None` otherwise or if it isn't
/// e-notation.
pub(crate) fn plain_decimal(text: &str, point: char, max_digits: usize) -> Option<String> {
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (mantissa, exponent) = text.split_once('e')?;
    let exponent: i64 = exponent.parse().ok()?;
    let (int, frac) = mantissa.split_once(point).unwrap_or((mantissa, ""));
    if int.is_empty() || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits: String = format!("{int}{frac}");
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some("0".into());
    }
    // Where the point goes in `digits` (leading zeros of the mantissa
    // dropped with them).
    let at = int.trim_start_matches('0').len() as i64 + exponent;
    let digits = digits.trim_end_matches('0');
    let len = digits.len() as i64;
    let (whole, fraction) = if at <= 0 {
        (
            String::from("0"),
            format!("{}{digits}", "0".repeat((-at) as usize)),
        )
    } else if at >= len {
        (
            format!("{digits}{}", "0".repeat((at - len) as usize)),
            String::new(),
        )
    } else {
        (
            digits[..at as usize].to_string(),
            digits[at as usize..].to_string(),
        )
    };
    let count = whole.trim_start_matches('0').len() + fraction.len();
    if count > max_digits {
        return None;
    }
    let mut out = String::from(if negative { "-" } else { "" });
    out.push_str(&whole);
    if !fraction.is_empty() {
        out.push(point);
        out.push_str(&fraction);
    }
    Some(out)
}

fn view_mode_number(mode: CalcMode) -> i64 {
    match mode {
        CalcMode::Standard => 0,
        CalcMode::Scientific => 1,
        CalcMode::Programmer => 2,
    }
}

impl StandardCalculatorViewModel {
    /// `CaptureCalcManagerSnapshot()`
    fn capture_calc_manager_snapshot(&self) -> CalcManagerSnapshot {
        let items = self.standard_calculator_manager.get_history_items();
        if items.is_empty() {
            return CalcManagerSnapshot::default();
        }
        CalcManagerSnapshot {
            history_items: Some(
                items
                    .iter()
                    .map(|h| CalcManagerHistoryItem::from_history_item(h))
                    .collect(),
            ),
        }
    }

    /// `RestoreHistoryItems(CalcManagerSnapshot)`
    fn restore_history_items(&mut self, items: Option<&Vec<CalcManagerHistoryItem>>) {
        let Some(items) = items else {
            return;
        };
        let restored: Vec<Rc<HistoryItem>> = items
            .iter()
            .map(CalcManagerHistoryItem::to_history_item)
            .collect();
        self.with_manager(|m| m.set_history_items(&restored));
    }

    /// `Snapshot` getter (+ the gmnb extension and
    /// `ApplicationViewModel.Snapshot`).
    pub(crate) fn snapshot(&self) -> ApplicationSnapshot {
        let mut result = StandardCalculatorSnapshot {
            calc_manager: self.capture_calc_manager_snapshot(),
            primary_display: PrimaryDisplaySnapshot {
                display_value: self.display_value.clone(),
                is_error: self.is_in_error,
            },
            ..Default::default()
        };
        if !self.tokens.is_empty() && !self.commands.is_empty() {
            result.expression_display = Some(ExpressionDisplaySnapshot {
                tokens: tokens_from_engine(&self.tokens),
                commands: self
                    .commands
                    .iter()
                    .map(ExpressionCommandWrapper::from_command)
                    .collect(),
            });
        }
        result.display_commands = self
            .standard_calculator_manager
            .get_display_commands_snapshot()
            .iter()
            .map(ExpressionCommandWrapper::from_command)
            .collect();

        let history_for = |mode| {
            let items = self
                .standard_calculator_manager
                .get_history_items_for_mode(mode);
            Some(
                items
                    .iter()
                    .map(|h| CalcManagerHistoryItem::from_history_item(h))
                    .collect::<Vec<_>>(),
            )
        };
        let extension = SnapshotExtension {
            standard_history: history_for(CalculatorMode::Standard),
            scientific_history: history_for(CalculatorMode::Scientific),
            memory: self
                .memorized_numbers
                .iter()
                .map(|m| m.value.clone())
                .collect(),
            radix: Some(self.current_radix_type),
            word_size: Some(self.value_bit_length),
            angle: Some(self.angle_unit()),
            fe: self.is_f_to_e_checked,
            shift_mode: Some(self.shift_mode),
            continuation: Some(self.capture_continuation()),
        };

        ApplicationSnapshot {
            mode: view_mode_number(self.get_calculator_mode()),
            standard_calculator: Some(result),
            extension: Some(extension),
        }
    }

    /// `Snapshot` setter, extended with the gmnb extension. The mode has
    /// already been switched to the snapshot's (`RestoreFromSnapshot` sets
    /// `Mode` first).
    pub(crate) fn restore_snapshot(
        &mut self,
        snapshot: &StandardCalculatorSnapshot,
        extension: Option<&SnapshotExtension>,
    ) -> bool {
        // Recall starts a separate session, including empty memory.
        let mode = self.get_calculator_mode();
        let _ = self.with_manager(|m| m.reset(true));
        self.reset_managed_calculator_submodes();

        // Deviation: upstream appends the restored history to whatever the
        // manager holds; restoring replaces it.
        for m in [CalcMode::Standard, CalcMode::Scientific] {
            self.set_native_calculator_mode(m);
            self.with_manager(|mgr| mgr.clear_history());
        }
        self.history_vm.clear_items();

        match extension {
            Some(x) if x.standard_history.is_some() || x.scientific_history.is_some() => {
                // Programmer mode keeps the history of the mode before it as
                // the manager's own ("s"."m"): that mode's goes last.
                let items = |h: &Option<Vec<CalcManagerHistoryItem>>| h.clone().unwrap_or_default();
                let current = items(&snapshot.calc_manager.history_items);
                let order = if mode == CalcMode::Programmer
                    && items(&x.standard_history) == current
                    && items(&x.scientific_history) != current
                {
                    [CalcMode::Scientific, CalcMode::Standard]
                } else {
                    [CalcMode::Standard, CalcMode::Scientific]
                };
                for m in order {
                    self.set_native_calculator_mode(m);
                    self.restore_history_items(if m == CalcMode::Standard {
                        x.standard_history.as_ref()
                    } else {
                        x.scientific_history.as_ref()
                    });
                }
                self.set_native_calculator_mode(mode);
            }
            _ => {
                self.set_native_calculator_mode(mode);
                self.restore_history_items(snapshot.calc_manager.history_items.as_ref());
            }
        }

        if let Some(x) = extension {
            self.restore_extension_submodes(mode, x);
            self.restore_memory(mode, &x.memory);
        }

        // Extension: what the display commands don't hold (see the module
        // docs); `None` for a snapshot that predates it.
        let continuation = extension.and_then(|x| x.continuation.as_ref());
        // The saved equations are in the restored history already: replaying
        // them must not add them again.
        self.with_manager(|m| m.set_history_suppressed(true));
        let display = &snapshot.primary_display.display_value;
        let is_error = snapshot.primary_display.is_error;
        // Extension: the engine shows another value than the display (see
        // the module docs). The engine is restored to its own value; an
        // error that only the display shows (a paste error) isn't the
        // engine's.
        let engine_value = continuation.and_then(|k| k.engine_value.as_deref());
        let value = engine_value.unwrap_or(display);
        let engine_error = is_error && engine_value.is_none();
        if continuation.is_some_and(|k| k.unreplayable) {
            // Extension: the commands wouldn't rebuild this calculation.
            self.restore_new_calculation(mode, display, is_error);
            self.with_manager(|m| m.set_history_suppressed(false));
            self.drain();
            return false;
        }
        // Whether the pending expression was restored (no budget ran out).
        let whole = match &snapshot.expression_display {
            Some(expression) if snapshot.display_commands.is_empty() => {
                // Expression was evaluated before.
                //
                // Deviation: upstream loads it as a history item, which
                // replays it without "=" (so the next "=" evaluates it again
                // instead of repeating its last operation, and the next digit
                // is typed into its last operand), and in Programmer mode
                // continues in the Standard engine. It is evaluated again
                // instead: the result is then exact, as it was, and "="
                // repeats the same operation. Where that doesn't give the
                // saved display and repeated operation back (an operand was
                // only saved to the digits it showed), the value is shown and
                // the saved repeated operation set up as below.
                if engine_error {
                    self.restore_error_display(display);
                } else if !self.reevaluate(&expression.commands, value, continuation) {
                    self.restore_continuation(mode, &[], value, continuation);
                }
                // It is shown as saved either way.
                true
            }
            _ if engine_error => {
                // Expression was not evaluated before, or it was an error.
                let whole = snapshot.expression_display.is_none()
                    || self.replay_within_budget(&snapshot.display_commands);
                self.restore_error_display(display);
                whole
            }
            _ => self.restore_continuation(mode, &snapshot.display_commands, value, continuation),
        };
        if engine_value.is_some() {
            // Extension: the display shown over the engine's value.
            self.set_primary_display(display, is_error);
        }
        // The expression line as it was: the engine's (the replay shows it
        // too), a History item's, or empty though the engine has an
        // expression (any key after "=" clears the line, and MS, M+ or an
        // angle switch don't show it again); unless a budget ran out and
        // the pending expression was dropped. A snapshot without "k"
        // (older, or upstream's) shows an expression it doesn't hold as
        // the replay does.
        match (&snapshot.expression_display, continuation) {
            _ if !whole => {}
            (Some(expression), _) => {
                let tokens = tokens_to_engine(&expression.tokens);
                let commands: Vec<ExpressionCommand> = expression
                    .commands
                    .iter()
                    .map(ExpressionCommandWrapper::to_command)
                    .collect();
                self.set_expression_display(tokens, commands);
            }
            (None, Some(_)) => self.set_expression_display(Vec::new(), Vec::new()),
            (None, None) => {}
        }
        // Extension: F-E as a History selection left it, or enabled.
        self.restore_history_load(continuation.is_some_and(|k| k.history_load));
        // Extension: the carry the next RoL or RoR through carry shifts in
        // (no key sets it; C, which the restore began with, cleared it). An
        // error the engine is in has none: every key clears it.
        if !engine_error && continuation.is_some_and(|k| k.carry) {
            self.with_manager(|m| m.set_carry(true));
        }
        // Extension: the left operand `%` reads with no operator pending
        // (after `=`, the result), where the restore didn't leave it as
        // saved; an exact one an evaluation left is kept.
        if !engine_error
            && let Some(left) = continuation.and_then(|k| k.left.as_ref())
            && self.capture_continuation().left.as_ref() != Some(left)
            && let ExpressionCommand::Operand(operand) = left.to_command()
        {
            let _ = self.with_manager(|m| m.set_left_operand(&operand));
        }
        // Extension: whether the input holds the number typed last (which,
        // with 0 shown, makes the C key CE), which what the restore typed
        // needn't have left so. Snapshots without "k" are left as replayed.
        if let Some(k) = continuation {
            self.with_manager(|m| m.set_input_empty(k.empty_input));
            self.on_input_changed();
        }
        if engine_error && let Some(expression) = &snapshot.expression_display {
            // The parentheses the expression line leaves open, as shown
            // while the error is (the engine's are gone with it).
            let open = expression.commands.iter().fold(0i64, |n, c| match c {
                ExpressionCommandWrapper::Parentheses(cmd::OPENP) => n + 1,
                ExpressionCommandWrapper::Parentheses(cmd::CLOSEP) => n - 1,
                _ => n,
            });
            self.set_parenthesis_count(u32::try_from(open.max(0)).unwrap_or(0));
        }
        // Extension: a calculation that doesn't save again as it was saved
        // wouldn't continue as it would have; start a new one from the
        // saved value instead (see the module docs). An engine error needs
        // no check: every key clears it, as it would have. Snapshots
        // without "k" (older, or upstream's) can't be checked.
        let exact = continuation.is_none() || engine_error || self.saves_as(snapshot, extension);
        if !exact {
            self.restore_new_calculation(mode, display, is_error);
        }
        self.with_manager(|m| m.set_history_suppressed(false));
        self.drain();
        exact
    }

    /// Extension: whether the calculation saves as `snapshot` and
    /// `extension` describe it: the display, the expression line, the
    /// display commands, what `"k"` records and the modes. Memory and the
    /// histories are restored as they were saved and not compared (a slot
    /// comes back as the digits it showed).
    fn saves_as(
        &self,
        snapshot: &StandardCalculatorSnapshot,
        extension: Option<&SnapshotExtension>,
    ) -> bool {
        let now = self.snapshot();
        let (Some(standard), Some(x)) = (now.standard_calculator, now.extension) else {
            return false;
        };
        standard.primary_display == snapshot.primary_display
            && standard.expression_display == snapshot.expression_display
            && standard.display_commands == snapshot.display_commands
            && extension.is_none_or(|e| {
                e.continuation == x.continuation
                    && e.radix.is_none_or(|r| Some(r) == x.radix)
                    && e.word_size.is_none_or(|w| Some(w) == x.word_size)
                    && e.angle.is_none_or(|a| Some(a) == x.angle)
                    && e.fe == x.fe
                    && e.shift_mode.is_none_or(|s| Some(s) == x.shift_mode)
            })
    }

    /// Extension: the fallback for a calculation that can't be restored as
    /// it was: the calculation is cleared and the saved value shown as a
    /// result, so the next digit replaces it and "=" repeats nothing; an
    /// error stays an error. Memory, the histories and the modes are kept.
    fn restore_new_calculation(&mut self, mode: CalcMode, display: &str, is_error: bool) {
        self.clear_unbudgeted();
        if is_error {
            self.restore_error_display(display);
        } else {
            self.show_value(mode, display, ShownValue::Result);
            if self.display_value != display {
                // As it was shown (a number being typed shows its digits
                // even with F-E on).
                self.set_primary_display(display, false);
            }
        }
        self.set_expression_display(Vec::new(), Vec::new());
        self.restore_history_load(false);
    }

    /// Extension: what the engine would save now, beside its display
    /// commands, and what the display shows instead of the engine.
    fn capture_continuation(&self) -> ContinuationSnapshot {
        let c = self.standard_calculator_manager.continuation();
        // The engine's value if the display shows another: a History item's
        // result, or an error the engine isn't in.
        let engine_value = self
            .standard_calculator_manager
            .engine_primary_display()
            .filter(|(_, engine_error)| !engine_error)
            .map(|(text, _)| self.localize_display_value(&text, false))
            .filter(|value| self.is_in_error || *value != self.display_value);
        ContinuationSnapshot {
            shown: c.shown,
            repeat: c
                .repeat
                .map(|(op, operand)| {
                    vec![
                        ExpressionCommandWrapper::Binary(op),
                        ExpressionCommandWrapper::from_command(&ExpressionCommand::Operand(
                            operand,
                        )),
                    ]
                })
                .unwrap_or_default(),
            engine_value,
            history_load: self.is_last_operation_history_load(),
            entry: c.entry,
            clears: c.clears,
            unreplayable: c.unreplayable,
            carry: c.carry,
            left: c.left.map(|operand| {
                ExpressionCommandWrapper::from_command(&ExpressionCommand::Operand(operand))
            }),
            empty_input: c.empty_input,
        }
    }

    /// Extension: evaluates a saved, evaluated expression again (within
    /// [`REPLAY_WORK`]). Keeps the result if it shows `display` and leaves
    /// the saved repeated operation (when the snapshot has one) and kind of
    /// display; otherwise clears it and returns false.
    fn reevaluate(
        &mut self,
        commands: &[ExpressionCommandWrapper],
        display: &str,
        continuation: Option<&ContinuationSnapshot>,
    ) -> bool {
        let within = self.within_work(REPLAY_WORK, |vm| {
            vm.with_deferred_display(|vm| {
                vm.replay(commands);
                vm.send_command(cmd::EQU);
            })
        });
        let same = within
            && !self.is_in_error
            && self.display_value == display
            && continuation.is_none_or(|k| {
                let now = self.capture_continuation();
                (k.shown, &k.repeat, k.entry, k.clears)
                    == (now.shown, &now.repeat, now.entry, now.clears)
            });
        if !same {
            self.clear_unbudgeted();
        }
        same
    }

    /// Extension: restores a calculation the engine isn't in error in from
    /// its display commands and the saved continuation: the operation "="
    /// repeats, then the pending expression and any operand being typed,
    /// then the value the engine showed (`display`) if they don't produce it
    /// (see [`show_value`](Self::show_value)). Each step within its budget;
    /// past one, the calculation is cleared and the value shown, so the
    /// next digit replaces it and "=" doesn't repeat anything. Returns
    /// whether the display commands were replayed whole.
    fn restore_continuation(
        &mut self,
        mode: CalcMode,
        display_commands: &[ExpressionCommandWrapper],
        display: &str,
        continuation: Option<&ContinuationSnapshot>,
    ) -> bool {
        // After "=" with nothing to repeat ("(" came next), the next key
        // still clears the expression line: "1 + 1 =" sets that up, and the
        // "(" in the commands drops what it would repeat.
        let one = [
            ExpressionCommandWrapper::Binary(cmd::ADD),
            ExpressionCommandWrapper::Operand {
                commands: vec![cmd::ZERO + 1],
                is_negative: false,
                is_decimal_present: false,
                is_sci_fmt: false,
            },
        ];
        let repeat = match continuation.map(|k| &k.repeat[..]) {
            Some(repeat @ [_, _]) => Some(repeat),
            _ if continuation.is_some_and(|k| k.clears) => Some(&one[..]),
            _ => None,
        };
        if let Some([op, operand]) = repeat {
            // "1 op operand =": the left operand doesn't matter (1 is valid in
            // every radix and every operator that evaluated before takes it).
            let within = self.within_work(VALUE_WORK, |vm| {
                vm.with_deferred_display(|vm| {
                    vm.send_command(cmd::ZERO + 1);
                    vm.replay(std::slice::from_ref(op));
                    vm.replay(std::slice::from_ref(operand));
                    vm.send_command(cmd::EQU);
                })
            });
            if !within || self.is_in_error {
                self.clear_unbudgeted();
            }
        }
        let entry = continuation.and_then(|k| k.entry);
        // "(" keeps the value shown before it (unless an operator came
        // before it): the value a shown result or recalled value left is
        // set up before the trailing "(", as it was.
        let open = display_commands
            .iter()
            .rposition(|c| *c != ExpressionCommandWrapper::Parentheses(cmd::OPENP))
            .map_or(0, |i| i + 1);
        let (display_commands, parens) = display_commands.split_at(open);
        // The empty input's 0 isn't typed (see `restore_entry`). A number
        // whose sign was changed last is typed with the other sign, then
        // changed: the operand puts the sign after its first digit, which
        // would leave a digit the last command (that "(" multiplies).
        let (commands, signed) = match display_commands {
            [rest @ .., ExpressionCommandWrapper::Operand { .. }]
                if entry == Some(Entry::Empty) =>
            {
                (rest, None)
            }
            [
                rest @ ..,
                ExpressionCommandWrapper::Operand {
                    commands,
                    is_negative,
                    is_decimal_present,
                    is_sci_fmt: false,
                },
            ] if entry == Some(Entry::Signed) => (
                rest,
                Some(ExpressionCommandWrapper::Operand {
                    commands: commands.clone(),
                    is_negative: !is_negative,
                    is_decimal_present: *is_decimal_present,
                    is_sci_fmt: false,
                }),
            ),
            all => (all, None),
        };
        let mut whole = self.replay_within_budget(commands);
        if let Some(operand) = signed.filter(|_| whole) {
            whole = self.replay_within_budget(&[operand]);
            if whole {
                self.send_command(cmd::SIGN);
            }
        }
        if whole {
            self.restore_entry(entry);
        }
        // Without the record (an older snapshot), or if the commands didn't
        // produce the display after all (a budget ran out), a value they
        // don't produce was shown, not typed: a typed operand is in them.
        let shown = continuation
            .and_then(|k| k.shown)
            .or_else(|| (self.display_value != display).then_some(ShownValue::Result));
        if let Some(shown) = shown {
            self.show_value(mode, display, shown);
        }
        if whole && !parens.is_empty() {
            whole = self.replay_within_budget(parens);
        }
        whole
    }

    /// Extension: leaves the number being entered as the saved engine had
    /// it (`"in"`), where replaying the display commands doesn't: an empty
    /// input after C or CE (the commands end with its 0, which typed would
    /// be a digit that "(" multiplies; CE empties it, and the engine as
    /// reset is already empty), an empty input that a radix or angle
    /// switch, MS or M+ ended (F-E twice ends it), or a `%` result, which is
    /// added to the expression rather than typed.
    fn restore_entry(&mut self, entry: Option<Entry>) {
        let recording = self
            .standard_calculator_manager
            .current_calculator_engine()
            .is_some_and(|e| e.f_in_recording_state());
        match entry {
            Some(Entry::Empty)
                if self.standard_calculator_manager.continuation().entry != Some(Entry::Empty) =>
            {
                self.send_command(cmd::CENTR);
            }
            Some(Entry::Ended) if recording => {
                self.send_command(cmd::FE);
                self.send_command(cmd::FE);
            }
            Some(Entry::Percent) if recording => {
                let _ = self.with_manager(|m| m.add_entry_as_percent_result());
            }
            _ => {}
        }
    }

    /// Replays the display commands within [`REPLAY_WORK`]; past it, the
    /// pending expression is dropped (the displayed value is shown by the
    /// caller) and this returns false.
    fn replay_within_budget(&mut self, commands: &[ExpressionCommandWrapper]) -> bool {
        let within = self.within_work(REPLAY_WORK, |vm| {
            vm.with_deferred_display(|vm| vm.replay(commands))
        });
        if !within {
            self.clear_unbudgeted();
        }
        within
    }

    /// Extension: runs `f` with the engine's primary and expression display
    /// updates held back, then shows the last ones once
    /// (`CalculatorManager::begin_deferred_display`). Replaying one command
    /// at a time otherwise copies and re-localizes the whole expression per
    /// command, which is quadratic in a long saved calculation.
    fn with_deferred_display(&mut self, f: impl FnOnce(&mut Self)) {
        self.standard_calculator_manager.begin_deferred_display();
        f(self);
        let _ = self.with_manager(|m| m.end_deferred_display());
    }

    /// `SetPrimaryDisplay(displayValue, true)` for a restored engine error.
    ///
    /// Deviation: upstream only puts the view model into the error state and
    /// leaves the engine running, so the next engine refresh (the radix reset
    /// of a page activation, for one) silently replaces the error with a
    /// number. The engine is put into its error state too
    /// (`CalculatorManager.DisplayPasteError`, which the C++ view model used
    /// for paste errors) before the saved error text is shown. A paste error
    /// (`"ev"`, see the module docs) is only the view model's, as it was: the
    /// engine's calculation is restored under it, so a memory recall or a
    /// paste continues it as before.
    fn restore_error_display(&mut self, display_value: &str) {
        self.with_manager(|m| m.display_paste_error());
        self.set_primary_display(display_value, true);
        self.drain();
    }

    /// Extension: the display commands only describe the expression and an
    /// operand that is still being typed, so a value the engine is merely
    /// showing (after MR, π, "=", F-E, MS, a radix switch…) is not in them
    /// and upstream restores "0" or the last operand. Enter the displayed
    /// value and end its entry as the saved calculation's was: a result
    /// like MR does (`IDC_SET_RESULT`, upstream's "set the result": the
    /// last command is then a recall), a typed number like upstream's
    /// history recall does (F-E twice, which leaves the last command a
    /// digit). Either way the next digit replaces it, as it would have.
    fn show_value(&mut self, mode: CalcMode, display_value: &str, shown: ShownValue) {
        if self.is_in_error || !self.enter_value_within_budget(mode, display_value) {
            return;
        }
        match shown {
            ShownValue::Result => self.send_command(cmd::SET_RESULT),
            ShownValue::EndedEntry => {
                self.send_command(cmd::FE);
                self.send_command(cmd::FE);
            }
            ShownValue::EndedSign => {
                // Its sign changed twice, so ± is the last command.
                self.send_command(cmd::SIGN);
                self.send_command(cmd::SIGN);
                self.send_command(cmd::FE);
                self.send_command(cmd::FE);
            }
        }
    }

    /// [`enter_value`](Self::enter_value) within [`VALUE_WORK`]; past it,
    /// the entry is cleared. Returns whether the value was entered.
    pub(crate) fn enter_value_within_budget(&mut self, mode: CalcMode, value: &str) -> bool {
        let mut entered = false;
        if self.within_work(VALUE_WORK, |vm| entered = vm.enter_value(mode, value)) {
            return entered;
        }
        self.clear_unbudgeted();
        false
    }

    /// Types a displayed number (as `OnPaste` would) into the engine.
    /// Extension: like a paste, the text must pass
    /// `CopyPasteManager.ValidatePasteExpression` first, which bounds its
    /// length and operand count.
    fn enter_value(&mut self, mode: CalcMode, value: &str) -> bool {
        let (view_mode, number_base, bit_length) = match mode {
            CalcMode::Standard => (
                copypaste::ViewMode::Standard,
                copypaste::NumberBase::Unknown,
                copypaste::BitLength::BitLengthUnknown,
            ),
            CalcMode::Scientific => (
                copypaste::ViewMode::Scientific,
                copypaste::NumberBase::Unknown,
                copypaste::BitLength::BitLengthUnknown,
            ),
            CalcMode::Programmer => (
                copypaste::ViewMode::Programmer,
                crate::standard_vm::number_base(self.current_radix_type),
                crate::standard_vm::bit_length(self.value_bit_length),
            ),
        };
        let locale = crate::localization::LocalizationSettings::get_instance().paste_locale();
        // F-E shows numbers in e-notation; typed so, an integer isn't one
        // to the engine ("1.21e+2" isn't 121 as an exponent of a negative
        // base). Type it out where its digits fit.
        let max_digits = if mode == CalcMode::Standard { 16 } else { 32 };
        let plain = (mode != CalcMode::Programmer)
            .then(|| plain_decimal(value, locale.decimal_separator, max_digits))
            .flatten();
        let value = copypaste::validate_paste_expression_localized(
            plain.as_deref().unwrap_or(value),
            view_mode,
            view_mode.group_type(),
            number_base,
            bit_length,
            &locale,
        );
        if copypaste::is_error_message(&value) {
            return false;
        }
        let Ok(keys) = copypaste::calculator_paste_commands(&value, view_mode, &locale) else {
            return false;
        };
        for key in keys {
            self.send_command(paste_command_id(key));
        }
        true
    }

    fn replay(&mut self, commands: &[ExpressionCommandWrapper]) {
        let fe = self.is_f_to_e_checked;
        for (i, command) in commands.iter().enumerate() {
            // An operand was written into the expression by the command
            // after it, in the form F-E gave it then: e-notation, or plain
            // (unless the number needs e-notation). F-E may have been
            // switched since; write it in its own form, then switch back.
            let switch = match i.checked_sub(1).map(|j| &commands[j]) {
                Some(ExpressionCommandWrapper::Operand { is_sci_fmt, .. })
                    if !matches!(command, ExpressionCommandWrapper::Operand { .. }) =>
                {
                    *is_sci_fmt != fe
                }
                _ => false,
            };
            if switch {
                self.send_command(cmd::FE);
            }
            self.replay_one(command);
            if switch {
                self.send_command(cmd::FE);
            }
        }
        // An error ignores F-E, and a spent budget drops it, which would
        // leave the engine in an operand's form once C clears either.
        let engine_fe = self
            .standard_calculator_manager
            .current_calculator_engine()
            .map(|e| e.number_format() != calcmanager::NumberFormat::Float);
        if engine_fe.is_some_and(|engine_fe| engine_fe != fe) {
            self.with_manager(|m| m.set_exponential_format(fe));
        }
    }

    fn replay_one(&mut self, command: &ExpressionCommandWrapper) {
        // A sign change recorded as an operation ("negate(3)") was made to a
        // number whose entry had ended (F-E, MS, a History selection); sent
        // right after its digits it would change the sign of the number
        // being typed instead, and the next digit would extend it. End the
        // entry first, as F-E does.
        if let ExpressionCommandWrapper::Unary(ops) = command
            && ops.last() == Some(&cmd::SIGN)
            && self.standard_calculator_manager.is_engine_recording()
        {
            self.send_command(cmd::FE);
            self.send_command(cmd::FE);
        }
        let angled = matches!(command, ExpressionCommandWrapper::Unary(ops) if ops.len() == 2);
        let command = [command.to_command()];
        for c in crate::standard_vm::get_commands_from_expression_commands(&command) {
            // →deg after a number being typed, as a key press sends it.
            self.end_entry_for(c);
            self.send_command(c);
        }
        if angled {
            // Back to the angle unit shown (the operation was sent with the
            // one it was worked out in).
            self.resync_angle();
        }
    }

    /// Extension: angle unit / F-E (Scientific), word size / radix
    /// (Programmer) and the shift mode.
    fn restore_extension_submodes(&mut self, mode: CalcMode, x: &SnapshotExtension) {
        if let Some(s) = x.shift_mode {
            self.shift_mode = s;
        }
        if let Some(a) = x.angle {
            self.current_angle_type = match a {
                AngleUnit::Degrees => cmd::DEG,
                AngleUnit::Radians => cmd::RAD,
                AngleUnit::Gradians => cmd::GRAD,
            };
        }
        if let Some(w) = x.word_size {
            self.value_bit_length = w;
        }
        match mode {
            CalcMode::Scientific => {
                // Same code path as entering Scientific mode.
                self.set_calculator_type(CalcMode::Scientific);
                if x.fe {
                    self.set_is_f_to_e_checked(true);
                }
            }
            CalcMode::Programmer => {
                self.set_calculator_type(CalcMode::Programmer);
                if let Some(r) = x.radix
                    && r != Radix::Dec
                {
                    self.switch_programmer_mode_base(r);
                }
            }
            CalcMode::Standard => {}
        }
    }

    /// Extension: re-enters the memory strings (oldest first) and stores
    /// each with MS, then clears the entry. Only the newest
    /// [`MAX_RESTORED_MEMORY`] are kept, as the manager would.
    fn restore_memory(&mut self, mode: CalcMode, memory: &[String]) {
        if memory.is_empty() {
            return;
        }
        let memory = &memory[..memory.len().min(MAX_RESTORED_MEMORY)];
        self.with_deferred_display(|vm| {
            // Extension: each slot within VALUE_WORK, all within
            // MEMORY_WORK; a slot past either is dropped.
            vm.within_work(MEMORY_WORK, |vm| {
                for value in memory.iter().rev() {
                    if vm.enter_value_within_budget(mode, value) {
                        let _ = vm.with_manager(|m| m.memorize_number());
                    }
                }
            });
            vm.clear_unbudgeted();
        });
        // A slot keeps the form it was shown in when it was stored (F-E or
        // not) until the whole list is shown again (M+, M−, a mode or
        // radix switch); the restored list was shown in today's form.
        if self.memorized_numbers.len() == memory.len() {
            for (slot, text) in self.memorized_numbers.iter_mut().zip(memory) {
                slot.value.clone_from(text);
            }
        }
    }
}
