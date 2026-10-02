use gtk::prelude::*;
const KEYS: [&str; 24] = ["%", "CE", "C", "⌫", "1/x", "x²", "√x", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "±", "0", ".", "="];
fn main() {
    let app = gtk::Application::new(Some("io.github.test.PGtk"), Default::default());
    app.connect_activate(|app| {
        let w = gtk::ApplicationWindow::builder().application(app).title("gtk").default_width(760).default_height(700).build();
        let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
        col.append(&gtk::Label::new(Some("1,234,567.89")));
        col.append(&gtk::Entry::new());
        let grid = gtk::Grid::new();
        grid.set_row_homogeneous(true); grid.set_column_homogeneous(true); grid.set_vexpand(true);
        for (i, k) in KEYS.iter().enumerate() { grid.attach(&gtk::Button::with_label(k), (i % 4) as i32, (i / 4) as i32, 1, 1); }
        col.append(&grid);
        w.set_child(Some(&col));
        w.present();
    });
    app.run();
}
