//! Settings (upstream Settings.xaml: theme + about), plus palette and motion.

use std::rc::Rc;

use adw::prelude::*;

use std::cell::{Cell, RefCell};

use crate::pages::Ctx;
use crate::settings::Persist;
use crate::theme::{PaletteId, Scheme, complement, rgba, to_hex};
use crate::window::{Window, apply_backdrop, apply_theme_setting};

/// A palette preview: which palette, its current scheme, and its swatch.
type Tile = (PaletteId, Rc<Cell<Scheme>>, gtk::DrawingArea);

pub fn show(win: &Rc<Window>) {
    let ctx = win.ctx().clone();
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title("Settings");
    dialog.add_css_class("wc-prefs");

    let page = adw::PreferencesPage::new();
    page.set_title("Appearance");

    // Theme.
    let group = adw::PreferencesGroup::builder().title("App theme").build();
    let theme = adw::ToggleGroup::new();
    for (name, label) in [("light", "Light"), ("dark", "Dark"), ("system", "System")] {
        theme.add(adw::Toggle::builder().name(name).label(label).build());
    }
    theme.set_active_name(Some(&ctx.store.data.borrow().theme));
    theme.set_valign(gtk::Align::Center);
    let row = adw::ActionRow::builder()
        .title("Theme")
        .subtitle("Follows the system unless you choose otherwise")
        .build();
    row.add_suffix(&theme);
    group.add(&row);
    {
        let ctx = ctx.clone();
        theme.connect_active_name_notify(move |t| {
            if ctx.layers.covers(t) {
                // Covered, Settings changes nothing (R15-M-02).
                let saved = ctx.store.data.borrow().theme.clone();
                if t.active_name().as_deref() != Some(saved.as_str()) {
                    t.set_active_name(Some(&saved));
                }
                return;
            }
            let name = t
                .active_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "system".into());
            apply_theme_setting(&name);
            ctx.store.data.borrow_mut().theme = name;
            ctx.store.persist();
        });
    }
    page.add(&group);

    {
        // Palette.
        let group = adw::PreferencesGroup::builder()
            .title("Palette")
            .description("Colours for the aurora, keys and graphs")
            .build();
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .max_children_per_line(5)
            .min_children_per_line(3)
            .homogeneous(true)
            .row_spacing(10)
            .column_spacing(10)
            .css_classes(["wc-palettes"])
            .build();
        let dark = adw::StyleManager::default().is_dark();
        // Each tile draws from a cell so the generated palettes can re-preview.
        let tiles: Rc<RefCell<Vec<Tile>>> = Rc::default();
        for id in PaletteId::ALL {
            let cell = Rc::new(Cell::new(ctx.hub.scheme_for(id, dark)));
            let swatch = gtk::DrawingArea::builder()
                .content_width(96)
                .content_height(56)
                .build();
            let c2 = cell.clone();
            swatch.set_draw_func(move |_, cr, w, h| draw_swatch(cr, w as f64, h as f64, &c2.get()));
            tiles.borrow_mut().push((id, cell, swatch.clone()));
            let label = gtk::Label::new(Some(id.title()));
            label.add_css_class("caption");
            let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
            b.append(&swatch);
            b.append(&label);
            let child = gtk::FlowBoxChild::builder().child(&b).build();
            child.update_property(&[gtk::accessible::Property::Label(id.title())]);
            flow.append(&child);
            if id == ctx.hub.palette() {
                flow.select_child(&child);
            }
        }
        let refresh_tiles = {
            let (tiles, ctx) = (tiles.clone(), ctx.clone());
            move || {
                let dark = adw::StyleManager::default().is_dark();
                for (id, cell, area) in tiles.borrow().iter() {
                    if matches!(id, PaletteId::System | PaletteId::Custom) {
                        cell.set(ctx.hub.scheme_for(*id, dark));
                        area.queue_draw();
                    }
                }
            }
        };

        // Freestyle: two colour pickers (secondary defaults to the complement).
        let (p0, q0) = ctx.hub.custom();
        let color_dialog = gtk::ColorDialog::builder()
            .with_alpha(false)
            .title("Pick a colour")
            .build();
        let primary = gtk::ColorDialogButton::new(Some(color_dialog.clone()));
        primary.set_rgba(&rgba(p0, 1.0));
        primary.set_valign(gtk::Align::Center);
        let secondary = gtk::ColorDialogButton::new(Some(color_dialog));
        secondary.set_rgba(&rgba(q0, 1.0));
        secondary.set_valign(gtk::Align::Center);
        let complement_btn = gtk::Button::builder()
            .label("Complement")
            .tooltip_text("Use the colour opposite the primary on the colour wheel")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let row1 = adw::ActionRow::builder()
            .title("Primary colour")
            .subtitle("Accent, keys and the aurora's main hue")
            .build();
        row1.add_suffix(&primary);
        let row2 = adw::ActionRow::builder()
            .title("Secondary colour")
            .subtitle("The second glow and the = key's gradient")
            .build();
        row2.add_suffix(&complement_btn);
        row2.add_suffix(&secondary);
        let custom_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        custom_list.append(&row1);
        custom_list.append(&row2);
        custom_list.set_margin_top(12);
        let custom_reveal = gtk::Revealer::builder()
            .child(&custom_list)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(ctx.hub.palette() == PaletteId::Custom)
            .build();
        let system_hint = gtk::Label::new(Some(
            "Uses your desktop's accent colour (the freedesktop portal setting your desktop or shell publishes) and its complement, and follows it live.",
        ));
        system_hint.add_css_class("dim-label");
        system_hint.add_css_class("caption");
        system_hint.set_wrap(true);
        system_hint.set_xalign(0.0);
        system_hint.set_margin_top(10);
        let system_reveal = gtk::Revealer::builder()
            .child(&system_hint)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(ctx.hub.palette() == PaletteId::System)
            .build();

        let apply_custom = {
            // Weak: these buttons own the closures that hold this.
            let (ctx, primary, secondary, refresh) = (
                ctx.clone(),
                primary.downgrade(),
                secondary.downgrade(),
                refresh_tiles.clone(),
            );
            Rc::new(move || {
                let (Some(primary), Some(secondary)) = (primary.upgrade(), secondary.upgrade())
                else {
                    return;
                };
                let c = |b: &gtk::ColorDialogButton| {
                    let r = b.rgba();
                    [r.red(), r.green(), r.blue()]
                };
                let (p, q) = (c(&primary), c(&secondary));
                ctx.hub.set_custom(p, q);
                {
                    let mut d = ctx.store.data.borrow_mut();
                    d.custom_primary = to_hex(p);
                    d.custom_secondary = to_hex(q);
                }
                ctx.store.persist();
                refresh();
            })
        };
        // No cover check here, unlike Settings' other controls: a picked
        // colour comes from the chooser, which covers Settings until it
        // has closed. Covered, the buttons themselves refuse (an Action).
        for b in [&primary, &secondary] {
            let apply = apply_custom.clone();
            b.connect_rgba_notify(move |_| apply());
        }
        {
            let (primary, secondary) = (primary.downgrade(), secondary.downgrade());
            complement_btn.connect_clicked(move |_| {
                let (Some(primary), Some(secondary)) = (primary.upgrade(), secondary.upgrade())
                else {
                    return;
                };
                let r = primary.rgba();
                secondary.set_rgba(&rgba(complement([r.red(), r.green(), r.blue()]), 1.0));
            });
        }
        {
            // Keep the System preview in step with live accent changes.
            let refresh = refresh_tiles.clone();
            ctx.hub.subscribe_while(&flow, move |_, _| refresh());
        }

        {
            let ctx = ctx.clone();
            let (custom_reveal, system_reveal) = (custom_reveal.clone(), system_reveal.clone());
            flow.connect_selected_children_changed(move |f| {
                // GTK carries out an assistive technology's Selection
                // request (SelectChild, DeselectChild, ClearSelection...)
                // whether or not the grid is covered (crate::inert), and
                // a single-selection grid can be left with none. Covered,
                // or left empty, it shows the saved palette again, and
                // nothing is applied or saved (R15-M-02): GTK then answers
                // that the child asked for isn't selected.
                let saved = ctx.hub.palette();
                let chosen = f
                    .selected_children()
                    .first()
                    .map(|c| PaletteId::ALL[c.index() as usize]);
                let Some(id) = chosen.filter(|&id| id == saved || !ctx.layers.covers(f)) else {
                    let at = PaletteId::ALL.iter().position(|&id| id == saved);
                    if let Some(child) = at.and_then(|i| f.child_at_index(i as i32)) {
                        f.select_child(&child);
                    }
                    return;
                };
                custom_reveal.set_reveal_child(id == PaletteId::Custom);
                system_reveal.set_reveal_child(id == PaletteId::System);
                if id != ctx.hub.palette() {
                    ctx.hub.set_palette(id);
                    ctx.store.data.borrow_mut().palette = id.key().into();
                    ctx.store.persist();
                    let s = ctx.hub.scheme();
                    let a = &ctx.aurora;
                    for (i, c) in s.blobs.iter().enumerate() {
                        a.pulse(
                            a.width() as f32 * (0.2 + 0.2 * i as f32),
                            a.height() as f32 * 0.5,
                            *c,
                            1.4,
                        );
                    }
                }
            });
        }
        group.add(&flow);
        group.add(&system_reveal);
        group.add(&custom_reveal);
        page.add(&group);

        // Motion.
        let group = adw::PreferencesGroup::builder().title("Motion").build();
        let anim = adw::SwitchRow::builder()
            .title("Living background")
            .subtitle("Let the aurora drift. Pauses whenever the window is in the background")
            .active(ctx.store.data.borrow().animated_background)
            .build();
        {
            let ctx = ctx.clone();
            anim.connect_active_notify(move |r| {
                let saved = ctx.store.data.borrow().animated_background;
                if ctx.layers.covers(r) {
                    // Covered, Settings changes nothing (R15-M-02).
                    if r.is_active() != saved {
                        r.set_active(saved);
                    }
                    return;
                }
                ctx.aurora.set_animated(r.is_active());
                ctx.store.data.borrow_mut().animated_background = r.is_active();
                ctx.store.persist();
            });
        }
        group.add(&anim);
        let hint = gtk::Label::new(Some(
            "Key animations follow the system's “reduce animations” setting.",
        ));
        hint.add_css_class("dim-label");
        hint.add_css_class("caption");
        hint.set_xalign(0.0);
        hint.set_margin_top(8);
        hint.set_wrap(true);
        group.add(&hint);
        page.add(&group);

        // Window.
        let group = adw::PreferencesGroup::builder().title("Window").build();
        let percent = |v: f64| format!("{}%", (v * 100.0).round());
        let saved = f64::from(crate::settings::backdrop_alpha(
            ctx.store.data.borrow().background_opacity,
        ));
        let scale = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            crate::settings::MIN_BACKGROUND_OPACITY,
            1.0,
            0.05,
        );
        scale.set_value(saved);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_valign(gtk::Align::Center);
        scale.set_width_request(160);
        // The range reports its value as a fraction (0.1); say it as shown.
        scale.update_property(&[
            gtk::accessible::Property::Label("Background opacity"),
            gtk::accessible::Property::ValueText(&percent(saved)),
        ]);
        let shown = gtk::Label::new(Some(&percent(saved)));
        shown.add_css_class("numeric");
        shown.set_width_chars(4);
        shown.set_xalign(1.0);
        let opacity_hint = |composited: bool| {
            if composited {
                "Lower lets your desktop show through, frosted if your compositor blurs translucent windows"
            } else {
                "Your display isn't compositing windows, so GMNB stays opaque until it does"
            }
        };
        let display = WidgetExt::display(&win.widget());
        let opacity = adw::ActionRow::builder()
            .title("Background opacity")
            .subtitle(opacity_hint(display.is_composited()))
            .build();
        {
            // A compositing manager can start or stop while this is open
            // (X11); the window follows, so the explanation (the row's
            // accessible description too) does as well.
            let row = opacity.downgrade();
            let handler = display.connect_composited_notify(move |d| {
                if let Some(row) = row.upgrade() {
                    row.set_subtitle(opacity_hint(d.is_composited()));
                }
            });
            let handler = std::cell::Cell::new(Some(handler));
            opacity.connect_destroy(move |_| {
                if let Some(h) = handler.take() {
                    display.disconnect(h);
                }
            });
        }
        opacity.add_suffix(&scale);
        opacity.add_suffix(&shown);
        {
            let (ctx, win, shown) = (ctx.clone(), win.widget(), shown.clone());
            let pending = Rc::default();
            scale.connect_value_changed(move |s| {
                if ctx.layers.covers(s) {
                    // Covered, Settings changes nothing (R15-M-02).
                    let saved = ctx.store.data.borrow().background_opacity;
                    let saved = f64::from(crate::settings::backdrop_alpha(saved));
                    if s.value() != saved {
                        s.set_value(saved);
                    }
                    return;
                }
                let v = s.value();
                shown.set_text(&percent(v));
                s.update_property(&[gtk::accessible::Property::ValueText(&percent(v))]);
                apply_backdrop(&win, &ctx.aurora, crate::settings::backdrop_alpha(v));
                ctx.store.data.borrow_mut().background_opacity = v;
                persist_at_rest(&ctx, &pending);
            });
        }
        group.add(&opacity);
        let from_env = crate::RENDERER_FROM_ENV.get().copied().unwrap_or(false);
        let vulkan = adw::SwitchRow::builder()
            .title("Vulkan acceleration")
            .subtitle(if from_env {
                "GSK_RENDERER is set where GMNB was started, and decides instead"
            } else {
                "Off draws in software: far less memory, plainer motion. Takes effect the next time GMNB starts"
            })
            .active(ctx.store.data.borrow().vulkan)
            .build();
        {
            let ctx = ctx.clone();
            vulkan.connect_active_notify(move |r| {
                let saved = ctx.store.data.borrow().vulkan;
                if ctx.layers.covers(r) {
                    // Covered, Settings changes nothing (R15-M-02).
                    if r.is_active() != saved {
                        r.set_active(saved);
                    }
                    return;
                }
                ctx.store.data.borrow_mut().vulkan = r.is_active();
                ctx.store.persist();
            });
        }
        group.add(&vulkan);
        page.add(&group);

        page.add(&graphing_group(&ctx));
    }

    // About.
    let group = adw::PreferencesGroup::builder().title("About").build();
    let about_row = adw::ButtonRow::builder()
        .title(format!("About {}", crate::APP_NAME))
        .end_icon_name("go-next-symbolic")
        .build();
    {
        let win = win.widget();
        about_row.connect_activated(move |_| about(&win));
    }
    group.add(&about_row);
    page.add(&group);

    dialog.add(&page);
    dialog.present(Some(&win.widget()));
}

