//! Key layouts, labels and keyboard shortcuts for Standard / Scientific /
//! Programmer, transcribed from the upstream XAML and Resources.resw.

use calcvm::{Button as B, CalcMode, ShiftMode};
use gtk::gdk;

use crate::widgets::icon::paths;
use crate::widgets::keypad::{Key, KeyKind};

use KeyKind::{Equals as Eq, Function as Fn_, Number as Num, Operator as Op, Toggle as Tog};

/// Pseudo-ids for keys that are UI toggles, not engine buttons.
pub const KEY_SECOND: u32 = 9001;
pub const KEY_HYP: u32 = 9002;
pub const KEY_TRIG_SECOND: u32 = 9003;

fn k(id: B, label: &str, kind: KeyKind, tip: &str) -> Key {
    Key::new(id.id(), label, kind).tip(tip)
}

/// (key, row, col)
pub type Layout = Vec<(Key, i32, i32)>;

pub fn standard() -> Layout {
    let rows: [[Key; 4]; 6] = [
        [
            k(B::Percent, "%", Fn_, "Percent"),
            k(B::ClearEntry, "CE", Fn_, "Clear entry (Delete)"),
            k(B::Clear, "C", Fn_, "Clear (Esc)"),
            k(B::Backspace, "⌫", Fn_, "Backspace").icon(paths::BACKSPACE),
        ],
        [
            k(
                B::Invert,
                "<sup>1</sup>/<sub>x</sub>",
                Fn_,
                "Reciprocal (R)",
            ),
            k(B::XPower2, "x<sup>2</sup>", Fn_, "Square (Q)"),
            k(B::Sqrt, "<sup>2</sup>√x", Fn_, "Square root (@)"),
            k(B::Divide, "÷", Op, "Divide (/)").icon(paths::DIVIDE),
        ],
        [
            num(B::Seven),
            num(B::Eight),
            num(B::Nine),
            k(B::Multiply, "×", Op, "Multiply (*)").icon(paths::MULTIPLY),
        ],
        [
            num(B::Four),
            num(B::Five),
            num(B::Six),
            k(B::Subtract, "−", Op, "Minus (-)").icon(paths::SUBTRACT),
        ],
        [
            num(B::One),
            num(B::Two),
            num(B::Three),
            k(B::Add, "+", Op, "Plus (+)").icon(paths::ADD),
        ],
        [
            k(B::Negate, "+/−", Num, "Positive negative (F9)"),
            num(B::Zero),
            k(B::Decimal, ".", Num, "Decimal separator"),
            k(B::Equals, "=", Eq, "Equals (Enter)").icon(paths::EQUALS),
        ],
    ];
    grid(rows.into_iter().map(Vec::from).collect())
}

pub fn scientific() -> Layout {
    let rows: Vec<Vec<Key>> = vec![
        vec![
            Key::new(KEY_SECOND, "2<sup>nd</sup>", Tog).tip("Second function"),
            k(B::Pi, "π", Fn_, "Pi (P)"),
            k(B::Euler, "e", Fn_, "Euler's number (Shift+E)"),
            k(B::Clear, "C", Fn_, "Clear (Esc)"),
            k(B::Backspace, "⌫", Fn_, "Backspace").icon(paths::BACKSPACE),
        ],
        vec![
            k(B::XPower2, "x<sup>2</sup>", Fn_, "Square (Q)"),
            k(
                B::Invert,
                "<sup>1</sup>/<sub>x</sub>",
                Fn_,
                "Reciprocal (R)",
            ),
            k(B::Abs, "|x|", Fn_, "Absolute value (|)"),
            k(B::Exp, "exp", Fn_, "Exponential (X)"),
            k(B::Mod, "mod", Fn_, "Modulo (%)"),
        ],
        vec![
            k(B::Sqrt, "<sup>2</sup>√x", Fn_, "Square root (@)"),
            k(B::OpenParenthesis, "(", Fn_, "Left parenthesis"),
            k(B::CloseParenthesis, ")", Fn_, "Right parenthesis"),
            k(B::Factorial, "n!", Fn_, "Factorial (!)"),
            k(B::Divide, "÷", Op, "Divide (/)").icon(paths::DIVIDE),
        ],
        vec![
            k(B::XPowerY, "x<sup>y</sup>", Fn_, "X to the exponent (^)"),
            num(B::Seven),
            num(B::Eight),
            num(B::Nine),
            k(B::Multiply, "×", Op, "Multiply (*)").icon(paths::MULTIPLY),
        ],
        vec![
            k(
                B::TenPowerX,
                "10<sup>x</sup>",
                Fn_,
                "Ten to the exponent (Ctrl+G)",
            ),
            num(B::Four),
            num(B::Five),
            num(B::Six),
            k(B::Subtract, "−", Op, "Minus (-)").icon(paths::SUBTRACT),
        ],
        vec![
            k(B::LogBase10, "log", Fn_, "Log (L)"),
            num(B::One),
            num(B::Two),
            num(B::Three),
            k(B::Add, "+", Op, "Plus (+)").icon(paths::ADD),
        ],
        vec![
            k(B::LogBaseE, "ln", Fn_, "Natural log (N)"),
            k(B::Negate, "+/−", Num, "Positive negative (F9)"),
            num(B::Zero),
            k(B::Decimal, ".", Num, "Decimal separator"),
            k(B::Equals, "=", Eq, "Equals (Enter)").icon(paths::EQUALS),
        ],
    ];
    grid(rows)
}

