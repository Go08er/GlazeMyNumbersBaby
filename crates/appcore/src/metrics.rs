//! The base look's measurements: DGMNB's, named once.
//!
//! GMNB's default theme must look as DGMNB does, so every size, radius,
//! weight, gap and inset DGMNB draws with lives here, and both apps read
//! it from here (`docs/theming.md`, "Acceptance: the base look is
//! DGMNB's"). GMNB's resolved base style is pinned against these
//! constants by importing them, not by copying numbers.
//!
//! All lengths are logical pixels. Where one number serves two unrelated
//! purposes, each purpose has its own constant with the same value, so a
//! theme can change one without the other.
//!
//! The modules follow the element tables of the theming reference: text,
//! shape, focus, the window's chrome, shared controls, then the pages.

/// A text style: a size in px and a weight (100 to 900).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: f32,
}

impl TextStyle {
    pub const fn new(size: f32, weight: f32) -> TextStyle {
        TextStyle { size, weight }
    }
}

/// `text.*` and the sizes pages set in their own styles.
pub mod text {
    use super::TextStyle;

    /// `text.body`.
    pub const BODY: TextStyle = TextStyle::new(15.0, 400.0);
    /// `text.strong`.
    pub const STRONG: TextStyle = TextStyle::new(15.0, 600.0);
    /// `text.small`.
    pub const SMALL: TextStyle = TextStyle::new(13.0, 400.0);
    /// `text.caption`.
    pub const CAPTION: TextStyle = TextStyle::new(12.0, 500.0);
    /// `text.key`.
    pub const KEY: TextStyle = TextStyle::new(19.0, 400.0);
    /// `text.title`.
    pub const TITLE: TextStyle = TextStyle::new(17.0, 600.0);

    /// A number key's label is `KEY` this much larger.
    pub const NUMBER_KEY_SIZE_ADD: f32 = 2.0;
    /// A number key's label weight.
    pub const NUMBER_KEY_WEIGHT: f32 = 500.0;
    /// A paragraph's line height, times its size, rounded.
    pub const PARAGRAPH_LINE: f32 = 1.4;
    /// A button's label shrinks to fit down to this size.
    pub const BUTTON_MIN_SIZE: f32 = 10.0;

    /// The calculator display's value: largest size and weight.
    pub const DISPLAY_VALUE: TextStyle = TextStyle::new(52.0, 400.0);
    /// The compact (keep on top) display's value.
    pub const DISPLAY_VALUE_COMPACT: TextStyle = TextStyle::new(40.0, 400.0);
    /// Where the display value stops shrinking.
    pub const DISPLAY_MIN_SIZE: f32 = 16.0;
    /// Where a long expression stops shrinking.
    pub const EXPRESSION_MIN_SIZE: f32 = 9.0;
    /// The programmer radix list's value.
    pub const RADIX_VALUE: TextStyle = TextStyle::new(14.0, 400.0);
    /// Where a radix value stops shrinking.
    pub const RADIX_VALUE_MIN_SIZE: f32 = 9.0;
    /// A programmer bit button.
    pub const BIT: TextStyle = TextStyle::new(16.0, 400.0);
    /// A programmer bit button that is set.
    pub const BIT_ON: TextStyle = TextStyle::new(16.0, 700.0);
    /// A history result and a memory value.
    pub const PANEL_RESULT: TextStyle = TextStyle::new(22.0, 500.0);
    /// Where a history result stops shrinking.
    pub const PANEL_RESULT_MIN_SIZE: f32 = 10.0;
    /// Where a history expression stops shrinking.
    pub const PANEL_EXPRESSION_MIN_SIZE: f32 = 9.0;
    /// The date page's results.
    pub const DATE_RESULT: TextStyle = TextStyle::new(28.0, 400.0);
    /// Where a date result stops shrinking.
    pub const DATE_RESULT_MIN_SIZE: f32 = 14.0;
    /// The converter's values; `CONVERTER_VALUE_CHECKED_WEIGHT` while
    /// the field is the one typed into.
    pub const CONVERTER_VALUE: TextStyle = TextStyle::new(38.0, 300.0);
    pub const CONVERTER_VALUE_CHECKED_WEIGHT: f32 = 500.0;
    /// Where a converter value stops shrinking.
    pub const CONVERTER_VALUE_MIN_SIZE: f32 = 14.0;
    /// The converter's currency symbol.
    pub const CONVERTER_SYMBOL: TextStyle = TextStyle::new(26.0, 400.0);
    /// The currency status line's shrink floor.
    pub const STATUS_MIN_SIZE: f32 = 9.0;
    /// A unit list abbreviation's shrink floor.
    pub const UNIT_ABBR_MIN_SIZE: f32 = 9.0;
    /// A converter unit list row's shrink floor.
    pub const UNIT_ROW_MIN_SIZE: f32 = 10.0;
    /// An equation's error line's shrink floor.
    pub const GRAPH_ERROR_MIN_SIZE: f32 = 9.0;
    /// A graph variable's name.
    pub const GRAPH_VARIABLE: TextStyle = TextStyle::new(15.0, 600.0);
    /// The function analysis title.
    pub const ANALYSIS_TITLE: TextStyle = TextStyle::new(18.0, 600.0);
    /// Where the analysis title stops shrinking.
    pub const ANALYSIS_TITLE_MIN_SIZE: f32 = 10.0;
    /// The graph's axis labels.
    pub const CANVAS_LABEL: TextStyle = TextStyle::new(11.0, 400.0);
    /// Settings' About card's app name.
    pub const ABOUT_TITLE: TextStyle = TextStyle::new(22.0, 700.0);
}

