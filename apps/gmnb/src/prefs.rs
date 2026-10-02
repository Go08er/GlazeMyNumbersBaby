//! Settings (upstream Settings.xaml: theme + about), plus palette and motion.

use std::rc::Rc;

use adw::prelude::*;

use std::cell::{Cell, RefCell};

use crate::settings::Persist;
use crate::theme::{PaletteId, Scheme, complement, rgba, to_hex};
use crate::window::{Window, apply_theme_setting};

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
                let Some(child) = f.selected_children().into_iter().next() else {
                    return;
                };
                let id = PaletteId::ALL[child.index() as usize];
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
    about.add_legal_section(
        "Outfit typeface",
        Some("Copyright 2021 The Outfit Project Authors"),
        gtk::License::Custom,
        Some(include_str!("../assets/fonts/OFL-Outfit.txt")),
    );
    about.add_legal_section(
        "Exchange rates",
        None,
        gtk::License::Custom,
        Some("Currency reference rates from central banks (European Central Bank and others), served by the Frankfurter API. Rates are informational and may lag the market."),
    );
    about.present(Some(parent));
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
