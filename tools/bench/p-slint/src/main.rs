slint::slint! {
    import { Button, LineEdit, VerticalBox, GridBox } from "std-widgets.slint";
    import "../../../../apps/gmnb/assets/fonts/Outfit-Variable.ttf";
    export component App inherits Window {
        title: "slint";
        preferred-width: 760px;
        preferred-height: 700px;
        default-font-family: "Outfit";
        VerticalBox {
            Text { text: "1,234,567.89"; font-size: 56px; horizontal-alignment: right; }
            LineEdit { placeholder-text: "y = x²"; }
            GridBox {
                Row { Button { text: "%"; } Button { text: "CE"; } Button { text: "C"; } Button { text: "⌫"; } }
                Row { Button { text: "1/x"; } Button { text: "x²"; } Button { text: "√x"; } Button { text: "÷"; } }
                Row { Button { text: "7"; } Button { text: "8"; } Button { text: "9"; } Button { text: "×"; } }
                Row { Button { text: "4"; } Button { text: "5"; } Button { text: "6"; } Button { text: "−"; } }
                Row { Button { text: "1"; } Button { text: "2"; } Button { text: "3"; } Button { text: "+"; } }
                Row { Button { text: "±"; } Button { text: "0"; } Button { text: "."; } Button { text: "="; primary: true; } }
            }
        }
    }
}
fn main() { App::new().unwrap().run().unwrap(); }