/// `shape.*` and the other corner radii, and the icons' stroke.
pub mod shape {
    /// `shape.radius`: keys, buttons, date and unit buttons.
    pub const RADIUS: f32 = 8.0;
    /// `shape.row_radius`: list rows.
    pub const ROW_RADIUS: f32 = 6.0;
    /// A text field's radius (same value as the rows').
    pub const FIELD_RADIUS: f32 = 6.0;
    /// `shape.card_radius`: panels, cards, popovers, the graph.
    pub const CARD_RADIUS: f32 = 12.0;
    /// A popup card's radius (same value as `CARD_RADIUS`).
    pub const POPUP_RADIUS: f32 = 12.0;
    /// `menu` radius: the display's right-click menu.
    pub const MENU_RADIUS: f32 = 10.0;
    /// `panel.sheet` radius: the narrow window's bottom sheet.
    pub const SHEET_RADIUS: f32 = 14.0;
    /// `converter.chip` radius.
    pub const CHIP_RADIUS: f32 = 14.0;
    /// `toast` radius.
    pub const TOAST_RADIUS: f32 = 19.0;
    /// `graph.canvas.trace.bubble` radius.
    pub const BUBBLE_RADIUS: f32 = 8.0;
    /// `graph.analysis.card` radius.
    pub const ANALYSIS_CARD_RADIUS: f32 = 8.0;
    /// `graph.toolbar` radius.
    pub const TOOLBAR_RADIUS: f32 = 10.0;
    /// `icons.stroke`: line width of an icon on its 24-unit grid.
    pub const ICON_STROKE: f32 = 1.8;
    /// The grid an icon is drawn on.
    pub const ICON_GRID: f32 = 24.0;
    /// A border's width.
    pub const BORDER: f32 = 1.0;
    /// A focused or erroneous text field's border width.
    pub const FIELD_BORDER_ACTIVE: f32 = 2.0;
    /// `popover` shadow: three rings, as (spread, black alpha).
    pub const POPOVER_SHADOW: [(f32, f32); 3] = [(6.0, 0.05), (3.0, 0.07), (1.0, 0.10)];
    /// `scrim`: black at this alpha.
    pub const SCRIM_ALPHA: f32 = 0.30;
}

/// `focus`: the ring drawn for keyboard focus.
pub mod focus {
    /// The ring's line width.
    pub const WIDTH: f32 = 2.0;
    /// The ring's distance from the control.
    pub const OFFSET: f32 = 2.0;
    /// The ring's corner radius is the control's plus this.
    pub const RADIUS_GROW: f32 = 2.0;
    /// A scroll view's ring: its distance inside the view...
    pub const SCROLL_INSET: f32 = 3.0;
    /// ...and its corner radius.
    pub const SCROLL_RADIUS: f32 = 6.0;
    /// A slider's ring: the box round the knob...
    pub const SLIDER_BOX: f32 = 18.0;
    /// ...and its corner radius.
    pub const SLIDER_RADIUS: f32 = 9.0;
}

/// Window chrome: header, resize edge, navigation, toast.
pub mod window {
    /// A new window's size.
    pub const DEFAULT_WIDTH: i32 = 360;
    pub const DEFAULT_HEIGHT: i32 = 600;
    /// The smallest window.
    pub const MIN_SIZE: (f64, f64) = (300.0, 400.0);
    /// `header.height`.
    pub const HEADER_H: f32 = 46.0;
    /// The header's padding, left and right...
    pub const HEADER_PAD_X: f32 = 6.0;
    /// ...and top and bottom.
    pub const HEADER_PAD_Y: f32 = 5.0;
    /// The grab area at the window's edges for resizing.
    pub const RESIZE_EDGE: f32 = 6.0;
    /// The space after a button on the left.
    pub const HEADER_BUTTON_STEP: f32 = 38.0;
    /// The space between buttons on the right.
    pub const HEADER_BUTTON_GAP: f32 = 2.0;
    /// The space between the window controls and the page's buttons.
    pub const HEADER_CONTROLS_GAP: f32 = 8.0;
    /// A page title's gap from the button before it.
    pub const HEADER_TITLE_GAP: f32 = 6.0;
    /// The title's room, for a page's name...
    pub const HEADER_TITLE_W: f32 = 220.0;
    /// ...and for Settings'.
    pub const HEADER_SETTINGS_TITLE_W: f32 = 200.0;
    /// A scroll view moves this far for an arrow key.
    pub const SCROLL_LINE: f32 = 40.0;
    /// A mouse wheel notch.
    pub const WHEEL_LINE: f32 = 48.0;
}

