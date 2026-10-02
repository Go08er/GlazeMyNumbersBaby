//! Toolkit-neutral key events and the app's keyboard map (upstream
//! `KeyboardShortcutManager`, from Resources.resw). Each UI converts its
//! native key events into [`KeyPress`] and asks this module what they mean.

use calcvm::{AngleUnit as AU, Button as B, CalcMode, Radix as R, ShiftMode, WordSize as W};

use crate::keys::shift_keys;
use crate::modes::ViewMode;

/// Keys that don't produce a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Named {
    Enter,
    Escape,
    Backspace,
    Delete,
    Insert,
    Tab,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    /// F1…F24.
    F(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Named(Named),
    /// The character the key produces with the current modifiers and layout
    /// (`'R'` for Shift+R, `'@'` for Shift+2 on a US layout).
    Char(char),
}

/// One key press, with modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyPress {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// From the numeric keypad (keypad `.` is always a decimal point).
    pub keypad: bool,
}

impl KeyPress {
    pub fn named(n: Named) -> Self {
        Self::new(Key::Named(n))
    }

    pub fn char(c: char) -> Self {
        Self {
            shift: c.is_uppercase(),
            ..Self::new(Key::Char(c))
        }
    }

    pub fn new(key: Key) -> Self {
        KeyPress {
            key,
            ctrl: false,
            shift: false,
            alt: false,
            keypad: false,
        }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn is(&self, n: Named) -> bool {
        self.key == Key::Named(n)
    }

    /// The produced character, if any.
    pub fn text(&self) -> Option<char> {
        match self.key {
            Key::Char(c) => Some(c),
            Key::Named(_) => None,
        }
    }

    /// Letter keys compared case-insensitively.
    fn lower(&self) -> Option<char> {
        self.text().map(|c| c.to_ascii_lowercase())
    }

    fn digit(&self) -> Option<u32> {
        self.text().and_then(|c| c.to_digit(10))
    }

    fn plain(&self) -> bool {
        !self.ctrl && !self.alt
    }
}

/// Non-engine actions reachable from the keyboard on a calculator page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Press(B),
    ToggleHistory,
    ClearHistory,
    Angle(AU),
    Radix(R),
    Word(W),
}

