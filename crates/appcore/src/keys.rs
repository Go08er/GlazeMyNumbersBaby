//! Key layouts and labels for Standard / Scientific / Programmer (from the
//! upstream XAML and Resources.resw), the converter keypad and the graphing
//! keypad. Labels use a tiny markup subset: `<sup>…</sup>` and `<sub>…</sub>`.

use calcvm::{Button as B, ShiftMode};

use crate::icons as paths;

/// How a key is styled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Number,
    Operator,
    Function,
    Equals,
    Toggle,
}

/// Description of one key.
#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub id: u32,
    /// Label markup (`<sup>`/`<sub>` only).
    pub label: String,
    pub kind: KeyKind,
    pub tooltip: Option<String>,
    /// Accessible name (screen readers); falls back to the tooltip.
    pub a11y: Option<String>,
    /// Draw this icon (see [`crate::icons`]) instead of the label.
    pub icon: Option<&'static str>,
}

impl Key {
    pub fn new(id: impl Into<u32>, label: &str, kind: KeyKind) -> Self {
        Key {
            id: id.into(),
            label: label.to_string(),
            kind,
            tooltip: None,
            a11y: None,
            icon: None,
        }
    }
    pub fn tip(mut self, tooltip: &str) -> Self {
        self.tooltip = Some(tooltip.to_string());
        self
    }
    pub fn a11y(mut self, name: &str) -> Self {
        self.a11y = Some(name.to_string());
        self
    }
    pub fn icon(mut self, path: &'static str) -> Self {
        self.icon = Some(path);
        self
    }

    /// What assistive technology should call this key.
    pub fn accessible_name(&self) -> String {
        self.a11y
            .clone()
            .or_else(|| self.tooltip.clone())
            .unwrap_or_else(|| plain_label(&self.label))
    }
}

/// Markup label → plain text ("x<sup>2</sup>" → "x2").
pub fn plain_label(markup: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in markup.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

use KeyKind::{Equals as Equ, Function as Fn_, Number as Num, Operator as Op, Toggle as Tog};

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
            k(B::Equals, "=", Equ, "Equals (Enter)").icon(paths::EQUALS),
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
            k(B::Equals, "=", Equ, "Equals (Enter)").icon(paths::EQUALS),
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
            k(B::Equals, "=", Equ, "Equals (Enter)").icon(paths::EQUALS),
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

/// A Scientific key whose meaning flips with "2nd".
pub struct SecondFlip {
    pub normal: B,
    pub second: B,
    pub normal_label: &'static str,
    pub second_label: &'static str,
    pub normal_tip: &'static str,
    pub second_tip: &'static str,
}

impl SecondFlip {
    pub fn button(&self, second: bool) -> B {
        if second { self.second } else { self.normal }
    }
    pub fn label(&self, second: bool) -> &'static str {
        if second {
            self.second_label
        } else {
            self.normal_label
        }
    }
    pub fn tip(&self, second: bool) -> &'static str {
        if second {
            self.second_tip
        } else {
            self.normal_tip
        }
    }
}

const fn flip(
    normal: B,
    second: B,
    normal_label: &'static str,
    second_label: &'static str,
    normal_tip: &'static str,
    second_tip: &'static str,
) -> SecondFlip {
    SecondFlip {
        normal,
        second,
        normal_label,
        second_label,
        normal_tip,
        second_tip,
    }
}