/// The navigation drawer.
pub mod nav {
    /// `nav.width`.
    pub const WIDTH: f32 = 280.0;
    /// The strip of the window left uncovered beside the drawer.
    pub const MIN_UNCOVERED: f32 = 40.0;
    /// The line down the drawer's right edge.
    pub const EDGE: f32 = 1.0;
    /// The menu button in the drawer's header, and the title after it.
    pub const TITLE_X: f32 = 48.0;
    pub const TITLE_W: f32 = 200.0;
    /// The settings row's room at the bottom.
    pub const FOOT_H: f32 = 52.0;
    /// The settings row's inset.
    pub const FOOT_INSET: f32 = 6.0;
    /// The list's top padding.
    pub const LIST_PAD: f32 = 4.0;
    /// A group header's x offset and height.
    pub const HEADER_X: f32 = 18.0;
    pub const HEADER_W: f32 = 200.0;
    pub const HEADER_H: f32 = 28.0;
    /// `nav.row`: x margin, height and the step to the next.
    pub const ROW_MARGIN_X: f32 = 6.0;
    pub const ROW_H: f32 = 40.0;
    pub const ROW_STEP: f32 = 42.0;
    /// `nav.row` icon size, its box and offset...
    pub const ROW_ICON: f32 = 18.0;
    pub const ROW_ICON_X: f32 = 8.0;
    pub const ROW_ICON_BOX: f32 = 28.0;
    /// ...and the label's offset and the room it leaves on the right.
    pub const ROW_LABEL_X: f32 = 44.0;
    pub const ROW_LABEL_TRIM: f32 = 50.0;
}

/// Toast.
pub mod toast {
    pub const MIN_HEIGHT: f32 = 38.0;
    /// Padding, left and right.
    pub const PAD_X: f32 = 16.0;
    /// The distance from the window's bottom edge to the toast's top.
    pub const BOTTOM: f32 = 64.0;
    /// `fg` at this alpha.
    pub const ALPHA: f32 = 0.92;
}

/// Shared controls.
pub mod controls {
    /// `icon_button`.
    pub const ICON_BUTTON: f32 = 36.0;
    /// `icon_button` icon size.
    pub const ICON_BUTTON_ICON: f32 = 18.0;
    /// An icon button that is on: the accent's alpha behind it.
    pub const ICON_BUTTON_ON_ALPHA: f32 = 0.18;
    /// `text_button` padding, left and right.
    pub const BUTTON_PAD_X: f32 = 6.0;
    /// `text_button` height.
    pub const TEXT_BUTTON_H: f32 = 34.0;
    /// `text_button_gap`.
    pub const TEXT_BUTTON_GAP: f32 = 2.0;
    /// `segmented.gap`.
    pub const SEGMENTED_GAP: f32 = 4.0;
    /// `segmented.item` height.
    pub const SEGMENTED_H: f32 = 36.0;
    /// `row` height (menus, dropdown lists).
    pub const ROW_H: f32 = 36.0;
    /// A selected row's accent alpha.
    pub const ROW_CHECKED_ALPHA: f32 = 0.16;
    /// A key or control that is disabled is faded to this alpha.
    pub const DISABLED_ALPHA: f32 = 0.35;
    /// `keys.icon` size.
    pub const KEY_ICON: f32 = 20.0;
    /// A key's label is shrunk to fit inside its width less this.
    pub const KEY_LABEL_MARGIN: f32 = 8.0;
    /// `field` padding, left and right.
    pub const FIELD_PAD_X: f32 = 10.0;
    /// `field` height in a dialog-like place (the unit search).
    pub const FIELD_H: f32 = 38.0;
    /// The selection's accent alpha.
    pub const SELECTION_ALPHA: f32 = 0.30;
    /// The text line's height, for the selection and the caret.
    pub const FIELD_LINE_H: f32 = 20.0;
    /// The caret's width.
    pub const CARET_W: f32 = 1.5;
    /// An IME preedit underline: its distance below the baseline...
    pub const PREEDIT_DROP: f32 = 3.0;
    /// ...and its thickness.
    pub const PREEDIT_H: f32 = 1.0;
    /// How far the caret is kept from the field's edge.
    pub const CARET_MARGIN: f32 = 2.0;
    /// `slider.track` height.
    pub const SLIDER_TRACK_H: f32 = 4.0;
    /// `slider.track` radius.
    pub const SLIDER_TRACK_RADIUS: f32 = 2.0;
    /// The track's inset from the slider's ends.
    pub const SLIDER_INSET: f32 = 8.0;
    /// `slider.knob` radius (size 16).
    pub const SLIDER_KNOB_RADIUS: f32 = 8.0;
    /// `slider.knob` halo radius (halo_size 24) and its accent alpha.
    pub const SLIDER_HALO_RADIUS: f32 = 12.0;
    pub const SLIDER_HALO_ALPHA: f32 = 0.16;
    /// `switch`: size and the knob's radius.
    pub const SWITCH_W: f32 = 46.0;
    pub const SWITCH_H: f32 = 24.0;
    pub const SWITCH_KNOB_RADIUS: f32 = 9.0;
    /// The knob's centre from the switch's end.
    pub const SWITCH_KNOB_INSET: f32 = 12.0;
    /// `scrollbar`: width, radius, the least height of its thumb and its
    /// distance from the view's right edge.
    pub const SCROLLBAR_W: f32 = 3.0;
    pub const SCROLLBAR_RADIUS: f32 = 1.5;
    pub const SCROLLBAR_MIN_THUMB: f32 = 24.0;
    pub const SCROLLBAR_INSET: f32 = 5.0;
    /// A popup menu's padding (the display's right-click menu)...
    pub const MENU_PAD: f32 = 4.0;
    /// ...and a list popup's (bit shift).
    pub const LIST_POPUP_PAD: f32 = 6.0;
    /// A row's label inset in a menu or list.
    pub const MENU_LABEL_X: f32 = 12.0;
}