pub fn programmer() -> Layout {
    let rows: Vec<Vec<Key>> = vec![
        vec![
            k(B::A, "A", Num, "A"),
            k(B::Lsh, "≪", Fn_, "Left shift (<)"),
            k(B::Rsh, "≫", Fn_, "Right shift (>)"),
            k(B::Clear, "C", Fn_, "Clear (Esc)"),
            k(B::Backspace, "⌫", Fn_, "Backspace").icon(paths::BACKSPACE),
        ],
        vec![
            k(B::B, "B", Num, "B"),
            k(B::OpenParenthesis, "(", Fn_, "Left parenthesis"),
            k(B::CloseParenthesis, ")", Fn_, "Right parenthesis"),
            k(B::Mod, "%", Fn_, "Modulo (%)"),
            k(B::Divide, "÷", Op, "Divide (/)").icon(paths::DIVIDE),
        ],
        vec![
            k(B::C, "C", Num, "C"),
            num(B::Seven),
            num(B::Eight),
            num(B::Nine),
            k(B::Multiply, "×", Op, "Multiply (*)").icon(paths::MULTIPLY),
        ],
        vec![
            k(B::D, "D", Num, "D"),
            num(B::Four),
            num(B::Five),
            num(B::Six),
            k(B::Subtract, "−", Op, "Minus (-)").icon(paths::SUBTRACT),
        ],
        vec![
            k(B::E, "E", Num, "E"),
            num(B::One),
            num(B::Two),
            num(B::Three),
            k(B::Add, "+", Op, "Plus (+)").icon(paths::ADD),
        ],
        vec![
            k(B::F, "F", Num, "F"),
            k(B::Negate, "+/−", Num, "Positive negative (F9)"),
            num(B::Zero),
            k(B::Decimal, ".", Num, "Decimal separator"),
            k(B::Equals, "=", Eq, "Equals (Enter)").icon(paths::EQUALS),
        ],
    ];
    grid(rows)
}

/// Trigonometry flyout (2×4): toggles + six functions.
pub fn trig() -> Layout {
    grid(vec![
        vec![
            Key::new(KEY_TRIG_SECOND, "2<sup>nd</sup>", Tog).tip("Inverse functions"),
            k(B::Sin, "sin", Fn_, "Sine (S)"),
            k(B::Cos, "cos", Fn_, "Cosine (O)"),
            k(B::Tan, "tan", Fn_, "Tangent (T)"),
        ],
        vec![
            Key::new(KEY_HYP, "hyp", Tog).tip("Hyperbolic functions"),
            k(B::Sec, "sec", Fn_, "Secant (U)"),
            k(B::Csc, "csc", Fn_, "Cosecant (I)"),
            k(B::Cot, "cot", Fn_, "Cotangent (J)"),
        ],
    ])
}