pub const SECOND_FLIPS: [SecondFlip; 6] = [
    flip(
        B::XPower2,
        B::Cube,
        "x<sup>2</sup>",
        "x<sup>3</sup>",
        "Square (Q)",
        "Cube (#)",
    ),
    flip(
        B::Sqrt,
        B::CubeRoot,
        "<sup>2</sup>√x",
        "<sup>3</sup>√x",
        "Square root (@)",
        "Cube root (B)",
    ),
    flip(
        B::XPowerY,
        B::YRootX,
        "x<sup>y</sup>",
        "<sup>y</sup>√x",
        "X to the exponent (^)",
        "Y root of x (Ctrl+Y)",
    ),
    flip(
        B::TenPowerX,
        B::TwoPowerX,
        "10<sup>x</sup>",
        "2<sup>x</sup>",
        "Ten to the exponent (Ctrl+G)",
        "Two to the exponent (G)",
    ),
    flip(
        B::LogBase10,
        B::LogBaseY,
        "log",
        "log<sub>y</sub>x",
        "Log (L)",
        "Log base y (Shift+L)",
    ),
    flip(
        B::LogBaseE,
        B::EPowerX,
        "ln",
        "e<sup>x</sup>",
        "Natural log (N)",
        "E to the exponent (Ctrl+N)",
    ),
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

/// Tooltip (and accessible name) for a trig key in its current state.
pub fn trig_tip(stem: &str, inv: bool, hyp: bool) -> String {
    let (name, key) = match stem {
        "sin" => ("sine", 'S'),
        "cos" => ("cosine", 'O'),
        "tan" => ("tangent", 'T'),
        "sec" => ("secant", 'U'),
        "csc" => ("cosecant", 'I'),
        "cot" => ("cotangent", 'J'),
        other => return other.to_string(),
    };
    let text = match (inv, hyp) {
        (false, false) => format!("{name} ({key})"),
        (true, false) => format!("arc {name} (Shift+{key})"),
        (false, true) => format!("hyperbolic {name} (Ctrl+{key})"),
        (true, true) => format!("inverse hyperbolic {name} (Ctrl+Shift+{key})"),
    };
    capitalize(&text)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

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
        .find(|f| f.normal == base)
        .map(|f| f.second)
        .unwrap_or(base)
}

/// Tooltips for the two shift keys in a shift mode.
pub fn shift_tips(mode: ShiftMode) -> (&'static str, &'static str) {
    match mode {
        ShiftMode::Arithmetic => ("Left shift (<)", "Right shift (>)"),
        ShiftMode::Logical => ("Left shift (<)", "Logical right shift (>)"),
        ShiftMode::Rotate => ("Rotate left (<)", "Rotate right (>)"),
        ShiftMode::RotateThroughCarry => (
            "Rotate left through carry (<)",
            "Rotate right through carry (>)",
        ),
    }
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
// Converter keypad
// ---------------------------------------------------------------------------

/// Converter keypad ids (converter commands aren't calculator buttons).
pub mod conv {
    pub const CLEAR: u32 = 1;
    pub const BACK: u32 = 2;
    pub const NEGATE: u32 = 3;
    pub const DECIMAL: u32 = 4;
    pub const DIGIT0: u32 = 10;
}

/// The converter keypad: (key, row, col, row span, col span).
pub fn converter() -> Vec<(Key, i32, i32, i32, i32)> {
    let mut out = vec![
        (
            Key::new(conv::CLEAR, "CE", KeyKind::Function).tip("Clear entry (Esc)"),
            0,
            0,
            1,
            2,
        ),
        (
            Key::new(conv::BACK, "⌫", KeyKind::Function)
                .tip("Backspace")
                .icon(paths::BACKSPACE),
            0,
            2,
            1,
            1,
        ),
    ];
    for d in 1..=9u32 {
        let r = 3 - ((d - 1) / 3) as i32;
        let c = ((d - 1) % 3) as i32;
        out.push((
            Key::new(conv::DIGIT0 + d, &d.to_string(), KeyKind::Number),
            r,
            c,
            1,
            1,
        ));
    }
    out.push((
        Key::new(conv::NEGATE, "+/−", KeyKind::Number).tip("Positive negative (F9)"),
        4,
        0,
        1,
        1,
    ));
    out.push((Key::new(conv::DIGIT0, "0", KeyKind::Number), 4, 1, 1, 1));
    out.push((
        Key::new(conv::DECIMAL, ".", KeyKind::Number).tip("Decimal separator"),
        4,
        2,
        1,
        1,
    ));
    out
}

/// Converter keypad id → view-model command.
pub fn converter_command(id: u32) -> Option<unitconv::Command> {
    use unitconv::Command;
    Some(match id {
        conv::CLEAR => Command::Clear,
        conv::BACK => Command::Backspace,
        conv::NEGATE => Command::Negate,
        conv::DECIMAL => Command::Decimal,
        d if (conv::DIGIT0..conv::DIGIT0 + 10).contains(&d) => {
            Command::from_digit(d - conv::DIGIT0)?
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Graphing keypad
// ---------------------------------------------------------------------------

/// The graphing keypad: (label, text inserted at the cursor). `"\u{8}"`
/// deletes the character before the cursor.
pub const GRAPH_PAD: &[&[(&str, &str)]] = &[
    &[
        ("x", "x"),
        ("y", "y"),
        ("π", "π"),
        ("e", "e"),
        ("^", "^"),
        ("√", "sqrt("),
        ("|x|", "abs("),
    ],
    &[
        ("sin", "sin("),
        ("cos", "cos("),
        ("tan", "tan("),
        ("ln", "ln("),
        ("log", "log("),
        ("(", "("),
        (")", ")"),
    ],
    &[
        ("7", "7"),
        ("8", "8"),
        ("9", "9"),
        ("÷", "/"),
        ("<", "<"),
        ("≤", "<="),
        ("⌫", "\u{8}"),
    ],
    &[
        ("4", "4"),
        ("5", "5"),
        ("6", "6"),
        ("×", "*"),
        (">", ">"),
        ("≥", ">="),
        ("=", "="),
    ],
    &[
        ("1", "1"),
        ("2", "2"),
        ("3", "3"),
        ("−", "-"),
        ("n!", "!"),
        ("⌊x⌋", "floor("),
        ("⌈x⌉", "ceil("),
    ],
    &[
        ("0", "0"),
        (".", "."),
        (",", ","),
        ("+", "+"),
        ("sec", "sec("),
        ("csc", "csc("),
        ("cot", "cot("),
    ],
];

/// Spoken name for a graphing keypad key.
pub fn graph_pad_name(label: &str) -> &str {
    match label {
        "π" => "pi",
        "^" => "power",
        "√" => "square root",
        "|x|" => "absolute value",
        "÷" => "divide",
        "×" => "multiply",
        "−" => "minus",
        "+" => "plus",
        "<" => "less than",
        ">" => "greater than",
        "≤" => "less than or equal",
        "≥" => "greater than or equal",
        "=" => "equals",
        "⌫" => "backspace",
        "n!" => "factorial",
        "⌊x⌋" => "floor",
        "⌈x⌉" => "ceiling",
        "(" => "left parenthesis",
        ")" => "right parenthesis",
        "," => "comma",
        "." => "decimal point",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_labels_strip_markup() {
        assert_eq!(plain_label("x<sup>2</sup>"), "x2");
        assert_eq!(plain_label("<sup>1</sup>/<sub>x</sub>"), "1/x");
    }

    #[test]
    fn layouts_have_unique_positions_and_ids() {
        for layout in [
            standard(),
            scientific(),
            programmer(),
            trig(),
            functions(),
            bitwise(),
        ] {
            let mut pos: Vec<_> = layout.iter().map(|(_, r, c)| (*r, *c)).collect();
            pos.sort();
            pos.dedup();
            assert_eq!(pos.len(), layout.len());
            for (k, _, _) in &layout {
                assert!(!k.accessible_name().is_empty());
            }
        }
    }

    #[test]
    fn second_flips_resolve_and_describe_both_states() {
        for f in &SECOND_FLIPS {
            assert_eq!(resolve_second(f.normal, true), f.second);
            assert_eq!(resolve_second(f.normal, false), f.normal);
            assert_ne!(f.tip(false), f.tip(true));
        }
        assert_eq!(trig_tip("sin", false, false), "Sine (S)");
        assert_eq!(
            trig_tip("cos", true, true),
            "Inverse hyperbolic cosine (Ctrl+Shift+O)"
        );
    }

    #[test]
    fn converter_keys_map_to_commands() {
        for (k, ..) in converter() {
            assert!(converter_command(k.id).is_some(), "{}", k.label);
        }
    }
}