/// What a key press means on a Standard/Scientific/Programmer page.
pub fn shortcut(mode: CalcMode, kp: &KeyPress, shift_mode: ShiftMode) -> Option<Action> {
    use Action::*;
    let (ctrl, shift) = (kp.ctrl, kp.shift);
    let sci = mode == CalcMode::Scientific;
    let prog = mode == CalcMode::Programmer;
    let std_or_sci = mode != CalcMode::Programmer;
    let lower = kp.lower();

    // Control (+Shift) chords.
    if ctrl {
        if kp.alt {
            return None;
        }
        return match (lower?, shift) {
            ('h', false) => Some(ToggleHistory),
            ('d', true) => Some(ClearHistory),
            ('m', false) => Some(Press(B::Memory)),
            ('l', false) => Some(Press(B::MemoryClear)),
            ('r', false) => Some(Press(B::MemoryRecall)),
            ('p', false) => Some(Press(B::MemoryAdd)),
            ('q', false) => Some(Press(B::MemorySubtract)),
            ('s', false) if sci => Some(Press(B::Sinh)),
            ('o', false) if sci => Some(Press(B::Cosh)),
            ('t', false) if sci => Some(Press(B::Tanh)),
            ('u', false) if sci => Some(Press(B::Sech)),
            ('i', false) if sci => Some(Press(B::Csch)),
            ('j', false) if sci => Some(Press(B::Coth)),
            ('s', true) if sci => Some(Press(B::InvSinh)),
            ('o', true) if sci => Some(Press(B::InvCosh)),
            ('t', true) if sci => Some(Press(B::InvTanh)),
            ('u', true) if sci => Some(Press(B::InvSech)),
            ('i', true) if sci => Some(Press(B::InvCsch)),
            ('j', true) if sci => Some(Press(B::InvCoth)),
            ('g', false) if sci => Some(Press(B::TenPowerX)),
            ('y', false) if sci => Some(Press(B::YRootX)),
            ('n', false) if sci => Some(Press(B::EPowerX)),
            ('d', false) if sci => Some(Press(B::Degrees)),
            _ => None,
        };
    }
    if kp.alt {
        return None;
    }

    // Named keys.
    if let Key::Named(n) = kp.key {
        return match n {
            Named::Enter => Some(Press(B::Equals)),
            Named::Escape => Some(Press(B::Clear)),
            Named::Delete => Some(Press(B::ClearEntry)),
            Named::Backspace => Some(Press(B::Backspace)),
            Named::F(9) => Some(Press(B::Negate)),
            Named::F(3) if sci => Some(Angle(AU::Gradians)),
            Named::F(4) if sci => Some(Angle(AU::Degrees)),
            Named::F(5) if sci => Some(Angle(AU::Radians)),
            Named::F(5) if prog => Some(Radix(R::Hex)),
            Named::F(6) if prog => Some(Radix(R::Dec)),
            Named::F(7) if prog => Some(Radix(R::Oct)),
            Named::F(8) if prog => Some(Radix(R::Bin)),
            Named::F(2) if prog => Some(Word(W::Qword)),
            Named::F(3) if prog => Some(Word(W::Dword)),
            Named::F(4) if prog => Some(Word(W::Word)),
            Named::F(12) if prog => Some(Word(W::Byte)),
            _ => None,
        };
    }
    let ch = kp.text()?;
    // The keypad's decimal key types the decimal point whatever the layout's
    // separator is.
    if kp.keypad && matches!(ch, '.' | ',') && std_or_sci {
        return Some(Press(B::Decimal));
    }

    // Letter "virtual keys" (with and without Shift).
    if prog && let Some(d) = "abcdef".find(lower?) {
        return Some(Press(B::DIGITS[10 + d]));
    }
    let letter = match (lower?, shift) {
        ('r', false) if std_or_sci => Some(B::Invert),
        ('q', false) if std_or_sci => Some(B::XPower2),
        ('s', false) if sci => Some(B::Sin),
        ('o', false) if sci => Some(B::Cos),
        ('t', false) if sci => Some(B::Tan),
        ('u', false) if sci => Some(B::Sec),
        ('i', false) if sci => Some(B::Csc),
        ('j', false) if sci => Some(B::Cot),
        ('s', true) if sci => Some(B::InvSin),
        ('o', true) if sci => Some(B::InvCos),
        ('t', true) if sci => Some(B::InvTan),
        ('u', true) if sci => Some(B::InvSec),
        ('i', true) if sci => Some(B::InvCsc),
        ('j', true) if sci => Some(B::InvCot),
        ('x', false) if sci => Some(B::Exp),
        ('m', false) if sci => Some(B::DMS),
        ('v', false) if sci => Some(B::FToE),
        ('l', false) if sci => Some(B::LogBase10),
        ('l', true) if sci => Some(B::LogBaseY),
        ('n', false) if sci => Some(B::LogBaseE),
        ('p', false) if sci => Some(B::Pi),
        ('y', false) if sci => Some(B::XPowerY),
        ('g', false) if sci => Some(B::TwoPowerX),
        ('b', false) if sci => Some(B::CubeRoot),
        ('r', true) if sci => Some(B::Rand),
        ('e', true) if sci => Some(B::Euler),
        _ => None,
    };
    if let Some(b) = letter {
        return Some(Press(b));
    }

    // Typed characters.
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

/// What a key press means on a converter page: a converter keypad id.
pub fn converter_shortcut(kp: &KeyPress) -> Option<u32> {
    use crate::keys::conv;
    if !kp.plain() {
        return None;
    }
    Some(match kp.key {
        Key::Named(Named::Escape | Named::Delete) => conv::CLEAR,
        Key::Named(Named::Backspace) => conv::BACK,
        Key::Named(Named::F(9)) => conv::NEGATE,
        Key::Char('-') => conv::NEGATE,
        Key::Char('.' | ',') => conv::DECIMAL,
        Key::Char(c) => conv::DIGIT0 + c.to_digit(10)?,
        Key::Named(_) => return None,
    })
}

/// Window-level shortcuts, handled before the page sees the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    /// Alt+1…5 jump between the calculators (upstream access keys).
    SwitchMode(ViewMode),
    Copy,
    Paste,
}

pub fn window_shortcut(kp: &KeyPress) -> Option<WindowAction> {
    if kp.alt
        && !kp.ctrl
        && let Some(d) = kp.digit()
        && let Some(mode) = ViewMode::ALL
            .into_iter()
            .find(|m| m.alt_number() == Some(d))
    {
        return Some(WindowAction::SwitchMode(mode));
    }
    if kp.ctrl && !kp.alt {
        match kp.key {
            Key::Char('c' | 'C') | Key::Named(Named::Insert) => return Some(WindowAction::Copy),
            Key::Char('v' | 'V') => return Some(WindowAction::Paste),
            _ => {}
        }
    }
    if kp.shift && !kp.ctrl && !kp.alt && kp.is(Named::Insert) {
        return Some(WindowAction::Paste);
    }
    None
}

