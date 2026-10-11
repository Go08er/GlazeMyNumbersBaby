//! Standard / Scientific / Programmer.

use appcore::KeyPress;
use appcore::input::{self, Action};
use appcore::keys::{self, KEY_HYP, KEY_SECOND, KEY_TRIG_SECOND, Key, KeyKind};
use calcvm::{AngleUnit, Button as B, CalcMode, CalculatorViewModel, Radix, ShiftMode, WordSize};

use crate::app::{Cx, Msg as AppMsg};
use crate::gfx::Rect;
use crate::ui::{self, Align, CAPTION, Frame, KeyLook, SMALL, STRONG, Style, id};

pub const WIDE: f32 = 620.0;
const PANEL_W: f32 = 300.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    History,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Popup {
    Trig,
    Functions,
    Bitwise,
    Shift,
    /// History/memory as a sheet when the window is narrow.
    Panel,
    /// Right-click on the display.
    DisplayMenu(f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MemOp {
    Recall,
    Add,
    Subtract,
    Clear,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Key(u32),
    Tab(Tab),
    HistoryRecall(usize),
    HistoryRemove(usize),
    HistoryClear,
    Memory(MemOp, usize),
    MemoryClearAll,
    OpenPanel(Tab),
    Popup(Option<Popup>),
    Angle,
    FToE,
    Radix(Radix),
    Word,
    Shift(ShiftMode),
    BitView(bool),
    FlipBit(u32),
    Copy,
    Paste,
}

pub struct CalcPage {
    pub vm: CalculatorViewModel,
    second: bool,
    trig_inv: bool,
    hyp: bool,
    pub popup: Option<Popup>,
    tab: Tab,
    bit_view: bool,
    /// What is still to be told to assistive tech, oldest first
    /// ([`CalcPage::display`]): a frame tells one thing.
    say: std::collections::VecDeque<String>,
    /// How many frames have told something: each one's live node is a new
    /// one.
    told: u64,
    /// Whether this showing of "(" with the focus has been announced.
    paren_focused: bool,
}

/// What of the History and Memory panel has the focus: a History row (or
/// its Delete), a Memory row (or the `j`th of its MC, M+, M−), the key
/// that clears either list, or the keypad's MC or MR, which an empty
/// memory disables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PanelFocus {
    History(usize, bool),
    HistoryClear,
    Memory(usize, Option<usize>),
    MemoryClear,
    MemoryKey,
}

/// [`PanelFocus`] before a change, and how long the lists were.
struct PanelBefore {
    at: Option<PanelFocus>,
    history: usize,
    memory: usize,
}

/// The Bit shift flyout's modes, in order (rows `("shift", i)`).
const SHIFT_MODES: [(ShiftMode, &str); 4] = [
    (ShiftMode::Arithmetic, "Arithmetic shift"),
    (ShiftMode::Logical, "Logical shift"),
    (ShiftMode::Rotate, "Rotate circular shift"),
    (
        ShiftMode::RotateThroughCarry,
        "Rotate through carry circular shift",
    ),
];

fn msg(m: Msg) -> AppMsg {
    AppMsg::Calc(m)
}

impl CalcPage {
    pub fn new(saved: Option<serde_json::Value>) -> CalcPage {
        let mut vm = CalculatorViewModel::new();
        if let Some(s) = saved.and_then(|v| v.as_str().map(str::to_string)) {
            vm.restore_state(&s);
        }
        vm.take_events();
        // What a restored session shows at the start isn't told as news.
        vm.take_announcements();
        CalcPage {
            vm,
            second: false,
            trig_inv: false,
            hyp: false,
            popup: None,
            tab: Tab::History,
            bit_view: false,
            say: std::collections::VecDeque::new(),
            told: 0,
            paren_focused: false,
        }
    }

    /// Where the focus goes in the open popup when the keys open it, if
    /// not on its first control: the Bit shift flyout's chosen mode, as
    /// GMNB's radio buttons.
    pub fn popup_focus(&self) -> Option<crate::ui::Id> {
        let cur = self.vm.shift_mode();
        match self.popup {
            Some(Popup::Shift) => SHIFT_MODES
                .iter()
                .position(|(m, _)| *m == cur)
                .map(|i| id(("shift", i))),
            _ => None,
        }
    }

    pub fn save(&self) -> serde_json::Value {
        serde_json::Value::String(self.vm.save_state())
    }

    /// Back from another page: upstream sets the calculator up again
    /// (`SetCalculatorType`) even in the mode it was in.
    pub fn reactivate(&mut self, mode: CalcMode) {
        if self.vm.mode() == mode {
            self.vm.set_mode(mode);
            self.vm.take_events();
        }
    }

    pub fn set_mode(&mut self, mode: CalcMode) {
        if self.vm.mode() != mode {
            // Upstream's checkDefaultBitShift
            // (CalculatorProgrammerRadixOperators.xaml.cs:44-58), which
            // OnIsProgrammerPropertyChanged runs (Calculator.xaml.cs:408,
            // by EnsureProgrammer and OperatorsPanel.EnsureProgrammerRadixOps)
            // when the calculator turns to Programmer from Standard or
            // Scientific: it starts on Arithmetic shift. Not when Programmer
            // is shown again, after another page, nor when a restored
            // session's Programmer is shown first: that session goes on
            // with its shift mode, as it would have uninterrupted.
            if mode == CalcMode::Programmer {
                self.vm.set_shift_mode(ShiftMode::Arithmetic);
            }
            self.vm.set_mode(mode);
            self.popup = None;
            self.vm.take_events();
        }
        if mode == CalcMode::Programmer && self.tab == Tab::History {
            self.tab = Tab::Memory;
        }
    }

    // ------------------------------------------------------------ actions

    fn press(&mut self, button: B) {
        let mode = self.vm.mode();
        let mut b = button;
        if mode == CalcMode::Scientific {
            b = keys::resolve_second(b, self.second);
            b = keys::resolve_trig(b, self.trig_inv, self.hyp);
        }
        if mode == CalcMode::Programmer {
            let (l, r) = keys::shift_keys(self.vm.shift_mode());
            if b == B::Lsh {
                b = l.0;
            } else if b == B::Rsh {
                b = r.0;
            }
        }
        if b == B::Clear && mode != CalcMode::Standard && self.vm.shows_clear_entry() {
            b = B::ClearEntry;
        }
        if !self.vm.is_enabled(b) {
            return;
        }
        self.vm.press(b);
        self.after_press();
    }

    fn after_press(&mut self) {
        self.vm.take_events();
    }

    /// A focus change follows anything the engine has already said, and
    /// precedes what pressing the newly focused key will say. Keep it in
    /// the same one-at-a-time queue: two live nodes added in one frame
    /// reach AccessKit in no particular order.
    fn announce_paren_focus(&mut self) {
        let said = self.vm.take_announcements();
        self.say.extend(said.iter().map(keys::announcement));
        self.say
            .push_back(keys::open_parenthesis_count_announcement(
                self.vm.open_parens(),
            ));
        self.paren_focused = true;
    }

    /// What of the History or Memory list (or the keys that empty them)
    /// has the focus now, and how long the lists are: for
    /// [`CalcPage::follow_panel`], after a change.
    fn panel_focus(&self, focus: Option<ui::Id>) -> PanelBefore {
        let (history, memory) = (self.vm.history().len(), self.vm.memory().len());
        let at = focus.and_then(|f| {
            use PanelFocus::*;
            let row = |n: usize, of: &dyn Fn(usize) -> Option<PanelFocus>| (0..n).find_map(of);
            if f == id("hist-clear") {
                Some(HistoryClear)
            } else if f == id("mem-clear") {
                Some(MemoryClear)
            } else if [B::MemoryClear, B::MemoryRecall]
                .iter()
                .any(|b| f == id(("mem", *b as u32)))
            {
                Some(MemoryKey)
            } else {
                row(history, &|i| {
                    (f == id(("hist", i)))
                        .then_some(History(i, false))
                        .or((f == id(("hist-del", i))).then_some(History(i, true)))
                })
                .or_else(|| {
                    row(memory, &|i| {
                        (f == id(("memrow", i)))
                            .then_some(Memory(i, None))
                            .or_else(|| {
                                (0..3).find_map(|j| {
                                    (f == id(("memop", i, j))).then_some(Memory(i, Some(j)))
                                })
                            })
                    })
                })
            }
        });
        PanelBefore {
            at,
            history,
            memory,
        }
    }

    /// Keeps the focus somewhere sensible when the History or Memory row
    /// it was on (`before`) goes or moves, as upstream's does
    /// (HistoryList.xaml.cs:71-95, Memory.xaml.cs:68-93,
    /// Calculator.xaml.cs:714-744): on the item now in the deleted one's
    /// place, or the last one; when the list is emptied, on the History
    /// tab (upstream's docked pivot) or on M+ (`MemPlus`), the Memory
    /// sheet closed first if it was open over the keys. A row keeps the
    /// focus when items are added above it. The same when a whole list is
    /// cleared with the focus in it, or on the key that cleared it.
    fn follow_panel(&mut self, before: PanelBefore, cx: &mut Cx) {
        use PanelFocus::*;
        let (history, memory) = (self.vm.history().len(), self.vm.memory().len());
        // The same item if items came above it; else its place, or the last.
        let moved = input::item_after;
        let memory_emptied = |page: &mut CalcPage, cx: &mut Cx| {
            if page.popup == Some(Popup::Panel) {
                page.popup = None;
            }
            *cx.focus = Some(id(("mem", B::MemoryAdd as u32)));
        };
        match before.at {
            Some(History(..) | HistoryClear) if history == 0 && before.history > 0 => {
                *cx.focus = Some(id("tab-history"));
            }
            Some(History(row, delete)) if history != before.history => {
                let row = moved(row, before.history, history);
                *cx.focus = Some(if delete {
                    id(("hist-del", row))
                } else {
                    id(("hist", row))
                });
            }
            Some(Memory(..) | MemoryClear | MemoryKey) if memory == 0 && before.memory > 0 => {
                memory_emptied(self, cx);
            }
            Some(Memory(row, op)) if memory != before.memory => {
                let row = moved(row, before.memory, memory);
                *cx.focus = Some(match op {
                    Some(j) => id(("memop", row, j)),
                    None => id(("memrow", row)),
                });
            }
            _ => {}
        }
    }

    pub fn update(&mut self, m: Msg, cx: &mut Cx) {
        let before = self.panel_focus(*cx.focus);
        self.update_page(m, cx);
        self.follow_panel(before, cx);
    }

    fn update_page(&mut self, m: Msg, cx: &mut Cx) {
        match m {
            Msg::Key(KEY_SECOND) => self.second = !self.second,
            Msg::Key(KEY_TRIG_SECOND) => self.trig_inv = !self.trig_inv,
            Msg::Key(KEY_HYP) => self.hyp = !self.hyp,
            Msg::Key(k) => {
                if let Some(b) = B::from_id(k) {
                    // One of the keys 2nd turned into its second function
                    // (x³, ³√x, ʸ√x, 2ˣ, logᵧx, eˣ), upstream's InvRow1
                    // buttons, whose Click is ShiftButton_Uncheck
                    // (CalculatorScientificOperators.xaml:1089-1140,
                    // .xaml.cs:65): 2nd is unchecked and takes the focus.
                    // A typed shortcut runs the key's command alone, there
                    // as here (KeyboardShortcutManager.RunButtonCommand).
                    let second_function = self.second
                        && self.vm.mode() == CalcMode::Scientific
                        && keys::SECOND_FLIPS.iter().any(|f| f.normal == b);
                    // A click on "(" gave it the focus just before it
                    // pressed it: its focus tells the count then, as
                    // upstream's GotFocus comes before the Click (the view
                    // announces it, CalcPage::view).
                    if b == B::OpenParenthesis
                        && self.vm.mode() != CalcMode::Standard
                        && *cx.focus == Some(id(("keypad", k)))
                        && !self.paren_focused
                    {
                        self.announce_paren_focus();
                    }
                    self.press(b);
                    if second_function {
                        self.second = false;
                        *cx.focus = Some(id(("keypad", KEY_SECOND)));
                    }
                    // Upstream's FlyoutButton_Clicked
                    // (CalculatorScientificOperators.xaml.cs:82), every key
                    // of the Trigonometry and Function flyouts': the trig
                    // flyout's 2nd and hyp are unchecked, whichever key it
                    // was (a trig function or not), and the flyout closes.
                    if matches!(self.popup, Some(Popup::Trig | Popup::Functions)) {
                        self.trig_inv = false;
                        self.hyp = false;
                    }
                    if matches!(
                        self.popup,
                        Some(Popup::Trig | Popup::Functions | Popup::Bitwise)
                    ) {
                        self.popup = None;
                    }
                }
            }
            Msg::Tab(t) => self.tab = t,
            Msg::HistoryRecall(i) => {
                self.vm.history_recall(i);
                self.after_press();
                if !cx.wide {
                    self.popup = None;
                }
            }
            Msg::HistoryRemove(i) => self.vm.history_remove(i),
            Msg::HistoryClear => self.vm.history_clear(),
            Msg::Memory(op, i) => {
                match op {
                    MemOp::Recall => self.vm.memory_recall(i),
                    MemOp::Add => self.vm.memory_add(i),
                    MemOp::Subtract => self.vm.memory_subtract(i),
                    MemOp::Clear => self.vm.memory_clear(i),
                }
                self.after_press();
                if op == MemOp::Recall && !cx.wide {
                    self.popup = None;
                }
            }
            Msg::MemoryClearAll => self.press(B::MemoryClear),
            Msg::OpenPanel(tab) => {
                // The same opener closes its sheet; the other selects
                // the list it names instead of keeping the previous tab.
                let close = self.popup == Some(Popup::Panel) && self.tab == tab;
                self.tab = tab;
                self.popup = (!close).then_some(Popup::Panel);
            }
            Msg::Popup(p) => self.popup = if self.popup == p { None } else { p },
            Msg::Angle => {
                let next = self.vm.angle_unit().next();
                self.vm.set_angle_unit(next);
            }
            Msg::FToE => self.press(B::FToE),
            Msg::Radix(r) => self.vm.set_radix(r),
            Msg::Word => {
                let next = self.vm.word_size().next();
                self.vm.set_word_size(next);
            }
            // Another mode closes the flyout, as upstream's
            // (BitshiftFlyout_Checked); the one already chosen changes
            // nothing and leaves it open, as GMNB's radio buttons.
            Msg::Shift(s) => {
                if self.vm.shift_mode() != s {
                    self.vm.set_shift_mode(s);
                    self.popup = None;
                }
            }
            Msg::BitView(on) => self.bit_view = on,
            Msg::FlipBit(i) => self.vm.flip_bit(i),
            Msg::Copy => {
                self.popup = None;
                cx.copy(&self.vm.copy_text());
            }
            Msg::Paste => {
                self.popup = None;
                if let Some(t) = cx.paste() {
                    self.paste(&t);
                }
            }
        }
        self.vm.take_events();
    }

    pub fn paste(&mut self, text: &str) {
        // The view model shows upstream's "Invalid input" itself on rejection.
        self.vm.paste(text);
        self.after_press();
    }

    pub fn copy_text(&self) -> String {
        self.vm.copy_text().to_string()
    }

    /// Keyboard input; true if handled.
    pub fn key(&mut self, kp: &KeyPress, cx: &mut Cx) -> bool {
        let before = self.panel_focus(*cx.focus);
        let handled = self.key_page(kp, cx);
        self.follow_panel(before, cx);
        handled
    }

    fn key_page(&mut self, kp: &KeyPress, cx: &mut Cx) -> bool {
        let (mode, shift) = (self.vm.mode(), self.vm.shift_mode());
        let Some(action) = input::shortcut(mode, kp, shift) else {
            return false;
        };
        match action {
            Action::Press(b) => {
                if !self.vm.is_enabled(b) {
                    return true;
                }
                self.vm.press(b);
                self.after_press();
            }
            // Ctrl+H. Docked, it chooses the History list. In a narrow
            // window it is the header's History button (OpenPanel): the
            // sheet opens on History, shows History if it was showing
            // Memory, and closes if History is what it was showing, as
            // upstream's key shows the History flyout unless that is the
            // one open (ToggleHistoryFlyout).
            Action::ToggleHistory if cx.wide => self.tab = Tab::History,
            Action::ToggleHistory => self.update_page(Msg::OpenPanel(Tab::History), cx),
            Action::ClearHistory => self.vm.history_clear(),
            Action::Angle(u) => self.vm.set_angle_unit(u),
            Action::Radix(r) => self.vm.set_radix(r),
            Action::Word(w) => self.vm.set_word_size(w),
        }
        self.vm.take_events();
        true
    }

    // ------------------------------------------------------------ view

    pub fn view(&mut self, f: &mut Frame, area: Rect, compact: bool) {
        let mode = self.vm.mode();
        let wide = area.w >= WIDE && !compact;
        let (main, panel) = if wide {
            let (p, m) = area.take_right(PANEL_W);
            (m, Some(p))
        } else {
            (area, None)
        };
        let main = main.inset_xy(8.0, 4.0);
        let display_h = if compact {
            86.0
        } else {
            match mode {
                CalcMode::Standard => 116.0,
                CalcMode::Scientific => 104.0,
                CalcMode::Programmer => 84.0,
            }
        };
        let open = id(("keypad", B::OpenParenthesis.id()));
        if f.nodes.is_none() || mode == CalcMode::Standard || !f.focused(open) {
            self.paren_focused = false;
        } else if !self.paren_focused {
            self.announce_paren_focus();
        }
        let (display, mut rest) = main.take_top(display_h);
        self.display(f, display, compact);

        if mode == CalcMode::Scientific {
            let (row, r) = rest.take_top(32.0);
            rest = r;
            let unit = match self.vm.angle_unit() {
                AngleUnit::Degrees => "DEG",
                AngleUnit::Radians => "RAD",
                AngleUnit::Gradians => "GRAD",
            };
            let (a, row) = row.take_left(64.0);
            f.button(
                id("angle"),
                a,
                unit,
                CAPTION,
                msg(Msg::Angle),
                true,
                None,
                false,
            );
            describe(
                f,
                appcore::keys::angle_button_name(self.vm.angle_unit()),
                "Switch angle unit (F3/F4/F5)",
            );
            let (b, _) = row.take_left(52.0);
            f.button(
                id("fe"),
                b,
                "F-E",
                CAPTION,
                msg(Msg::FToE),
                self.vm.is_enabled(B::FToE),
                Some(self.vm.is_fe()),
                false,
            );
            describe(
                f,
                appcore::keys::FTOE_BUTTON_NAME,
                "Scientific notation (V)",
            );
        }
        if mode == CalcMode::Programmer {
            let (radix, r) = rest.take_top(4.0 * 26.0 + 4.0);
            rest = r;
            self.radix_list(f, radix);
            let (bar, r) = rest.take_top(36.0);
            rest = r;
            self.programmer_bar(f, bar);
        } else if !compact {
            let (row, r) = rest.take_top(34.0);
            rest = r;
            self.memory_row(f, row, !wide);
        }
        if mode == CalcMode::Scientific {
            let (row, r) = rest.take_top(36.0);
            rest = r;
            let (t, row) = row.take_left(130.0);
            f.button(
                id("trig-btn"),
                t.inset_xy(0.0, 3.0),
                "Trigonometry ▾",
                SMALL,
                msg(Msg::Popup(Some(Popup::Trig))),
                true,
                Some(self.popup == Some(Popup::Trig)),
                false,
            );
            let (fu, _) = row.take_left(110.0);
            f.button(
                id("func-btn"),
                fu.inset_xy(4.0, 3.0),
                "Function ▾",
                SMALL,
                msg(Msg::Popup(Some(Popup::Functions))),
                true,
                Some(self.popup == Some(Popup::Functions)),
                false,
            );
        }
        let pad = rest.inset_xy(0.0, 4.0);
        if mode == CalcMode::Programmer && self.bit_view {
            self.bit_flip(f, pad);
        } else {
            let layout = match mode {
                CalcMode::Standard => keys::standard(),
                CalcMode::Scientific => keys::scientific(),
                CalcMode::Programmer => keys::programmer(),
            };
            let gap = if compact { 3.0 } else { 4.0 };
            self.keypad(f, pad, &layout, gap, "keypad");
        }
        if let Some(p) = panel {
            self.side_panel(f, p.inset_xy(0.0, 4.0).take_right(PANEL_W - 8.0).0);
        }
    }

    fn display(&mut self, f: &mut Frame, r: Rect, compact: bool) {
        let t = f.t;
        let r = r.inset_xy(8.0, 0.0);
        let (expr, value) = r.take_top(if compact { 24.0 } else { 30.0 });
        let e = self.vm.expression();
        f.label_fit(expr, &e, SMALL, 9.0, t.fg_dim, Align::End);
        let max = if compact { 40.0 } else { 52.0 };
        let shown = self.vm.display_value();
        f.label_fit(
            value,
            &shown,
            Style::new(max, 400.0).tabular(),
            16.0,
            t.fg,
            Align::End,
        );
        // Right-click menu (copy / paste). The display always reports what it
        // shows (AccessKit names a label from its value).
        let did = id("display");
        f.hit(did, r, crate::ui::Sense::Click, None, false);
        let current = format!("Display is {shown}");
        if let Some(n) = f.node(did, accesskit::Role::Label, &current, r) {
            n.value = Some(current.clone());
        }
        // What upstream's view model tells a screen reader, as it does
        // (calcvm::Announcement, each `Announcement = …` of
        // StandardCalculatorViewModel): the display each time the value
        // shown changes, typed digits too, and after a binary operator
        // ("Display is 7 plus"); the open-parenthesis count each time it
        // changes, in Scientific and Programmer; a ")" with none open.
        // Each is a live node there for one frame only, under a name no
        // earlier frame's had: AccessKit announces a live node as it comes,
        // so the same words can be said again. One a frame, with another
        // frame asked for while more wait: AccessKit reports the nodes one
        // update adds in no particular order (it keeps them in a hash set),
        // and what one step says has an order (a paste's "Open parenthesis
        // count 1", "Display is 1", "Open parenthesis count 0").
        let said = self.vm.take_announcements();
        self.say.extend(said.iter().map(keys::announcement));
        if f.nodes.is_none() {
            // Nobody is listening.
            self.say.clear();
        }
        if let Some(text) = self.say.pop_front() {
            self.told = self.told.wrapping_add(1);
            if let Some(n) = f.node(
                id(("announcer", self.told)),
                accesskit::Role::Status,
                &text,
                r,
            ) {
                n.value = Some(text.clone());
                n.live = true;
            }
            f.again |= !self.say.is_empty();
        }
        if !e.is_empty()
            && let Some(n) = f.node(
                id("expression"),
                accesskit::Role::Label,
                &format!("Expression is {e}"),
                expr,
            )
        {
            n.live = false;
        }
    }

    fn key_label(&self, k: &Key) -> (String, String) {
        let mode = self.vm.mode();
        let Some(b) = B::from_id(k.id) else {
            return (k.label.clone(), k.accessible_name());
        };
        if mode == CalcMode::Scientific {
            if let Some(fl) = keys::SECOND_FLIPS.iter().find(|f| f.normal == b) {
                return (fl.label(self.second).into(), fl.tip(self.second).into());
            }
            if let Some(t) = keys::TRIG.iter().find(|t| t.0 == b) {
                return (
                    keys::trig_label(t.4, self.trig_inv, self.hyp),
                    keys::trig_tip(t.4, self.trig_inv, self.hyp),
                );
            }
        }
        if mode == CalcMode::Programmer && (b == B::Lsh || b == B::Rsh) {
            let shift = self.vm.shift_mode();
            let ((_, l), (_, r)) = keys::shift_keys(shift);
            let (lt, rt) = keys::shift_tips(shift);
            return if b == B::Lsh {
                (l.into(), lt.into())
            } else {
                (r.into(), rt.into())
            };
        }
        if b == B::Clear && mode != CalcMode::Standard && self.vm.shows_clear_entry() {
            return ("CE".into(), "Clear entry (Delete)".into());
        }
        (k.label.clone(), k.accessible_name())
    }

    fn keypad(&mut self, f: &mut Frame, r: Rect, layout: &keys::Layout, gap: f32, group: &str) {
        let rows = layout.iter().map(|k| k.1 + 1).max().unwrap_or(1) as usize;
        let cols = layout.iter().map(|k| k.2 + 1).max().unwrap_or(1) as usize;
        f.group(id(group), accesskit::Role::Group, "Keypad", r);
        for (k, row, col) in layout {
            let cell = r.cell(rows, cols, *row as usize, *col as usize, gap);
            let (label, name) = self.key_label(k);
            let look = match k.kind {
                KeyKind::Number => KeyLook::Number,
                KeyKind::Operator => KeyLook::Operator,
                KeyKind::Function => KeyLook::Function,
                KeyKind::Equals => KeyLook::Equals,
                KeyKind::Toggle => KeyLook::Toggle(match k.id {
                    KEY_SECOND => self.second,
                    KEY_TRIG_SECOND => self.trig_inv,
                    KEY_HYP => self.hyp,
                    _ => false,
                }),
            };
            let enabled = B::from_id(k.id).is_none_or(|b| {
                let mut b = b;
                if self.vm.mode() == CalcMode::Scientific {
                    b = keys::resolve_second(b, self.second);
                }
                self.vm.is_enabled(b)
            });
            let kid = id((group, k.id));
            f.key(
                kid,
                cell,
                &label,
                k.icon,
                look,
                &name,
                msg(Msg::Key(k.id)),
                enabled,
            );
            // Every key here (the flyouts' too) but the 2nd and hyp
            // toggles, upstream's ToggleButtons, which Enter toggles.
            if k.kind != KeyKind::Toggle {
                f.calculator_key(kid);
            }
            // Upstream's Scientific 2nd alone isn't focused by a click
            // (AllowFocusOnInteraction="False"): Enter after it is still
            // the focused control's, "=" after clicking a key. The
            // flyout's 2nd and hyp are, as every other control.
            if k.id == KEY_SECOND {
                f.no_focus_on_click(kid);
            }
        }
        f.end_group();
    }

    fn memory_row(&mut self, f: &mut Frame, r: Rect, toggle: bool) {
        let has = !self.vm.memory().is_empty();
        let items = MEMORY;
        let n = items.len() + usize::from(toggle);
        let cells = r.columns(n, 2.0);
        for (i, (b, label, name, tip)) in items.iter().enumerate() {
            let enabled = self.vm.is_enabled(*b);
            let mid = id(("mem", *b as u32));
            f.button(
                mid,
                cells[i],
                label,
                CAPTION,
                msg(Msg::Key(b.id())),
                enabled,
                None,
                false,
            );
            f.calculator_key(mid);
            describe(f, name, tip);
        }
        if toggle {
            f.button(
                id("mem-toggle"),
                cells[n - 1],
                "M▾",
                CAPTION,
                msg(Msg::OpenPanel(Tab::Memory)),
                has,
                None,
                false,
            );
            describe(f, "Memory", "Memory");
        }
    }

    fn radix_list(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        let cur = self.vm.radix();
        for (i, radix) in Radix::ALL.into_iter().enumerate() {
            let row = Rect::new(r.x, r.y + i as f32 * 26.0, r.w, 26.0);
            let rid = id(("radix", radix.label()));
            let value = self.vm.radix_value(radix);
            f.row(
                rid,
                row,
                msg(Msg::Radix(radix)),
                radix == cur,
                &format!("{} {value}", radix.label()),
            );
            let (name, val) = row.inset_xy(8.0, 0.0).take_left(52.0);
            f.label(
                name,
                radix.label(),
                STRONG,
                if radix == cur {
                    t.accent_text
                } else {
                    t.fg_dim
                },
                Align::Start,
            );
            f.label_fit(
                val,
                &value,
                Style::new(14.0, 400.0).tabular(),
                9.0,
                t.fg,
                Align::Start,
            );
        }
    }

    fn programmer_bar(&mut self, f: &mut Frame, r: Rect) {
        let r = r.inset_xy(0.0, 2.0);
        let (full, rest) = r.take_left(36.0);
        f.icon_button(
            id("full-keypad"),
            full,
            appcore::icons::KEYBOARD,
            "Full keypad",
            msg(Msg::BitView(false)),
            true,
            Some(!self.bit_view),
        );
        let (bits, rest) = rest.take_left(36.0);
        f.icon_button(
            id("bit-keypad"),
            bits,
            appcore::icons::BITS,
            "Bit toggling keypad",
            msg(Msg::BitView(true)),
            true,
            Some(self.bit_view),
        );
        let word = match self.vm.word_size() {
            WordSize::Qword => "QWORD",
            WordSize::Dword => "DWORD",
            WordSize::Word => "WORD",
            WordSize::Byte => "BYTE",
        };
        let (w, rest) = rest.take_left(76.0);
        f.button(
            id("word"),
            w.inset_xy(4.0, 0.0),
            word,
            CAPTION,
            msg(Msg::Word),
            true,
            None,
            false,
        );
        describe(
            f,
            appcore::keys::word_size_button_name(self.vm.word_size()),
            "Word size (F2/F3/F4/F12)",
        );
        let (bw, rest) = rest.take_left(86.0);
        f.button(
            id("bitwise-btn"),
            bw.inset_xy(2.0, 0.0),
            "Bitwise ▾",
            CAPTION,
            msg(Msg::Popup(Some(Popup::Bitwise))),
            true,
            Some(self.popup == Some(Popup::Bitwise)),
            false,
        );
        let (sh, rest) = rest.take_left(86.0);
        f.button(
            id("shift-btn"),
            sh.inset_xy(2.0, 0.0),
            "Bit shift ▾",
            CAPTION,
            msg(Msg::Popup(Some(Popup::Shift))),
            true,
            Some(self.popup == Some(Popup::Shift)),
            false,
        );
        let (mt, rest) = rest.take_right(40.0);
        f.button(
            id("mem-toggle"),
            mt,
            "M▾",
            CAPTION,
            msg(Msg::OpenPanel(Tab::Memory)),
            !self.vm.memory().is_empty(),
            None,
            false,
        );
        describe(f, "Memory", "Memory");
        let (ms, _) = rest.take_right(40.0);
        let mid = id(("mem", B::Memory as u32));
        f.button(
            mid,
            ms,
            "MS",
            CAPTION,
            msg(Msg::Key(B::Memory.id())),
            self.vm.is_enabled(B::Memory),
            None,
            false,
        );
        f.calculator_key(mid);
        describe(f, "Memory store", "Memory store (Ctrl+M)");
    }

    fn bit_flip(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        let width = self.vm.word_size().bits();
        let r = r.inset_xy(0.0, 8.0);
        f.group(id("bits"), accesskit::Role::Group, "Bit toggling keypad", r);
        let rows = r.h / 4.0;
        for row in 0..4u32 {
            let rr = Rect::new(r.x, r.y + row as f32 * rows, r.w, rows);
            for nib in 0..4u32 {
                let nr = rr.cell(1, 4, 0, nib as usize, 10.0);
                let (idx_r, bits_r) = nr.take_bottom(16.0);
                let top = 63 - (row * 16 + nib * 4);
                f.label(idx_r, &top.to_string(), CAPTION, t.fg_faint, Align::Start);
                for j in 0..4u32 {
                    let bit = top - j;
                    let cell = bits_r.cell(1, 4, 0, j as usize, 2.0).inset_xy(0.0, 4.0);
                    let on = bit < width && self.vm.bit(bit);
                    let bid = id(("bit", bit));
                    f.button(
                        bid,
                        cell,
                        if on { "1" } else { "0" },
                        Style::new(16.0, if on { 700.0 } else { 400.0 }),
                        msg(Msg::FlipBit(bit)),
                        bit < width,
                        Some(on),
                        false,
                    );
                    f.calculator_key(bid);
                    if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
                        n.label = format!("Bit {bit}");
                    }
                }
            }
        }
        f.end_group();
    }

    fn side_panel(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        f.cv.rounded(r, 12.0, t.surface2);
        let r = r.inset(8.0);
        let programmer = self.vm.mode() == CalcMode::Programmer;
        let (tabs, body) = r.take_top(34.0);
        let (h, m) = tabs.take_left(tabs.w / 2.0);
        if !programmer {
            f.button(
                id("tab-history"),
                h,
                "History",
                STRONG,
                msg(Msg::Tab(Tab::History)),
                true,
                Some(self.tab == Tab::History),
                false,
            );
        }
        f.button(
            id("tab-memory"),
            if programmer { tabs } else { m },
            "Memory",
            STRONG,
            msg(Msg::Tab(Tab::Memory)),
            true,
            Some(self.tab == Tab::Memory || programmer),
            false,
        );
        let tab = if programmer { Tab::Memory } else { self.tab };
        self.panel_body(f, body, tab);
    }

    fn panel_body(&mut self, f: &mut Frame, r: Rect, tab: Tab) {
        let t = f.t;
        let (bar, list) = r.take_bottom(40.0);
        let sid = id(("panel-scroll", tab == Tab::History));
        match tab {
            Tab::History => {
                let items = self.vm.history();
                if items.is_empty() {
                    f.label(
                        list,
                        "There's no history yet",
                        SMALL,
                        t.fg_dim,
                        Align::Center,
                    );
                } else {
                    let off = f.scroll_begin(sid, list, "History");
                    let mut y = list.y - off;
                    for (i, h) in items.iter().enumerate() {
                        let row = Rect::new(list.x, y, list.w, 64.0);
                        // Read as upstream's (HistoryList's
                        // GetHistoryItemAutomationName) and GMNB's: the
                        // expression, which ends in "=", then the result.
                        f.row(
                            id(("hist", i)),
                            row,
                            msg(Msg::HistoryRecall(i)),
                            false,
                            &format!("{} {}", h.expression, h.result),
                        );
                        let inner = row.inset_xy(10.0, 6.0);
                        let (e, v) = inner.take_top(20.0);
                        f.label_fit(
                            e.take_left(e.w - 24.0).0,
                            &h.expression,
                            SMALL,
                            9.0,
                            t.fg_dim,
                            Align::End,
                        );
                        f.label_fit(
                            v,
                            &h.result,
                            Style::new(22.0, 500.0).tabular(),
                            10.0,
                            t.fg,
                            Align::End,
                        );
                        let del = Rect::new(row.right() - 26.0, row.y + 4.0, 22.0, 22.0);
                        f.icon_button(
                            id(("hist-del", i)),
                            del,
                            appcore::icons::CLOSE,
                            "Delete",
                            msg(Msg::HistoryRemove(i)),
                            true,
                            None,
                        );
                        y += 66.0;
                    }
                    f.scroll_end(sid, list, items.len() as f32 * 66.0);
                }
                if !items.is_empty() {
                    f.icon_button(
                        id("hist-clear"),
                        bar.take_right(36.0).0.inset(2.0),
                        appcore::icons::TRASH,
                        "Clear all history (Ctrl+Shift+D)",
                        msg(Msg::HistoryClear),
                        true,
                        None,
                    );
                }
            }
            Tab::Memory => {
                let items = self.vm.memory();
                if items.is_empty() {
                    f.label(
                        list,
                        "There's nothing saved in memory",
                        SMALL,
                        t.fg_dim,
                        Align::Center,
                    );
                } else {
                    let off = f.scroll_begin(sid, list, "Memory");
                    let mut y = list.y - off;
                    for (i, m) in items.iter().enumerate() {
                        let row = Rect::new(list.x, y, list.w, 70.0);
                        f.row(
                            id(("memrow", i)),
                            row,
                            msg(Msg::Memory(MemOp::Recall, i)),
                            false,
                            &format!("Memory item {m}"),
                        );
                        let (v, ops) = row.inset_xy(10.0, 6.0).take_top(32.0);
                        f.label_fit(
                            v,
                            m,
                            Style::new(22.0, 500.0).tabular(),
                            10.0,
                            t.fg,
                            Align::End,
                        );
                        let (ops, _) = ops.take_right(126.0);
                        for (j, (op, label, name)) in [
                            (MemOp::Clear, "MC", "Clear memory item"),
                            (MemOp::Add, "M+", "Add to memory item"),
                            (MemOp::Subtract, "M−", "Subtract from memory item"),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let c = ops.cell(1, 3, 0, j, 4.0);
                            f.button(
                                id(("memop", i, j)),
                                c,
                                label,
                                CAPTION,
                                msg(Msg::Memory(op, i)),
                                true,
                                None,
                                true,
                            );
                            if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
                                n.label = name.into();
                            }
                        }
                        y += 72.0;
                    }
                    f.scroll_end(sid, list, items.len() as f32 * 72.0);
                    f.icon_button(
                        id("mem-clear"),
                        bar.take_right(36.0).0.inset(2.0),
                        appcore::icons::TRASH,
                        "Clear all memory (Ctrl+L)",
                        msg(Msg::MemoryClearAll),
                        true,
                        None,
                    );
                }
            }
        }
    }

    /// Flyouts and sheets, drawn above everything else.
    pub fn overlay(&mut self, f: &mut Frame, area: Rect) {
        let Some(popup) = self.popup else { return };
        let close = msg(Msg::Popup(None));
        match popup {
            Popup::Trig | Popup::Functions | Popup::Bitwise => {
                let (layout, w, h, name) = match popup {
                    Popup::Trig => (keys::trig(), 320.0, 104.0, "trig"),
                    Popup::Functions => (keys::functions(), 260.0, 104.0, "functions"),
                    _ => (keys::bitwise(), 260.0, 104.0, "bitwise"),
                };
                let anchor = f
                    .hits
                    .iter()
                    .find(|h| {
                        h.id == id(match popup {
                            Popup::Trig => "trig-btn",
                            Popup::Functions => "func-btn",
                            _ => "bitwise-btn",
                        })
                    })
                    .map(|h| h.rect)
                    .unwrap_or(area);
                f.scrim(close, false);
                let x = anchor.x.min(area.right() - w - 8.0).max(area.x + 8.0);
                let r = Rect::new(x, anchor.bottom() + 4.0, w, h);
                f.card(r, 12.0);
                self.keypad(f, r.inset(8.0), &layout, 4.0, name);
            }
            Popup::Shift => {
                let anchor = f
                    .hits
                    .iter()
                    .find(|h| h.id == id("shift-btn"))
                    .map(|h| h.rect)
                    .unwrap_or(area);
                f.scrim(close, false);
                let w = 270.0;
                let x = anchor.right().min(area.right() - 8.0) - w;
                let r = Rect::new(
                    x.max(area.x + 8.0),
                    anchor.bottom() + 4.0,
                    w,
                    4.0 * 36.0 + 12.0,
                );
                f.card(r, 12.0);
                let cur = self.vm.shift_mode();
                for (i, (mode, label)) in SHIFT_MODES.into_iter().enumerate() {
                    let row = Rect::new(r.x + 6.0, r.y + 6.0 + i as f32 * 36.0, r.w - 12.0, 36.0);
                    f.row(
                        id(("shift", i)),
                        row,
                        msg(Msg::Shift(mode)),
                        mode == cur,
                        label,
                    );
                    let t = f.t;
                    f.label(
                        row.inset_xy(12.0, 0.0),
                        label,
                        SMALL,
                        if mode == cur { t.accent_text } else { t.fg },
                        Align::Start,
                    );
                }
            }
            Popup::Panel => {
                f.scrim(close, true);
                let h = (area.h * 0.62).max(260.0).min(area.h - 16.0);
                let r = Rect::new(area.x + 6.0, area.bottom() - h - 6.0, area.w - 12.0, h);
                f.card(r, 14.0);
                let programmer = self.vm.mode() == CalcMode::Programmer;
                let inner = r.inset(8.0);
                let (tabs, body) = inner.take_top(34.0);
                let (hh, mm) = tabs.take_left(tabs.w / 2.0);
                if !programmer {
                    f.button(
                        id("tab-history"),
                        hh,
                        "History",
                        STRONG,
                        msg(Msg::Tab(Tab::History)),
                        true,
                        Some(self.tab == Tab::History),
                        false,
                    );
                }
                f.button(
                    id("tab-memory"),
                    if programmer { tabs } else { mm },
                    "Memory",
                    STRONG,
                    msg(Msg::Tab(Tab::Memory)),
                    true,
                    Some(self.tab == Tab::Memory || programmer),
                    false,
                );
                let tab = if programmer { Tab::Memory } else { self.tab };
                self.panel_body(f, body, tab);
            }
            Popup::DisplayMenu(x, y) => {
                f.scrim(close, false);
                let r = Rect::new(
                    x.min(area.right() - 140.0),
                    y.min(area.bottom() - 84.0),
                    132.0,
                    80.0,
                );
                f.card(r, 10.0);
                let t = f.t;
                for (i, (label, m)) in [("Copy", Msg::Copy), ("Paste", Msg::Paste)]
                    .into_iter()
                    .enumerate()
                {
                    let row = Rect::new(r.x + 4.0, r.y + 4.0 + i as f32 * 36.0, r.w - 8.0, 36.0);
                    f.row(id(("dmenu", i)), row, msg(m), false, label);
                    f.label(row.inset_xy(12.0, 0.0), label, ui::BODY, t.fg, Align::Start);
                }
            }
        }
    }

    /// Header buttons: keep on top (Standard) and the history sheet.
    pub fn wants_history_button(&self, wide: bool) -> bool {
        !wide && self.vm.mode() != CalcMode::Programmer
    }
}

