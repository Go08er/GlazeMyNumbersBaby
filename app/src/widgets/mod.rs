pub mod aurora;
pub mod bitflip;
pub mod calc_panel;
pub mod display;
pub mod graph_view;
pub mod icon;
pub mod keypad;
pub mod width_bin;

/// Whether decorative animations should run (off when the system asks for
/// reduced motion).
pub fn animations_enabled() -> bool {
    gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_animations())
}