/// Shortcuts that work even while a text field has focus: mode switching
/// (Alt+1…5) and upstream's Ctrl+Home "graph view". Everything else belongs
/// to the text field.
pub fn is_global_chord(kp: &KeyPress) -> bool {
    (kp.alt && !kp.ctrl && matches!(kp.digit(), Some(1..=5)))
        || (kp.ctrl && !kp.alt && kp.is(Named::Home))
}

/// Graphing page chords.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphAction {
    ZoomIn,
    ZoomOut,
    ResetView,
    ShowGraph,
}

pub fn graph_shortcut(kp: &KeyPress) -> Option<GraphAction> {
    if !kp.ctrl || kp.alt {
        return None;
    }
    match kp.key {
        Key::Char('+' | '=') => Some(GraphAction::ZoomIn),
        Key::Char('-') => Some(GraphAction::ZoomOut),
        Key::Char('0') => Some(GraphAction::ResetView),
        Key::Named(Named::Home) => Some(GraphAction::ShowGraph),
        _ => None,
    }
}

/// Keep-on-top (compact overlay) chords, Standard mode only: `Some(true)`
/// for Alt+Up, `Some(false)` for Alt+Down.
pub fn compact_shortcut(kp: &KeyPress) -> Option<bool> {
    if !kp.alt || kp.ctrl {
        return None;
    }
    match kp.key {
        Key::Named(Named::Up) => Some(true),
        Key::Named(Named::Down) => Some(false),
        _ => None,
    }
}

/// `alt+3`, `ctrl+home`, `ctrl+shift+d` → a key press.
pub fn parse_chord(chord: &str) -> Option<KeyPress> {
    let mut kp = KeyPress::new(Key::Char('\0'));
    let mut key = None;
    for part in chord.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "alt" => kp.alt = true,
            "ctrl" => kp.ctrl = true,
            "shift" => kp.shift = true,
            name => {
                key = Some(match name {
                    "home" => Key::Named(Named::Home),
                    "end" => Key::Named(Named::End),
                    "up" => Key::Named(Named::Up),
                    "down" => Key::Named(Named::Down),
                    "left" => Key::Named(Named::Left),
                    "right" => Key::Named(Named::Right),
                    "enter" | "return" => Key::Named(Named::Enter),
                    "escape" | "esc" => Key::Named(Named::Escape),
                    "backspace" => Key::Named(Named::Backspace),
                    "delete" => Key::Named(Named::Delete),
                    "insert" => Key::Named(Named::Insert),
                    "tab" => Key::Named(Named::Tab),
                    "plus" => Key::Char('+'),
                    "minus" => Key::Char('-'),
                    f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
                        Key::Named(Named::F(f[1..].parse().ok()?))
                    }
                    s => {
                        let mut it = s.chars();
                        let c = it.next()?;
                        if it.next().is_some() {
                            return None;
                        }
                        Key::Char(c)
                    }
                });
            }
        }
    }
    kp.key = key?;
    Some(kp)
}