/// Function flyout (2×3).
pub fn functions() -> Layout {
    grid(vec![
        vec![
            k(B::Abs, "|x|", Fn_, "Absolute value (|)"),
            k(B::Floor, "⌊x⌋", Fn_, "Floor ([)"),
            k(B::Ceil, "⌈x⌉", Fn_, "Ceiling (])"),
        ],
        vec![
            k(B::Rand, "rand", Fn_, "Random (Shift+R)"),
            k(B::DMS, "→dms", Fn_, "Degrees minutes seconds (M)"),
            k(B::Degrees, "→deg", Fn_, "Degrees (Ctrl+D)"),
        ],
    ])
}

/// Bitwise flyout (2×3).
pub fn bitwise() -> Layout {
    grid(vec![
        vec![
            k(B::And, "AND", Fn_, "And (&)"),
            k(B::Or, "OR", Fn_, "Or (|)"),
            k(B::Not, "NOT", Fn_, "Not (~)"),
        ],
        vec![
            k(B::Nand, "NAND", Fn_, "Nand (.)"),
            k(B::Nor, "NOR", Fn_, "Nor (\\)"),
            k(B::Xor, "XOR", Fn_, "Exclusive or (^)"),
        ],
    ])
}

fn num(b: B) -> Key {
    let d = b.digit_value().unwrap_or(0);
    Key::new(b.id(), &d.to_string(), Num).a11y(&d.to_string())
}