/// The calculator page.
pub mod calc {
    /// The calculator goes wide at this width.
    pub const WIDE: f32 = 620.0;
    /// The wide window's side panel strip; the panel is drawn
    /// `PANEL_MARGIN_LEFT` narrower than it.
    pub const PANEL_W: f32 = 300.0;
    /// The space between the page and the panel.
    pub const PANEL_MARGIN_LEFT: f32 = 8.0;
    /// `layout.calculator.margin`: left and right...
    pub const MARGIN_X: f32 = 8.0;
    /// ...top and bottom.
    pub const MARGIN_Y: f32 = 4.0;
    /// The panel's top and bottom margin.
    pub const PANEL_MARGIN_Y: f32 = 4.0;
    /// `display.heights`.
    pub const DISPLAY_H_STANDARD: f32 = 116.0;
    pub const DISPLAY_H_SCIENTIFIC: f32 = 104.0;
    pub const DISPLAY_H_PROGRAMMER: f32 = 84.0;
    pub const DISPLAY_H_COMPACT: f32 = 86.0;
    /// `display.padding_x`.
    pub const DISPLAY_PAD_X: f32 = 8.0;
    /// The expression line's height, and the compact one's.
    pub const EXPRESSION_H: f32 = 30.0;
    pub const EXPRESSION_H_COMPACT: f32 = 24.0;
    /// `keys.gap`, `keys.gap_compact`.
    pub const KEY_GAP: f32 = 4.0;
    pub const KEY_GAP_COMPACT: f32 = 3.0;
    /// A popup keypad's gap.
    pub const POPUP_KEY_GAP: f32 = 4.0;
    /// The keypad's top and bottom padding.
    pub const KEYPAD_PAD_Y: f32 = 4.0;
    /// Scientific's angle row, its DEG and F-E buttons' widths.
    pub const ANGLE_ROW_H: f32 = 32.0;
    pub const ANGLE_W: f32 = 64.0;
    pub const FE_W: f32 = 52.0;
    /// Scientific's flyout row, its two buttons' widths and the
    /// buttons' top and bottom inset.
    pub const FLYOUT_ROW_H: f32 = 36.0;
    pub const TRIG_W: f32 = 130.0;
    pub const FUNCTION_W: f32 = 110.0;
    pub const FLYOUT_BUTTON_PAD_Y: f32 = 3.0;
    /// The Function button's inset, left and right.
    pub const FUNCTION_INSET_X: f32 = 4.0;
    /// A flyout's or list's distance from the page's edge.
    pub const FLYOUT_EDGE: f32 = 8.0;
    /// A flyout's size: trigonometry, function and bitwise.
    pub const TRIG_FLYOUT: (f32, f32) = (320.0, 104.0);
    pub const FUNCTION_FLYOUT: (f32, f32) = (260.0, 104.0);
    pub const BITWISE_FLYOUT: (f32, f32) = (260.0, 104.0);
    /// A flyout's distance below its button, and its keypad's inset.
    pub const FLYOUT_GAP: f32 = 4.0;
    pub const FLYOUT_INSET: f32 = 8.0;
    /// The bit shift list's width.
    pub const SHIFT_W: f32 = 270.0;
    /// The display's right-click menu: size and distance from the edge.
    pub const DISPLAY_MENU: (f32, f32) = (132.0, 80.0);
    pub const DISPLAY_MENU_EDGE: (f32, f32) = (140.0, 84.0);
    /// The bottom sheet (narrow window): its share of the height, the
    /// least height and the margin round it.
    pub const SHEET_SHARE: f32 = 0.62;
    pub const SHEET_MIN_H: f32 = 260.0;
    pub const SHEET_MARGIN: f32 = 6.0;
    pub const SHEET_VERTICAL_MARGIN: f32 = 16.0;
}

