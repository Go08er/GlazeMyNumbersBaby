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
        // Ctrl+Alt is never a calculator chord. (v0.1.0 ignored Alt here;
        // rejecting it keeps AltGr-style Ctrl+Alt input from turning into
        // memory or hyperbolic commands.)
        if kp.alt {
            return None;
        }
        return match (lower?, shift) {
            // Not in Programmer, which has no History: upstream's
            // ToggleHistoryFlyout returns at once there.
            ('h', false) if std_or_sci => Some(ToggleHistory),
            ('h', false) => None,
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

    // Named keys: upstream's plain `VirtualKey` dictionary, which Shift
    // doesn't reach (`GetCurrentKeyDictionary` looks a key held with Shift
    // up among the Shift chords, and none of these is one): Shift+Enter is
    // not "=", Shift+Escape not Clear (R18-L-02).
    if let Key::Named(n) = kp.key {
        if shift {
            return None;
        }
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
    // Named keys are the plain dictionary's, as the calculator's
    // ([`shortcut`]): not with Shift.
    if kp.shift && matches!(kp.key, Key::Named(_)) {
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

/// What a key does to a focused control of its own (a button, a toggle or
/// menu button, a radio button, a list row, a calendar day, a link).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKey {
    /// Enter, whatever is held with it, or Space, unless Alt is: activates
    /// it, as a click does (a History item recalls its result).
    Activate,
    /// The arrows, Home/End, Page Up/Down: move within it (between list
    /// rows, radio buttons, a calendar's days and months). With Shift
    /// held too (a calendar's Shift+Page Up is a year), not with Ctrl or
    /// Alt. (A slider takes them whatever is held: [`Focus::Slider`].)
    Navigate,
}

/// The keys a focused control takes itself, before the page's shortcuts
/// (R16-M-03). Upstream's calculator keys (`CalculatorButton`, the bit
/// `FlipButtons`) ignore Enter, which stays "=" there; every other
/// control handles its own, and the shortcut manager skips a key handled.
///
/// What is held makes no difference to Enter and Space (R18-L-02):
/// upstream's controls read the key alone. A button or a toggle is
/// pressed by Space, Enter or a gamepad's A whatever is held
/// (`KeyPress::ButtonBase::IsPress`, ButtonBaseKeyProcess.h, called from
/// `ButtonBase::OnKeyDown` with the key only), a list item is clicked by
/// Enter, by Space and, where the list selects nothing as History and
/// Memory do, by Ctrl+Space (`ListViewBase::OnKeyDown`), and the app's
/// own handlers compare `e.Key` (`MathRichEditBox.OnKeyUp`,
/// `UnitConverter.OnValueKeyDown`). Alt too, then, for Enter. Not for
/// Space: Alt+Space is the window's menu, which Windows opens before a
/// control has the key and which that list skips by name; here it is the
/// window manager's the same way, and no control's.
pub fn control_key(kp: &KeyPress) -> Option<ControlKey> {
    match kp.key {
        Key::Named(Named::Enter) => Some(ControlKey::Activate),
        Key::Char(' ') if !kp.alt => Some(ControlKey::Activate),
        Key::Named(n) if is_navigation(n) && !kp.ctrl && !kp.alt => Some(ControlKey::Navigate),
        _ => None,
    }
}

/// The arrows, Home/End and Page Up/Down.
fn is_navigation(key: Named) -> bool {
    matches!(
        key,
        Named::Up
            | Named::Down
            | Named::Left
            | Named::Right
            | Named::Home
            | Named::End
            | Named::PageUp
            | Named::PageDown
    )
}

/// What has the keyboard's focus, as far as the keys it takes before the
/// page go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// A text field (graph equations, dialogs): every key but the
    /// app-wide chords.
    Text,
    /// One of the page's own keys, upstream's `CalculatorButton`s (every
    /// keypad's and flyout's keys but the 2nd and hyp toggles; MC, MR, M+,
    /// M− and MS) and bit `FlipButtons`: Space presses it and the arrows
    /// move on, but it ignores Enter, as upstream's do: Enter is the
    /// page's ("=" on the calculator, nothing on the converter or the
    /// graph's keypad).
    CalculatorKey,
    /// A slider (a graph variable's, Settings'): the arrows, Home/End and
    /// Page Up/Down move it whatever is held with them, as upstream's
    /// `Slider` reads the key alone (`KeyPress::Slider::KeyDown`,
    /// SliderKeyProcess.h: Ctrl+Right steps as Right does, and Ctrl+Home
    /// is its Home, not the graph's chord). Enter and Space aren't its.
    Slider,
    /// Any other control: a button, a toggle or menu button, a radio
    /// button, a list row, a switch, a calendar day, a link. Its
    /// activation and navigation keys are its own: Enter on a History
    /// item recalls it rather than evaluating (R16-M-03).
    Control,
    /// Nothing that takes keys (no focus, the window, a scroll view, a
    /// label): every key is the page's.
    Other,
}

/// Whose a key press is, given what has the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusKey {
    /// The focused control's: it activates it ([`ControlKey::Activate`];
    /// Enter in a text field, which submits it).
    Activate,
    /// The focused control's: it moves within it
    /// ([`ControlKey::Navigate`]), or moves it (a slider).
    Navigate,
    /// The focused text field's: typed, or an editing key.
    Edit,
    /// The app's and the page's (their shortcuts), and if they have no use
    /// for it the toolkit's (Tab).
    Page,
    /// The app's and the page's, and if they have no use for it nobody's:
    /// Enter on a calculator key, which ignores it whatever is held, as
    /// upstream's does (`CalculatorButton.OnKeyDown` and
    /// `FlipButtons.OnKeyDown` return on `VirtualKey.Enter`, modifiers
    /// unread), so the key isn't pressed. Only the bare Enter is "="
    /// ([`shortcut`]).
    PageOnly,
}

