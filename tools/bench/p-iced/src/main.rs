use iced::widget::{button, column, row, text, text_input};
use iced::{Element, Font, Length};
const KEYS: [&str; 24] = ["%", "CE", "C", "⌫", "1/x", "x²", "√x", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "±", "0", ".", "="];
#[derive(Default)] struct App { s: String }
#[derive(Debug, Clone)] enum Msg { Edit(String), Key }
fn update(a: &mut App, m: Msg) { if let Msg::Edit(s) = m { a.s = s; } }
fn view(a: &App) -> Element<'_, Msg> {
    let mut col = column![text("1,234,567.89").size(56), text_input("y = x²", &a.s).on_input(Msg::Edit)].spacing(6).padding(12);
    for r in KEYS.chunks(4) {
        col = col.push(row(r.iter().map(|k| button(text(*k).size(20).center()).on_press(Msg::Key).width(Length::Fill).height(Length::Fill).into())).spacing(6).height(Length::Fill));
    }
    col.into()
}
fn main() -> iced::Result {
    iced::application(App::default, update, view)
        .font(include_bytes!("../../../../apps/gmnb/assets/fonts/Outfit-Variable.ttf").as_slice())
        .default_font(Font::with_name("Outfit"))
        .window_size((760.0, 700.0))
        .run()
}