/// Dev/screenshot input scripts: plain characters are typed, `\n` is Enter,
/// `\x08` Backspace, `\x1b` Escape, and `{alt+3}`, `{ctrl+home}` are chords.
pub fn parse_key_script(text: &str) -> Vec<KeyPress> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            let chord: String = chars.by_ref().take_while(|&c| c != '}').collect();
            out.extend(parse_chord(&chord));
            continue;
        }
        out.push(match c {
            '\n' => KeyPress::named(Named::Enter),
            '\x08' => KeyPress::named(Named::Backspace),
            '\x1b' => KeyPress::named(Named::Escape),
            c => KeyPress::char(c),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(mode: CalcMode, kp: KeyPress) -> Option<Action> {
        shortcut(mode, &kp, ShiftMode::Arithmetic)
    }

    #[test]
    fn typed_characters_and_letters() {
        use CalcMode::*;
        assert_eq!(
            sc(Standard, KeyPress::char('7')),
            Some(Action::Press(B::Seven))
        );
        assert_eq!(
            sc(Standard, KeyPress::char('%')),
            Some(Action::Press(B::Percent))
        );
        assert_eq!(
            sc(Scientific, KeyPress::char('%')),
            Some(Action::Press(B::Mod))
        );
        assert_eq!(
            sc(Scientific, KeyPress::char('E')),
            Some(Action::Press(B::Euler))
        );
        assert_eq!(sc(Scientific, KeyPress::char('e')), None);
        assert_eq!(
            sc(Programmer, KeyPress::char('e')),
            Some(Action::Press(B::E))
        );
        assert_eq!(
            sc(Programmer, KeyPress::char('.')),
            Some(Action::Press(B::Nand))
        );
        assert_eq!(sc(Standard, KeyPress::char('(')), None);
        assert_eq!(
            sc(Scientific, KeyPress::char('S')),
            Some(Action::Press(B::InvSin))
        );
        assert_eq!(
            sc(Scientific, KeyPress::char('s').ctrl().shift()),
            Some(Action::Press(B::InvSinh))
        );
        assert_eq!(
            sc(Standard, KeyPress::char('D').ctrl()),
            Some(Action::ClearHistory)
        );
        assert_eq!(
            sc(Standard, KeyPress::named(Named::Enter)),
            Some(Action::Press(B::Equals))
        );
        assert_eq!(
            sc(Programmer, KeyPress::named(Named::F(5))),
            Some(Action::Radix(R::Hex))
        );
        assert_eq!(
            sc(Scientific, KeyPress::named(Named::F(5))),
            Some(Action::Angle(AU::Radians))
        );
        let mut kp_comma = KeyPress::char(',');
        kp_comma.keypad = true;
        assert_eq!(sc(Standard, kp_comma), Some(Action::Press(B::Decimal)));
        assert_eq!(sc(Standard, KeyPress::char('7').alt()), None);
    }

    #[test]
    fn programmer_shift_keys_follow_shift_mode() {
        let kp = KeyPress::char('>');
        assert_eq!(
            shortcut(CalcMode::Programmer, &kp, ShiftMode::Rotate),
            Some(Action::Press(B::Ror))
        );
    }

    #[test]
    fn global_chords_pass_through_text_fields() {
        assert!(is_global_chord(&KeyPress::char('1').alt()));
        assert!(is_global_chord(&KeyPress::char('5').alt()));
        assert!(is_global_chord(&KeyPress::named(Named::Home).ctrl()));
        assert!(!is_global_chord(&KeyPress::char('1')));
        assert!(!is_global_chord(&KeyPress::char('9').alt()));
        assert!(!is_global_chord(&KeyPress::named(Named::Home)));
        assert!(!is_global_chord(&KeyPress::char('c').ctrl()));
        assert!(!is_global_chord(&KeyPress::named(Named::Backspace)));
    }

    #[test]
    fn window_and_page_chords() {
        assert_eq!(
            window_shortcut(&KeyPress::char('3').alt()),
            Some(WindowAction::SwitchMode(ViewMode::Graphing))
        );
        assert_eq!(
            window_shortcut(&KeyPress::char('v').ctrl()),
            Some(WindowAction::Paste)
        );
        assert_eq!(
            window_shortcut(&KeyPress::named(Named::Insert).shift()),
            Some(WindowAction::Paste)
        );
        assert_eq!(
            graph_shortcut(&KeyPress::char('0').ctrl()),
            Some(GraphAction::ResetView)
        );
        assert_eq!(
            compact_shortcut(&KeyPress::named(Named::Up).alt()),
            Some(true)
        );
        assert_eq!(
            converter_shortcut(&KeyPress::char('4')),
            Some(crate::keys::conv::DIGIT0 + 4)
        );
        assert_eq!(converter_shortcut(&KeyPress::char('x')), None);
    }

    #[test]
    fn chord_tokens_and_scripts_parse() {
        assert_eq!(parse_chord("alt+3"), Some(KeyPress::char('3').alt()));
        assert_eq!(
            parse_chord("ctrl+home"),
            Some(KeyPress::named(Named::Home).ctrl())
        );
        assert_eq!(
            parse_chord("ctrl+shift+d"),
            Some(KeyPress::new(Key::Char('d')).ctrl().shift())
        );
        assert_eq!(parse_chord("f9"), Some(KeyPress::named(Named::F(9))));
        assert_eq!(parse_chord("ctrl+bogus"), None);
        let s = parse_key_script("1+2\n{alt+2}");
        assert_eq!(s.len(), 5);
        assert_eq!(s[3], KeyPress::named(Named::Enter));
        assert_eq!(s[4], KeyPress::char('2').alt());
    }
}