/// The history and memory panel.
pub mod panel {
    /// `panel` padding.
    pub const PADDING: f32 = 8.0;
    /// `panel.tabs.item` height.
    pub const TAB_H: f32 = 34.0;
    /// The bar under the list, holding the trash button.
    pub const BAR_H: f32 = 40.0;
    /// The trash button: its width and inset.
    pub const TRASH_W: f32 = 36.0;
    pub const TRASH_INSET: f32 = 2.0;
    /// `panel.row` height, and the step to the next row.
    pub const HISTORY_ROW_H: f32 = 64.0;
    pub const HISTORY_ROW_STEP: f32 = 66.0;
    /// A memory row's height and step.
    pub const MEMORY_ROW_H: f32 = 70.0;
    pub const MEMORY_ROW_STEP: f32 = 72.0;
    /// A row's content inset: left and right, top and bottom.
    pub const ROW_PAD_X: f32 = 10.0;
    pub const ROW_PAD_Y: f32 = 6.0;
    /// A history row's expression line height and the room kept for the
    /// delete button.
    pub const EXPRESSION_H: f32 = 20.0;
    pub const EXPRESSION_TRIM: f32 = 24.0;
    /// A memory row's value height.
    pub const MEMORY_VALUE_H: f32 = 32.0;
    /// `panel.delete`: size, and its distance from the row's right and top.
    pub const DELETE: f32 = 22.0;
    pub const DELETE_RIGHT: f32 = 26.0;
    pub const DELETE_TOP: f32 = 4.0;
    /// A memory row's three buttons: the width they share and their gap.
    pub const MEMORY_BUTTONS_W: f32 = 126.0;
    pub const MEMORY_BUTTONS_GAP: f32 = 4.0;
}

/// The programmer calculator.
pub mod programmer {
    /// `programmer.radix.row` height.
    pub const RADIX_ROW_H: f32 = 26.0;
    /// The radix list's rows (hexadecimal, decimal, octal, binary).
    pub const RADIX_ROWS: f32 = 4.0;
    /// The list's extra room below the rows.
    pub const RADIX_LIST_PAD: f32 = 4.0;
    /// `programmer.radix.row` padding, left and right.
    pub const RADIX_PAD_X: f32 = 8.0;
    /// The radix name's width.
    pub const RADIX_NAME_W: f32 = 52.0;
    /// The bar of word size, bitwise and shift buttons: its height and
    /// padding top and bottom.
    pub const BAR_H: f32 = 36.0;
    pub const BAR_PAD_Y: f32 = 2.0;
    /// The two keypad switches' width.
    pub const BAR_SWITCH_W: f32 = 36.0;
    /// Word size, bitwise and bit shift buttons' widths and insets.
    pub const WORD_W: f32 = 76.0;
    pub const WORD_INSET: f32 = 4.0;
    pub const BITWISE_W: f32 = 86.0;
    pub const SHIFT_W: f32 = 86.0;
    pub const BAR_BUTTON_INSET: f32 = 2.0;
    /// The M▾ and MS buttons' widths.
    pub const MEMORY_BUTTON_W: f32 = 40.0;
    /// The bit grid: padding top and bottom, rows, gap between
    /// nibbles, gap between bits, the index's height and a bit
    /// button's inset top and bottom.
    pub const BITS_PAD_Y: f32 = 8.0;
    pub const BITS_ROWS: f32 = 4.0;
    pub const NIBBLE_GAP: f32 = 10.0;
    pub const BIT_GAP: f32 = 2.0;
    pub const BIT_INDEX_H: f32 = 16.0;
    pub const BIT_INSET_Y: f32 = 4.0;
}

