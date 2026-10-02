use fltk::{prelude::*, *};
const KEYS: [&str; 24] = ["%", "CE", "C", "⌫", "1/x", "x²", "√x", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "±", "0", ".", "="];
fn main() {
    let a = app::App::default().with_scheme(app::Scheme::Gtk);
    let mut w = window::Window::default().with_size(760, 700).with_label("fltk");
    let mut d = frame::Frame::new(16, 16, 728, 100, "1,234,567.89");
    d.set_label_size(56);
    input::Input::new(16, 130, 728, 36, "");
    let (kw, kh) = (178, 80);
    for (i, k) in KEYS.iter().enumerate() {
        let (c, r) = ((i % 4) as i32, (i / 4) as i32);
        let mut b = button::Button::new(16 + c * (kw + 6), 190 + r * (kh + 6), kw, kh, *k);
        b.set_label_size(20);
    }
    w.end();
    w.show();
    a.run().unwrap();
}