/// Whose `kp` is with `focus` focused: the one rule both apps route by
/// (R18-L-02).
pub fn focus_key(focus: Focus, kp: &KeyPress) -> FocusKey {
    let own = control_key(kp);
    match focus {
        Focus::Text if is_global_chord(kp) => FocusKey::Page,
        Focus::Text if kp.is(Named::Enter) => FocusKey::Activate,
        Focus::Text => FocusKey::Edit,
        Focus::Slider => match kp.key {
            Key::Named(n) if is_navigation(n) => FocusKey::Navigate,
            _ => FocusKey::Page,
        },
        Focus::CalculatorKey if kp.is(Named::Enter) => FocusKey::PageOnly,
        Focus::CalculatorKey | Focus::Control => match own {
            Some(ControlKey::Activate) => FocusKey::Activate,
            Some(ControlKey::Navigate) => FocusKey::Navigate,
            None => FocusKey::Page,
        },
        Focus::Other => FocusKey::Page,
    }
}

/// Which item of a History or Memory list takes the keyboard focus when
/// the one that had it, the `item`th of `was`, went or moved, `now` items
/// being left (not none): the same item if items came above it (the
/// newest is first), else the one now in its place, or the last one, as
/// upstream's lists (HistoryList.xaml.cs:84, Memory.xaml.cs:82: the
/// deleted index, or the last).
pub fn item_after(item: usize, was: usize, now: usize) -> usize {
    (if now > was { item + (now - was) } else { item }).min(now.saturating_sub(1))
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
                    "pageup" => Key::Named(Named::PageUp),
                    "pagedown" => Key::Named(Named::PageDown),
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

    /// Every way of holding Ctrl, Shift and Alt with a key: (ctrl, shift,
    /// alt).
    fn held(kp: KeyPress) -> [KeyPress; 8] {
        let mut all = [kp; 8];
        for (i, k) in all.iter_mut().enumerate() {
            (k.ctrl, k.shift, k.alt) = (i & 1 != 0, i & 2 != 0, i & 4 != 0);
        }
        all
    }

    const NAVIGATION: [Named; 8] = [
        Named::Up,
        Named::Down,
        Named::Left,
        Named::Right,
        Named::Home,
        Named::End,
        Named::PageUp,
        Named::PageDown,
    ];

    /// Enter and Space activate a focused control, the arrows, Home/End
    /// and Page Up/Down move within it. Of those, only Enter is a
    /// calculator key too ("="), so it is the one a focused control takes
    /// from the calculator (R16-M-03). What is held makes no difference
    /// to Enter, as to upstream's controls, which read the key alone
    /// (R18-L-02), nor to Space but for Alt: Alt+Space is the window's
    /// menu. The navigation keys are a control's with Shift too, not with
    /// Ctrl or Alt.
    #[test]
    fn control_keys() {
        use ControlKey::*;
        let enter = KeyPress::named(Named::Enter);
        for kp in held(enter) {
            assert_eq!(control_key(&kp), Some(Activate), "{kp:?}");
        }
        for kp in held(KeyPress::char(' ')) {
            let want = (!kp.alt).then_some(Activate);
            assert_eq!(control_key(&kp), want, "{kp:?}");
        }
        for n in NAVIGATION {
            for kp in held(KeyPress::named(n)) {
                let want = (!kp.alt && !kp.ctrl).then_some(Navigate);
                assert_eq!(control_key(&kp), want, "{kp:?}");
            }
        }
        for kp in [
            KeyPress::char('7'),
            KeyPress::char('='),
            KeyPress::named(Named::Escape),
            KeyPress::named(Named::Backspace),
            KeyPress::named(Named::Delete),
            KeyPress::named(Named::Tab),
        ] {
            assert_eq!(control_key(&kp), None, "{kp:?}");
        }
        for mode in [
            CalcMode::Standard,
            CalcMode::Scientific,
            CalcMode::Programmer,
        ] {
            let calculator = |kp: KeyPress| shortcut(mode, &kp, ShiftMode::Arithmetic);
            assert_eq!(calculator(enter), Some(Action::Press(B::Equals)));
            for n in NAVIGATION {
                assert_eq!(calculator(KeyPress::named(n)), None, "{n:?} in {mode:?}");
            }
            assert_eq!(calculator(KeyPress::char(' ')), None);
        }
    }

    /// R18-L-02: whose Enter and Space are, by what has the focus, with
    /// every modifier held: the rule both apps route by. A button, a
    /// toggle or a list row is activated by Enter whatever is held, Alt
    /// too (upstream's read the key alone), and by Space unless Alt is
    /// held (the window's menu); a calculator key is pressed by Space the
    /// same way and ignores Enter whatever is held, which only bare is
    /// "="; a text field submits on Enter and types Space; a slider takes
    /// neither.
    #[test]
    fn enter_and_space_by_focus_and_modifier() {
        use FocusKey::*;
        for kp in held(KeyPress::named(Named::Enter)) {
            assert_eq!(focus_key(Focus::Control, &kp), Activate, "{kp:?}");
            assert_eq!(focus_key(Focus::CalculatorKey, &kp), PageOnly, "{kp:?}");
            assert_eq!(focus_key(Focus::Text, &kp), Activate, "{kp:?}");
            assert_eq!(focus_key(Focus::Slider, &kp), Page, "{kp:?}");
            assert_eq!(focus_key(Focus::Other, &kp), Page, "{kp:?}");
            // "=" is the bare Enter alone, wherever the focus is.
            let bare = !kp.ctrl && !kp.shift && !kp.alt;
            for mode in [
                CalcMode::Standard,
                CalcMode::Scientific,
                CalcMode::Programmer,
            ] {
                assert_eq!(
                    shortcut(mode, &kp, ShiftMode::Arithmetic),
                    bare.then_some(Action::Press(B::Equals)),
                    "{kp:?} in {mode:?}"
                );
            }
            assert_eq!(converter_shortcut(&kp), None, "{kp:?}");
        }
        for kp in held(KeyPress::char(' ')) {
            let own = if kp.alt { Page } else { Activate };
            assert_eq!(focus_key(Focus::Control, &kp), own, "{kp:?}");
            assert_eq!(focus_key(Focus::CalculatorKey, &kp), own, "{kp:?}");
            assert_eq!(focus_key(Focus::Text, &kp), Edit, "{kp:?}");
            assert_eq!(focus_key(Focus::Slider, &kp), Page, "{kp:?}");
            assert_eq!(focus_key(Focus::Other, &kp), Page, "{kp:?}");
            assert_eq!(sc(CalcMode::Scientific, kp), None, "{kp:?}");
        }
    }

    /// The other keys routed by focus: the navigation keys are a focused
    /// control's (a calculator key's too) bare or with Shift, a slider's
    /// whatever is held (upstream's reads the key alone: Ctrl+Right steps,
    /// Ctrl+Home is its Home), and a text field's whatever is held, but
    /// for Ctrl+Home, the graph view's chord. Everything else (what is
    /// typed, Escape, Backspace, Tab) is the page's from a control and the
    /// field's from a text field, but for Alt+1…5.
    #[test]
    fn the_other_keys_by_focus() {
        use FocusKey::*;
        for n in NAVIGATION {
            for kp in held(KeyPress::named(n)) {
                let own = if kp.alt || kp.ctrl { Page } else { Navigate };
                assert_eq!(focus_key(Focus::Control, &kp), own, "{kp:?}");
                assert_eq!(focus_key(Focus::CalculatorKey, &kp), own, "{kp:?}");
                assert_eq!(focus_key(Focus::Slider, &kp), Navigate, "{kp:?}");
                assert_eq!(focus_key(Focus::Other, &kp), Page, "{kp:?}");
                let field = if is_global_chord(&kp) { Page } else { Edit };
                assert_eq!(focus_key(Focus::Text, &kp), field, "{kp:?}");
            }
        }
        for key in [
            KeyPress::char('7'),
            KeyPress::char('='),
            KeyPress::named(Named::Escape),
            KeyPress::named(Named::Backspace),
            KeyPress::named(Named::Delete),
            KeyPress::named(Named::Tab),
            KeyPress::named(Named::F(9)),
        ] {
            for kp in held(key) {
                for focus in [
                    Focus::Control,
                    Focus::CalculatorKey,
                    Focus::Slider,
                    Focus::Other,
                ] {
                    assert_eq!(focus_key(focus, &kp), Page, "{focus:?} {kp:?}");
                }
                assert_eq!(focus_key(Focus::Text, &kp), Edit, "{kp:?}");
            }
        }
        assert_eq!(focus_key(Focus::Text, &KeyPress::char('2').alt()), Page);
        assert_eq!(
            focus_key(Focus::Text, &KeyPress::named(Named::Home).ctrl()),
            Page
        );
    }

    /// R18-L-02: the named calculator keys are upstream's plain
    /// dictionary's, which a key held with Shift isn't looked up in:
    /// Shift+Enter is not "=", Shift+Escape not Clear, and so on, on the
    /// converter too. What Shift types is unchanged (Shift+R, "@").
    #[test]
    fn named_keys_are_not_shortcuts_with_shift() {
        let named = [
            Named::Enter,
            Named::Escape,
            Named::Delete,
            Named::Backspace,
            Named::F(9),
            Named::F(2),
            Named::F(3),
            Named::F(4),
            Named::F(5),
            Named::F(6),
            Named::F(7),
            Named::F(8),
            Named::F(12),
        ];
        for mode in [
            CalcMode::Standard,
            CalcMode::Scientific,
            CalcMode::Programmer,
        ] {
            for n in named {
                for kp in held(KeyPress::named(n)) {
                    if kp.ctrl || kp.shift || kp.alt {
                        assert_eq!(sc(mode, kp), None, "{kp:?} in {mode:?}");
                    }
                }
            }
            for n in [
                Named::Enter,
                Named::Escape,
                Named::Delete,
                Named::Backspace,
                Named::F(9),
            ] {
                assert!(sc(mode, KeyPress::named(n)).is_some(), "{n:?} in {mode:?}");
            }
        }
        assert_eq!(
            sc(CalcMode::Scientific, KeyPress::named(Named::F(5))),
            Some(Action::Angle(AU::Radians))
        );
        assert_eq!(
            sc(CalcMode::Programmer, KeyPress::named(Named::F(12))),
            Some(Action::Word(W::Byte))
        );
        for n in [Named::Escape, Named::Delete, Named::Backspace, Named::F(9)] {
            assert!(converter_shortcut(&KeyPress::named(n)).is_some(), "{n:?}");
            for kp in held(KeyPress::named(n)) {
                if kp.ctrl || kp.shift || kp.alt {
                    assert_eq!(converter_shortcut(&kp), None, "{kp:?}");
                }
            }
        }
        assert_eq!(
            sc(CalcMode::Scientific, KeyPress::char('R')),
            Some(Action::Press(B::Rand))
        );
        assert_eq!(
            sc(CalcMode::Standard, KeyPress::char('@').shift()),
            Some(Action::Press(B::Sqrt))
        );
    }

    /// The focus after its History or Memory item went: the item now in
    /// its place, or the last; the same item when others came above it.
    #[test]
    fn the_item_that_takes_the_focus_of_one_that_went() {
        // One deleted: the 2nd of 3 (its place), the last of 3 (the last).
        assert_eq!(item_after(1, 3, 2), 1);
        assert_eq!(item_after(2, 3, 2), 1);
        assert_eq!(item_after(0, 3, 2), 0);
        assert_eq!(item_after(0, 2, 1), 0);
        // One added above (the newest is first): the same item.
        assert_eq!(item_after(0, 2, 3), 1);
        assert_eq!(item_after(2, 3, 5), 4);
        // The same number (an item changed in place): where it was.
        assert_eq!(item_after(1, 3, 3), 1);
        // Several went (a list that holds fewer).
        assert_eq!(item_after(4, 5, 2), 1);
    }

    /// Ctrl+H is the History's key where there is a History: not in
    /// Programmer, as upstream's ToggleHistoryFlyout returns at once there
    /// (in a narrow window it opened the Memory sheet, History being
    /// what Programmer hasn't).
    #[test]
    fn ctrl_h_is_the_historys_except_in_programmer() {
        let ctrl_h = KeyPress::char('h').ctrl();
        for mode in [CalcMode::Standard, CalcMode::Scientific] {
            assert_eq!(sc(mode, ctrl_h), Some(Action::ToggleHistory), "{mode:?}");
        }
        assert_eq!(sc(CalcMode::Programmer, ctrl_h), None);
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