/// Saves `ctx`'s settings once a slider has rested for a moment, not on
/// every step of a drag.
fn persist_at_rest(ctx: &Rc<Ctx>, pending: &Rc<RefCell<Option<gtk::glib::SourceId>>>) {
    if let Some(id) = pending.borrow_mut().take() {
        id.remove();
    }
    let (ctx, pending2) = (ctx.clone(), pending.clone());
    let id = gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        pending2.borrow_mut().take();
        ctx.store.persist();
    });
    *pending.borrow_mut() = Some(id);
}

/// Graphing: "Number precision", a slider from 5 to 20 significant digits
/// and then Off, with the calculators that round so labelled. Moving it
/// re-plots and re-analyses at once (the graphing page follows
/// `Ctx::precision`).
fn graphing_group(ctx: &Rc<Ctx>) -> adw::PreferencesGroup {
    use appcore::graph::{NUMBER_PRECISION_HELP, NumberPrecision as P};
    let group = adw::PreferencesGroup::builder().title("Graphing").build();
    let now = ctx.precision.get();
    let title = adw::ActionRow::builder()
        .title("Number precision")
        .subtitle(NUMBER_PRECISION_HELP)
        .activatable(false)
        .build();
    let (first, last) = (*P::POSITIONS.start(), *P::POSITIONS.end());
    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        f64::from(first),
        f64::from(last),
        1.0,
    );
    scale.set_round_digits(0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_value(f64::from(now.position()));
    // Labels alternate above and below: 14 and 15, 20 and Off, are
    // neighbours.
    let mut marks: Vec<(u8, &str, gtk::PositionType)> = P::NOTCHES
        .iter()
        .zip([
            gtk::PositionType::Bottom,
            gtk::PositionType::Top,
            gtk::PositionType::Bottom,
            gtk::PositionType::Top,
        ])
        .map(|(&(digits, short, _), side)| (digits, short, side))
        .collect();
    marks.push((first, "5", gtk::PositionType::Bottom));
    marks.push((P::MAX, "20", gtk::PositionType::Top));
    marks.push((last, "Off", gtk::PositionType::Bottom));
    for (position, label, side) in marks {
        let label = gtk::glib::markup_escape_text(label);
        scale.add_mark(f64::from(position), side, Some(&label));
    }
    let shown = gtk::Label::new(Some(&now.describe()));
    shown.add_css_class("caption");
    shown.add_css_class("numeric");
    scale.update_property(&[
        gtk::accessible::Property::Label("Number precision"),
        gtk::accessible::Property::ValueText(&now.describe()),
        gtk::accessible::Property::Description(NUMBER_PRECISION_HELP),
    ]);
    {
        let (ctx, shown) = (ctx.clone(), shown.clone());
        let pending = Rc::default();
        scale.connect_value_changed(move |s| {
            if ctx.layers.covers(s) {
                // Covered, Settings changes nothing (R15-M-02).
                let saved = f64::from(ctx.precision.get().position());
                if s.value() != saved {
                    s.set_value(saved);
                }
                return;
            }
            let p = P::at_position(s.value());
            // GTK rounds only what the pointer and the keys set: a value
            // between positions (an AT client's 20.4) moves to the
            // setting's own, so the value read back is the setting's, as
            // in DGMNB (R14-L-02). That comes back here, on a position.
            let position = f64::from(p.position());
            if s.value() != position {
                s.set_value(position);
                return;
            }
            shown.set_text(&p.describe());
            s.update_property(&[gtk::accessible::Property::ValueText(&p.describe())]);
            ctx.precision.set(p);
            ctx.store.data.borrow_mut().literal_digits = p;
            persist_at_rest(&ctx, &pending);
        });
    }
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.set_margin_top(6);
    column.set_margin_bottom(10);
    column.set_margin_start(12);
    column.set_margin_end(12);
    column.append(&scale);
    column.append(&shown);
    let slider = gtk::ListBoxRow::builder()
        .child(&column)
        .activatable(false)
        .selectable(false)
        .build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list.append(&title);
    list.append(&slider);
    group.add(&list);
    group
}