/// The memory buttons: shown, named (as upstream's: ClearMemoryButton
/// "Clear all memory", ...) and their tooltip (GMNB's description).
const MEMORY: &[(B, &str, &str, &str)] = &[
    (
        B::MemoryClear,
        "MC",
        "Clear all memory",
        "Clear all memory (Ctrl+L)",
    ),
    (
        B::MemoryRecall,
        "MR",
        "Memory recall",
        "Memory recall (Ctrl+R)",
    ),
    (B::MemoryAdd, "M+", "Memory add", "Memory add (Ctrl+P)"),
    (
        B::MemorySubtract,
        "M−",
        "Memory subtract",
        "Memory subtract (Ctrl+Q)",
    ),
    (B::Memory, "MS", "Memory store", "Memory store (Ctrl+M)"),
];

/// Names the node just made `name`, described by `tip` (what GMNB's
/// tooltip is read as).
fn describe(f: &mut Frame, name: &str, tip: &str) {
    if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
        n.label = name.to_string();
        n.description = Some((tip.to_string(), None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_cx<R>(wide: bool, run: impl FnOnce(&mut Cx) -> R) -> R {
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide,
            focus: &mut focus,
        };
        run(&mut cx)
    }

    /// R19-L-01: each opener of the narrow window's sheet shows the list
    /// it names, whatever list was shown before: M▾ Memory, the header's
    /// History button and Ctrl+H History. (M▾ opened History the first
    /// time, and the History button Memory after Memory had been chosen.)
    /// The same opener again closes the sheet; Ctrl+H over Memory shows
    /// History, as upstream's key shows the History flyout unless it is
    /// the one open. Docked, Ctrl+H chooses History. Programmer, which has
    /// no History, has no Ctrl+H.
    #[test]
    fn each_opener_of_the_sheet_shows_the_list_it_names() {
        let ctrl_h = input::parse_chord("ctrl+h").unwrap();
        let state = |p: &CalcPage| (p.popup, p.tab);
        let open = Some(Popup::Panel);
        let mut p = CalcPage::new(None);
        assert_eq!(state(&p), (None, Tab::History));
        // First use.
        with_cx(false, |cx| p.update(Msg::OpenPanel(Tab::Memory), cx));
        assert_eq!(state(&p), (open, Tab::Memory));
        with_cx(false, |cx| p.update(Msg::OpenPanel(Tab::Memory), cx));
        assert_eq!(state(&p), (None, Tab::Memory), "the same opener closes");
        with_cx(false, |cx| p.update(Msg::OpenPanel(Tab::History), cx));
        assert_eq!(state(&p), (open, Tab::History));
        // The other list chosen by hand, the sheet closed (Escape, a
        // click outside): each opener still shows its own.
        for (hand, opener) in [(Tab::Memory, Tab::History), (Tab::History, Tab::Memory)] {
            with_cx(false, |cx| {
                p.update(Msg::OpenPanel(opener), cx);
                if p.popup.is_none() {
                    p.update(Msg::OpenPanel(opener), cx);
                }
                p.update(Msg::Tab(hand), cx);
                p.update(Msg::Popup(None), cx);
            });
            assert_eq!(state(&p), (None, hand));
            with_cx(false, |cx| p.update(Msg::OpenPanel(opener), cx));
            assert_eq!(state(&p), (open, opener), "after {hand:?} by hand");
            with_cx(false, |cx| p.update(Msg::Popup(None), cx));
        }
        // Ctrl+H: opens on History whatever was shown last; closes
        // History's sheet; over Memory shows History.
        assert_eq!(state(&p), (None, Tab::Memory));
        assert!(with_cx(false, |cx| p.key(&ctrl_h, cx)));
        assert_eq!(state(&p), (open, Tab::History));
        assert!(with_cx(false, |cx| p.key(&ctrl_h, cx)));
        assert_eq!(state(&p), (None, Tab::History));
        with_cx(false, |cx| p.update(Msg::OpenPanel(Tab::Memory), cx));
        assert!(with_cx(false, |cx| p.key(&ctrl_h, cx)));
        assert_eq!(state(&p), (open, Tab::History), "over Memory");
        with_cx(false, |cx| p.update(Msg::Popup(None), cx));
        // Docked: no sheet, the History list chosen.
        with_cx(true, |cx| {
            p.update(Msg::Tab(Tab::Memory), cx);
            assert!(p.key(&ctrl_h, cx));
        });
        assert_eq!(state(&p), (None, Tab::History));
        // Programmer has Memory alone.
        p.set_mode(CalcMode::Programmer);
        assert!(!with_cx(false, |cx| p.key(&ctrl_h, cx)), "no Ctrl+H");
        assert_eq!(p.popup, None);
        with_cx(false, |cx| p.update(Msg::OpenPanel(Tab::Memory), cx));
        assert_eq!(state(&p), (open, Tab::Memory));
    }
}