fn grid(rows: Vec<Vec<Key>>) -> Layout {
    let mut out = Vec::new();
    for (r, row) in rows.into_iter().enumerate() {
        for (c, key) in row.into_iter().enumerate() {
            out.push((key, r as i32, c as i32));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 2nd / hyp remapping
// ---------------------------------------------------------------------------

/// Scientific keypad keys whose meaning flips with "2nd":
/// (normal, second, second-label)
pub const SECOND_FLIPS: [(B, B, &str, &str); 6] = [
    (B::XPower2, B::Cube, "x<sup>2</sup>", "x<sup>3</sup>"),
    (B::Sqrt, B::CubeRoot, "<sup>2</sup>√x", "<sup>3</sup>√x"),
    (B::XPowerY, B::YRootX, "x<sup>y</sup>", "<sup>y</sup>√x"),
    (
        B::TenPowerX,
        B::TwoPowerX,
        "10<sup>x</sup>",
        "2<sup>x</sup>",
    ),
    (B::LogBase10, B::LogBaseY, "log", "log<sub>y</sub>x"),
    (B::LogBaseE, B::EPowerX, "ln", "e<sup>x</sup>"),
];

/// Trig keys: base → (inverse, hyperbolic, inverse hyperbolic), label stem.
pub const TRIG: [(B, B, B, B, &str); 6] = [
    (B::Sin, B::InvSin, B::Sinh, B::InvSinh, "sin"),
    (B::Cos, B::InvCos, B::Cosh, B::InvCosh, "cos"),
    (B::Tan, B::InvTan, B::Tanh, B::InvTanh, "tan"),
    (B::Sec, B::InvSec, B::Sech, B::InvSech, "sec"),
    (B::Csc, B::InvCsc, B::Csch, B::InvCsch, "csc"),
    (B::Cot, B::InvCot, B::Coth, B::InvCoth, "cot"),
];

pub fn trig_label(stem: &str, inv: bool, hyp: bool) -> String {
    format!(
        "{stem}{}{}",
        if hyp { "h" } else { "" },
        if inv { "<sup>-1</sup>" } else { "" }
    )
}

pub fn resolve_trig(base: B, inv: bool, hyp: bool) -> B {
    TRIG.iter()
        .find(|t| t.0 == base)
        .map(|t| match (inv, hyp) {
            (false, false) => t.0,
            (true, false) => t.1,
            (false, true) => t.2,
            (true, true) => t.3,
        })
        .unwrap_or(base)
}

pub fn resolve_second(base: B, second: bool) -> B {
    if !second {
        return base;
    }
    SECOND_FLIPS
        .iter()
        .find(|f| f.0 == base)
        .map(|f| f.1)
        .unwrap_or(base)
}

/// The two shift keys' engine buttons and labels for a shift mode.
pub fn shift_keys(mode: ShiftMode) -> ((B, &'static str), (B, &'static str)) {
    match mode {
        ShiftMode::Arithmetic => ((B::Lsh, "≪"), (B::Rsh, "≫")),
        ShiftMode::Logical => ((B::Lsh, "≪"), (B::RshL, "≫")),
        ShiftMode::Rotate => ((B::Rol, "RoL"), (B::Ror, "RoR")),
        ShiftMode::RotateThroughCarry => ((B::RolC, "RoL"), (B::RorC, "RoR")),
    }
}

// ---------------------------------------------------------------------------
// Keyboard shortcuts (Resources.resw KeyboardShortcutManager entries)
// ---------------------------------------------------------------------------

/// Non-engine actions reachable from the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Press(B),
    ToggleHistory,
    ClearHistory,
    Angle(calcvm::AngleUnit),
    Radix(calcvm::Radix),
    Word(calcvm::WordSize),
}

pub fn shortcut(
    mode: CalcMode,
    key: gdk::Key,
    mods: gdk::ModifierType,
    shift_mode: ShiftMode,
) -> Option<Action> {
    use Action::*;
    use calcvm::{AngleUnit as AU, Radix as R, WordSize as W};
    let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
    let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
    let sci = mode == CalcMode::Scientific;
    let prog = mode == CalcMode::Programmer;
    let std_or_sci = mode != CalcMode::Programmer;
    let lower = key.to_lower();

    // Control (+Shift) chords.
    if ctrl {
        return match (lower, shift) {
            (gdk::Key::h, false) => Some(ToggleHistory),
            (gdk::Key::d, true) => Some(ClearHistory),
            (gdk::Key::m, false) => Some(Press(B::Memory)),
            (gdk::Key::l, false) => Some(Press(B::MemoryClear)),
            (gdk::Key::r, false) => Some(Press(B::MemoryRecall)),
            (gdk::Key::p, false) => Some(Press(B::MemoryAdd)),
            (gdk::Key::q, false) => Some(Press(B::MemorySubtract)),
            (gdk::Key::s, false) if sci => Some(Press(B::Sinh)),
            (gdk::Key::o, false) if sci => Some(Press(B::Cosh)),
            (gdk::Key::t, false) if sci => Some(Press(B::Tanh)),
            (gdk::Key::u, false) if sci => Some(Press(B::Sech)),
            (gdk::Key::i, false) if sci => Some(Press(B::Csch)),
            (gdk::Key::j, false) if sci => Some(Press(B::Coth)),
            (gdk::Key::s, true) if sci => Some(Press(B::InvSinh)),
            (gdk::Key::o, true) if sci => Some(Press(B::InvCosh)),
            (gdk::Key::t, true) if sci => Some(Press(B::InvTanh)),
            (gdk::Key::u, true) if sci => Some(Press(B::InvSech)),
            (gdk::Key::i, true) if sci => Some(Press(B::InvCsch)),
            (gdk::Key::j, true) if sci => Some(Press(B::InvCoth)),
            (gdk::Key::g, false) if sci => Some(Press(B::TenPowerX)),
            (gdk::Key::y, false) if sci => Some(Press(B::YRootX)),
            (gdk::Key::n, false) if sci => Some(Press(B::EPowerX)),
            (gdk::Key::d, false) if sci => Some(Press(B::Degrees)),
            _ => None,
        };
    }
    if mods.contains(gdk::ModifierType::ALT_MASK) {
        return None;
    }

    // Named keys.
    let named = match key {
        gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => Some(Press(B::Equals)),
        gdk::Key::Escape => Some(Press(B::Clear)),
        gdk::Key::Delete | gdk::Key::KP_Delete => Some(Press(B::ClearEntry)),
        gdk::Key::BackSpace => Some(Press(B::Backspace)),
        gdk::Key::F9 => Some(Press(B::Negate)),
        gdk::Key::KP_Decimal | gdk::Key::KP_Separator if std_or_sci => Some(Press(B::Decimal)),
        gdk::Key::F3 if sci => Some(Angle(AU::Gradians)),
        gdk::Key::F4 if sci => Some(Angle(AU::Degrees)),
        gdk::Key::F5 if sci => Some(Angle(AU::Radians)),
        gdk::Key::F5 if prog => Some(Radix(R::Hex)),
        gdk::Key::F6 if prog => Some(Radix(R::Dec)),
        gdk::Key::F7 if prog => Some(Radix(R::Oct)),
        gdk::Key::F8 if prog => Some(Radix(R::Bin)),
        gdk::Key::F2 if prog => Some(Word(W::Qword)),
        gdk::Key::F3 if prog => Some(Word(W::Dword)),
        gdk::Key::F4 if prog => Some(Word(W::Word)),
        gdk::Key::F12 if prog => Some(Word(W::Byte)),
        _ => None,
    };
    if named.is_some() {
        return named;
    }

    // Letter "virtual keys" (with and without Shift).
    if prog {
        if let Some(d) = [
            gdk::Key::a,
            gdk::Key::b,
            gdk::Key::c,
            gdk::Key::d,
            gdk::Key::e,
            gdk::Key::f,
        ]
        .iter()
        .position(|k| *k == lower)
        {
            return Some(Press(B::DIGITS[10 + d]));
        }
    }
    let letter = match (lower, shift) {
        (gdk::Key::r, false) if std_or_sci => Some(B::Invert),
        (gdk::Key::q, false) if std_or_sci => Some(B::XPower2),
        (gdk::Key::s, false) if sci => Some(B::Sin),
        (gdk::Key::o, false) if sci => Some(B::Cos),
        (gdk::Key::t, false) if sci => Some(B::Tan),
        (gdk::Key::u, false) if sci => Some(B::Sec),
        (gdk::Key::i, false) if sci => Some(B::Csc),
        (gdk::Key::j, false) if sci => Some(B::Cot),
        (gdk::Key::s, true) if sci => Some(B::InvSin),
        (gdk::Key::o, true) if sci => Some(B::InvCos),
        (gdk::Key::t, true) if sci => Some(B::InvTan),
        (gdk::Key::u, true) if sci => Some(B::InvSec),
        (gdk::Key::i, true) if sci => Some(B::InvCsc),
        (gdk::Key::j, true) if sci => Some(B::InvCot),
        (gdk::Key::x, false) if sci => Some(B::Exp),
        (gdk::Key::m, false) if sci => Some(B::DMS),
        (gdk::Key::v, false) if sci => Some(B::FToE),
        (gdk::Key::l, false) if sci => Some(B::LogBase10),
        (gdk::Key::l, true) if sci => Some(B::LogBaseY),
        (gdk::Key::n, false) if sci => Some(B::LogBaseE),
        (gdk::Key::p, false) if sci => Some(B::Pi),
        (gdk::Key::y, false) if sci => Some(B::XPowerY),
        (gdk::Key::g, false) if sci => Some(B::TwoPowerX),
        (gdk::Key::b, false) if sci => Some(B::CubeRoot),
        (gdk::Key::r, true) if sci => Some(B::Rand),
        (gdk::Key::e, true) if sci => Some(B::Euler),
        _ => None,
    };
    if let Some(b) = letter {
        return Some(Press(b));
    }

    // Typed characters.
    let ch = key.to_unicode()?;
    let (lsh, rsh) = shift_keys(shift_mode);
    let b = match ch {
        '0'..='9' => B::DIGITS[ch as usize - '0' as usize],
        '.' | ',' if std_or_sci => B::Decimal,
        '.' if prog => B::Nand,
        '/' => B::Divide,
        '*' => B::Multiply,
        '-' => B::Subtract,
        '+' => B::Add,
        '=' => B::Equals,
        '%' if mode == CalcMode::Standard => B::Percent,
        '%' => B::Mod,
        '(' if !std_or_sci || sci => B::OpenParenthesis,
        ')' if !std_or_sci || sci => B::CloseParenthesis,
        '!' if sci => B::Factorial,
        '@' if std_or_sci => B::Sqrt,
        '^' if sci => B::XPowerY,
        '^' if prog => B::Xor,
        '#' if sci => B::Cube,
        '|' if sci => B::Abs,
        '|' if prog => B::Or,
        '~' if prog => B::Not,
        '&' if prog => B::And,
        '\\' if prog => B::Nor,
        '<' if prog => lsh.0,
        '>' if prog => rsh.0,
        '[' if sci => B::Floor,
        ']' if sci => B::Ceil,
        _ => return None,
    };
    Some(Press(b))
}