pub fn about(parent: &adw::ApplicationWindow) {
    let about = adw::AboutDialog::builder()
        .application_name(crate::APP_NAME)
        .application_icon(crate::APP_ID)
        .developer_name("GMNB contributors")
        .version(env!("CARGO_PKG_VERSION"))
        .license_type(gtk::License::MitX11)
        .comments(
            "GlazeMyNumbers,Baby — a Rust port of the open-source Windows Calculator: the \
             original arbitrary-precision engine and every mode, with considerably more shimmer \
             than strictly necessary.\n\n\
             Not affiliated with or endorsed by Microsoft.",
        )
        .website("https://github.com/Go08er/GlazeMyNumbersBaby")
        .issue_url("https://github.com/Go08er/GlazeMyNumbersBaby/issues")
        .copyright(
            "© Microsoft Corporation (original Calculator)\n© 2026 GMNB contributors (Rust port)",
        )
        .build();
    about.add_credit_section(
        Some("Based on"),
        &["Windows Calculator by Microsoft https://github.com/microsoft/calculator"],
    );
    about.add_legal_section(
        "Windows Calculator",
        Some("Copyright (c) Microsoft Corporation. All rights reserved."),
        gtk::License::MitX11,
        None,
    );
    // Custom licence text is Pango markup: escape the plain-text notices.
    about.add_legal_section(
        "Outfit typeface",
        Some("Copyright 2021 The Outfit Project Authors"),
        gtk::License::Custom,
        Some(&gtk::glib::markup_escape_text(OUTFIT)),
    );
    about.add_legal_section(
        "CORE-MATH",
        Some("Correctly rounded maths functions, used by Graphing"),
        gtk::License::Custom,
        Some(&gtk::glib::markup_escape_text(CORE_MATH)),
    );
    about.add_legal_section("Rust crates", None, gtk::License::Custom, Some(CRATES));
    about.add_legal_section(
        "Exchange rates",
        None,
        gtk::License::Custom,
        Some("Currency reference rates from central banks (European Central Bank and others), served by the Frankfurter API. Rates are informational and may lag the market."),
    );
    about.connect_activate_link(|about, uri| {
        let ours = uri == THIRD_PARTY;
        if ours {
            third_party_licences(about);
        }
        ours
    });
    about.present(Some(parent));
}