/// The date page.
pub mod date {
    /// `layout.date.margin`: left and right, top and bottom.
    pub const MARGIN_X: f32 = 16.0;
    pub const MARGIN_Y: f32 = 8.0;
    /// `layout.date.max_width`.
    pub const MAX_WIDTH: f32 = 480.0;
    /// The add/subtract switch's height and width.
    pub const OP_SWITCH_H: f32 = 34.0;
    pub const OP_SWITCH_W: f32 = 240.0;
    /// The space under the mode switch.
    pub const SWITCH_GAP: f32 = 16.0;
    /// A date's label and button block, and the button's width at most.
    pub const FIELD_BLOCK_H: f32 = 64.0;
    pub const FIELD_LABEL_H: f32 = 22.0;
    pub const FIELD_TOP: f32 = 24.0;
    /// `date.field` height, and its width at most.
    pub const FIELD_H: f32 = 38.0;
    pub const FIELD_MAX_W: f32 = 320.0;
    /// The steps down the difference page.
    pub const STEP_FROM_TO: f32 = 72.0;
    pub const STEP_TO_LABEL: f32 = 80.0;
    pub const STEP_LABEL_RESULT: f32 = 24.0;
    pub const STEP_RESULT_NEXT: f32 = 46.0;
    /// The result's row height.
    pub const RESULT_H: f32 = 44.0;
    /// The days-only line's height.
    pub const SUBRESULT_H: f32 = 24.0;
    /// The steps down the add/subtract page.
    pub const STEP_FROM_OP: f32 = 76.0;
    pub const STEP_OP_OFFSETS: f32 = 46.0;
    pub const STEP_OFFSETS_LABEL: f32 = 82.0;
    /// The year, month and day boxes: row height, gap, label height,
    /// the row's top below the label, the box's height, the width of a
    /// step button and its inset.
    pub const OFFSETS_H: f32 = 66.0;
    pub const OFFSETS_GAP: f32 = 10.0;
    pub const OFFSET_LABEL_H: f32 = 22.0;
    pub const OFFSET_LINE_TOP: f32 = 24.0;
    pub const OFFSET_LINE_H: f32 = 36.0;
    pub const OFFSET_STEP_W: f32 = 30.0;
    pub const OFFSET_STEP_INSET: f32 = 1.0;
    pub const OFFSET_FIELD_INSET: f32 = 2.0;
    /// The calendar popover: size, distance from its button, padding.
    pub const CALENDAR: (f32, f32) = (300.0, 316.0);
    pub const CALENDAR_GAP: f32 = 4.0;
    pub const CALENDAR_EDGE: f32 = 8.0;
    pub const CALENDAR_PAD: f32 = 10.0;
    /// The calendar's header height, the room its title leaves for the
    /// arrows, the weekday row and the grid's gap.
    pub const CALENDAR_HEAD_H: f32 = 36.0;
    pub const CALENDAR_ARROWS_W: f32 = 36.0;
    pub const CALENDAR_TITLE_INSET: f32 = 76.0;
    pub const CALENDAR_WEEKDAY_H: f32 = 26.0;
    pub const CALENDAR_GRID_GAP: f32 = 2.0;
    /// `date.calendar.today`'s border width.
    pub const TODAY_BORDER: f32 = 1.0;
}

/// The converter page.
pub mod converter {
    /// The converter goes wide at this width.
    pub const WIDE: f32 = 700.0;
    /// `layout.converter.margin`: left and right, top and bottom.
    pub const MARGIN_X: f32 = 12.0;
    pub const MARGIN_Y: f32 = 6.0;
    /// `layout.converter.keypad_width`.
    pub const KEYPAD_W: f32 = 300.0;
    /// The wide window's space between the values and the keypad.
    pub const WIDE_GAP: f32 = 20.0;
    /// The narrow keypad: the least height the page keeps for the
    /// values, with and without the currency lines, the keypad's share
    /// at most, and its least and greatest heights.
    pub const NEEDED_CURRENCY: f32 = 320.0;
    pub const NEEDED: f32 = 250.0;
    pub const PAD_SHARE: f32 = 0.5;
    pub const PAD_MIN_H: f32 = 168.0;
    pub const PAD_MAX_H: f32 = 330.0;
    /// The values' space above the keypad when narrow.
    pub const NARROW_GAP: f32 = 6.0;
    /// The keypad's rows, columns and `layout.converter.gap`.
    pub const KEY_ROWS: usize = 5;
    pub const KEY_COLS: usize = 3;
    pub const KEY_GAP: f32 = 4.0;
    /// A value's field height, the step past it, and its padding.
    pub const FIELD_H: f32 = 58.0;
    pub const FIELD_STEP: f32 = 60.0;
    pub const FIELD_PAD_X: f32 = 2.0;
    /// `converter.marker`: width and height.
    pub const MARKER_W: f32 = 36.0;
    pub const MARKER_H: f32 = 3.0;
    /// The symbol's gap from the value.
    pub const SYMBOL_GAP: f32 = 8.0;
    /// `converter.unit` height, the room its label leaves for the arrow
    /// and the step past it.
    pub const UNIT_H: f32 = 34.0;
    pub const UNIT_ARROW: f32 = 44.0;
    pub const UNIT_STEP: f32 = 38.0;
    /// `converter.swap` (an icon button): its distance below, and the step
    /// past it.
    pub const SWAP_TOP: f32 = 2.0;
    pub const SWAP_STEP: f32 = 42.0;
    /// The space before the currency lines or the chips.
    pub const SECTION_GAP: f32 = 6.0;
    /// The currency ratio, timestamp and status lines' heights, and the
    /// steps past them.
    pub const RATIO_H: f32 = 22.0;
    pub const TIMESTAMP_H: f32 = 26.0;
    pub const TIMESTAMP_STEP: f32 = 28.0;
    pub const STATUS_H: f32 = 20.0;
    pub const STATUS_STEP: f32 = 22.0;
    /// The "Updating…" line's gap from the timestamp and its width; the
    /// refresh button's gap and width.
    pub const UPDATING_GAP: f32 = 10.0;
    pub const UPDATING_W: f32 = 120.0;
    pub const REFRESH_GAP: f32 = 6.0;
    pub const REFRESH_W: f32 = 116.0;
    /// `converter.caption` line height and its step.
    pub const CAPTION_H: f32 = 24.0;
    pub const CAPTION_STEP: f32 = 26.0;
    /// `converter.chip`: height, padding (twice) and the gaps.
    pub const CHIP_H: f32 = 28.0;
    pub const CHIP_PAD_X: f32 = 10.0;
    pub const CHIP_GAP: f32 = 6.0;
    pub const CHIP_ROW_STEP: f32 = 32.0;
    /// The room left under the last thing.
    pub const BOTTOM_PAD: f32 = 32.0;
    /// The unit list popup: width at most, least height below it, height
    /// at most, its gap from the button and edge, and its padding.
    pub const LIST_MAX_W: f32 = 340.0;
    pub const LIST_EDGE: f32 = 8.0;
    pub const LIST_GAP: f32 = 4.0;
    pub const LIST_MIN_BELOW: f32 = 200.0;
    pub const LIST_MAX_H: f32 = 420.0;
    pub const LIST_PAD: f32 = 8.0;
    /// The search field's height and the list's padding below it.
    pub const SEARCH_H: f32 = 38.0;
    pub const LIST_INNER_PAD_Y: f32 = 4.0;
    /// A unit row's step (its height is `ROW_STEP - ROW_GAP`).
    pub const UNIT_ROW_STEP: f32 = 36.0;
    pub const UNIT_ROW_GAP: f32 = 2.0;
    /// A unit row's padding and the abbreviation's width.
    pub const UNIT_ROW_PAD_X: f32 = 10.0;
    pub const UNIT_ABBR_W: f32 = 70.0;
    /// "No matching units" line's height.
    pub const EMPTY_H: f32 = 40.0;
}