const OUTFIT: &str = include_str!("../assets/fonts/OFL-Outfit.txt");
const CORE_MATH: &str = concat!(
    include_str!("../../../crates/crmath/vendor/COPYRIGHT"),
    "\n",
    include_str!("../../../crates/crmath/vendor/LICENSE"),
);
/// The About dialog's link to [`third_party_licences`].
const THIRD_PARTY: &str = "gmnb:third-party-licences";
const CRATES: &str = "GMNB is built from many Rust crates, each under its own licence (MIT, \
     Apache 2.0, BSD and others). <a href=\"gmnb:third-party-licences\">Show their \
     licences</a>; they are also installed with GMNB, as THIRD-PARTY-LICENSES.txt.";

/// THIRD-PARTY-LICENSES.txt (CORE-MATH, smithay-clipboard and every Rust
/// crate) in a text view, which lays out only what's on screen: as a
/// legal section's label, its 160 KB took over half a second per layout.
fn third_party_licences(parent: &impl IsA<gtk::Widget>) {
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text(include_str!("../../../THIRD-PARTY-LICENSES.txt"));
    let view = gtk::TextView::builder()
        .buffer(&buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(12)
        .bottom_margin(12)
        .left_margin(12)
        .right_margin(12)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&scroll));
    let dialog = adw::Dialog::builder()
        .title("Third-party licences")
        .content_width(640)
        .content_height(600)
        .child(&toolbar)
        .build();
    dialog.present(Some(parent));
}