/// The graphing page.
pub mod graph {
    /// Graphing goes wide at this width.
    pub const WIDE: f32 = 760.0;
    /// `layout.graph.margin`: left and right, top and bottom.
    pub const MARGIN_X: f32 = 8.0;
    pub const MARGIN_Y: f32 = 4.0;
    /// `layout.graph.side_width`.
    pub const SIDE_W: f32 = 340.0;
    /// `layout.graph.gap`: between the side panel and the canvas.
    pub const GAP: f32 = 8.0;
    /// `layout.graph.pad_height`.
    pub const PAD_H: f32 = 214.0;
    /// The side panel's padding top and bottom.
    pub const SIDE_PAD_Y: f32 = 4.0;
    /// `graph.divider`'s width.
    pub const DIVIDER: f32 = 1.0;
    /// The keypad's room, less this, under the divider.
    pub const PAD_TRIM: f32 = 8.0;
    /// `keys.pad.gap`.
    pub const PAD_GAP: f32 = 3.0;
    /// An equation row: height, the swatch's box, its inset and top,
    /// its dot's radius and hidden alpha, the row's field top and
    /// height, the field's left, and the step past it.
    pub const ROW_H: f32 = 40.0;
    pub const SWATCH_BOX: f32 = 28.0;
    pub const SWATCH_X: f32 = 2.0;
    pub const SWATCH_Y: f32 = 6.0;
    pub const SWATCH_RADIUS: f32 = 9.0;
    pub const SWATCH_HIDDEN_ALPHA: f32 = 0.3;
    pub const SWATCH_HOVER_RADIUS: f32 = 14.0;
    pub const ROW_FIELD_X: f32 = 34.0;
    pub const ROW_FIELD_Y: f32 = 2.0;
    pub const ROW_FIELD_H: f32 = 36.0;
    pub const ROW_STEP: f32 = 42.0;
    /// The row's three buttons (`graph.row_button`): the width they
    /// share, each one's size and its top.
    pub const ROW_BUTTONS_W: f32 = 96.0;
    pub const ROW_BUTTON: f32 = 32.0;
    pub const ROW_BUTTON_Y: f32 = 2.0;
    /// The field's gap from the buttons.
    pub const ROW_FIELD_TRIM: f32 = 4.0;
    /// `graph.error`: its left, the room it leaves on the right, its
    /// height, and the step past it.
    pub const ERROR_X: f32 = 36.0;
    pub const ERROR_TRIM: f32 = 40.0;
    pub const ERROR_H: f32 = 18.0;
    pub const ERROR_UP: f32 = 2.0;
    /// `graph.add`: its top, height and step past it.
    pub const ADD_Y: f32 = 2.0;
    pub const ADD_H: f32 = 36.0;
    pub const ADD_STEP: f32 = 44.0;
    /// The variables: heading left, height and step; a row's height,
    /// name width and inset, the value box's width, its inset and the
    /// step past a row.
    pub const VARIABLES_X: f32 = 4.0;
    pub const VARIABLES_H: f32 = 26.0;
    pub const VARIABLES_STEP: f32 = 28.0;
    pub const VARIABLE_ROW_H: f32 = 36.0;
    pub const VARIABLE_NAME_W: f32 = 34.0;
    pub const VARIABLE_NAME_INSET: f32 = 6.0;
    pub const VARIABLE_VALUE_W: f32 = 84.0;
    pub const VARIABLE_FIELD_INSET: f32 = 2.0;
    pub const VARIABLE_STEP: f32 = 40.0;
    /// The analysis: header height and back button width, the title's
    /// inset, height and step, the lines' heights, the card's insets
    /// (left, top) and the steps between cards.
    pub const ANALYSIS_HEAD_H: f32 = 40.0;
    pub const ANALYSIS_BACK_W: f32 = 190.0;
    pub const ANALYSIS_TITLE_X: f32 = 6.0;
    pub const ANALYSIS_TITLE_H: f32 = 30.0;
    pub const ANALYSIS_TITLE_STEP: f32 = 36.0;
    pub const ANALYSIS_NOTE_H: f32 = 24.0;
    pub const ANALYSIS_NOTE_STEP: f32 = 28.0;
    pub const ANALYSIS_ERROR_STEP: f32 = 20.0;
    pub const ANALYSIS_CARD_X: f32 = 12.0;
    pub const ANALYSIS_CARD_TOP: f32 = 6.0;
    pub const ANALYSIS_CARD_TITLE_H: f32 = 20.0;
    pub const ANALYSIS_CARD_TITLE_STEP: f32 = 26.0;
    pub const ANALYSIS_VALUE_STEP: f32 = 22.0;
    pub const ANALYSIS_CARD_NOTE_STEP: f32 = 18.0;
    pub const ANALYSIS_CARD_BOTTOM: f32 = 8.0;
    pub const ANALYSIS_CARD_INSET: f32 = 4.0;
    pub const ANALYSIS_CARD_GAP: f32 = 6.0;
    pub const ANALYSIS_GRID_SHARE: f32 = 0.6;
    /// `graph.canvas.radius`, as the same value as `CARD_RADIUS`.
    pub const CANVAS_RADIUS: f32 = 12.0;
    /// `graph.canvas.grid_minor` and `grid_major` alphas, dark and light.
    pub const GRID_MINOR_ALPHA: (f32, f32) = (0.05, 0.06);
    pub const GRID_MAJOR_ALPHA: (f32, f32) = (0.11, 0.13);
    /// A grid line's width.
    pub const GRID_WIDTH: f32 = 1.0;
    /// `graph.canvas.axis`: alpha and width.
    pub const AXIS_ALPHA: f32 = 0.55;
    pub const AXIS_WIDTH: f32 = 1.3;
    /// `graph.canvas.fill_opacity`.
    pub const FILL_OPACITY: f32 = 0.18;
    /// `graph.canvas.trace.hair_opacity` and the hair's width.
    pub const HAIR_OPACITY: f32 = 0.45;
    pub const HAIR_WIDTH: f32 = 1.0;
    /// The trace dot's radius is the curve's plus this; the halo grows
    /// it by `TRACE_HALO_GROW` (`graph.canvas.trace.halo.grow`).
    pub const TRACE_DOT_ADD: f32 = 2.0;
    pub const TRACE_HALO_GROW: f32 = 1.5;
    /// The bubble: padding added to the text (twice), height, its
    /// distance from the point and from the canvas edge.
    pub const BUBBLE_PAD: f32 = 18.0;
    pub const BUBBLE_H: f32 = 28.0;
    pub const BUBBLE_OFFSET: f32 = 14.0;
    pub const BUBBLE_EDGE: f32 = 6.0;
    /// `graph.toolbar`: its distance from the canvas's right and top,
    /// width, padding left and top (the same below), the button's size
    /// and the step past it.
    pub const TOOLBAR_RIGHT: f32 = 46.0;
    pub const TOOLBAR_TOP: f32 = 8.0;
    pub const TOOLBAR_W: f32 = 38.0;
    pub const TOOLBAR_PAD_X: f32 = 2.0;
    pub const TOOLBAR_PAD_Y: f32 = 4.0;
    pub const TOOLBAR_BUTTON: f32 = 34.0;
    pub const TOOLBAR_STEP: f32 = 36.0;
    pub const TOOLBAR_BUTTONS: f32 = 6.0;
    pub const TOOLBAR_FILL_ALPHA: f32 = 0.94;
    /// The style popup: size, and its padding.
    pub const STYLE_POPUP: (f32, f32) = (264.0, 150.0);
    pub const STYLE_PAD: f32 = 12.0;
    /// The options popup: size.
    pub const OPTIONS_POPUP: (f32, f32) = (300.0, 300.0);
    pub const OPTIONS_PAD: f32 = 12.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six text styles of the theming reference, as DGMNB draws them.
    #[test]
    fn the_text_styles_are_the_references() {
        let all = [
            (text::BODY, 15.0, 400.0),
            (text::STRONG, 15.0, 600.0),
            (text::SMALL, 13.0, 400.0),
            (text::CAPTION, 12.0, 500.0),
            (text::KEY, 19.0, 400.0),
            (text::TITLE, 17.0, 600.0),
        ];
        for (style, size, weight) in all {
            assert_eq!(style, TextStyle::new(size, weight));
        }
    }
}