fn draw_swatch(cr: &gtk::cairo::Context, w: f64, h: f64, s: &Scheme) {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    let r = 12.0;
    cr.new_sub_path();
    cr.arc(w - r, r, r, -FRAC_PI_2, 0.0);
    cr.arc(w - r, h - r, r, 0.0, FRAC_PI_2);
    cr.arc(r, h - r, r, FRAC_PI_2, PI);
    cr.arc(r, r, r, PI, 1.5 * PI);
    cr.close_path();
    cr.clip();
    let c = |c: [f32; 3]| (c[0] as f64, c[1] as f64, c[2] as f64);
    let base = gtk::cairo::LinearGradient::new(0.0, 0.0, w * 0.3, h);
    let (r0, g0, b0) = c(s.base_top);
    let (r1, g1, b1) = c(s.base_bottom);
    base.add_color_stop_rgb(0.0, r0, g0, b0);
    base.add_color_stop_rgb(1.0, r1, g1, b1);
    let _ = cr.set_source(&base);
    let _ = cr.paint();
    for (i, (x, y)) in [(0.2, 0.2), (0.85, 0.35), (0.35, 0.95), (0.9, 0.95)]
        .iter()
        .enumerate()
    {
        let g = gtk::cairo::RadialGradient::new(x * w, y * h, 0.0, x * w, y * h, w * 0.6);
        let (r, gg, b) = c(s.blobs[i]);
        g.add_color_stop_rgba(0.0, r, gg, b, (s.blob_alpha * 1.4).min(1.0) as f64);
        g.add_color_stop_rgba(1.0, r, gg, b, 0.0);
        let _ = cr.set_source(&g);
        let _ = cr.paint();
    }
    let hot = gtk::cairo::LinearGradient::new(w - 34.0, h - 26.0, w - 10.0, h - 8.0);
    let (r0, g0, b0) = c(s.hot_a);
    let (r1, g1, b1) = c(s.hot_b);
    hot.add_color_stop_rgb(0.0, r0, g0, b0);
    hot.add_color_stop_rgb(1.0, r1, g1, b1);
    let _ = cr.set_source(&hot);
    cr.arc(w - 20.0, h - 18.0, 9.0, 0.0, TAU);
    let _ = cr.fill();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legal sections' text is Pango markup, and text that doesn't parse
    /// shows as nothing (as the Outfit licence's "PERMISSION & CONDITIONS"
    /// once did).
    #[test]
    fn legal_text_is_markup() {
        assert!(pango::parse_markup(OUTFIT, '\0').is_err());
        for text in [OUTFIT, CORE_MATH] {
            let escaped = gtk::glib::markup_escape_text(text);
            let (_, plain, _) = pango::parse_markup(&escaped, '\0').unwrap();
            assert_eq!(plain, text);
        }
        // GtkLabel takes the links out before Pango sees the rest.
        let link = format!("<a href=\"{THIRD_PARTY}\">");
        assert!(CRATES.contains(&link));
        let rest = CRATES.replace(&link, "").replace("</a>", "");
        let (_, plain, _) = pango::parse_markup(&rest, '\0').unwrap();
        assert!(plain.contains("Show their licences"));
        assert!(CORE_MATH.contains("Alexei Sibidanov") && CORE_MATH.contains("Permission is"));
    }
}
