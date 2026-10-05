//! Graphing session state that outlives the widgets: saved equations
//! (text, colour, line style, visibility), validated on restore, and the
//! limits both twins enforce on input.

use graphing::equation::LineStyle;
use serde::{Deserialize, Serialize};

/// Longest equation text the apps accept (typed, pasted or restored).
pub const MAX_EQUATION_CHARS: usize = 1000;

/// Upstream's limit on simultaneous equations.
pub const MAX_EQUATIONS: usize = graphing::graph::MAX_EQUATIONS;

/// The line styles offered in the equation style flyout.
pub const STYLES: [(LineStyle, &str, &str); 3] = [
    (LineStyle::Solid, "solid", "Solid"),
    (LineStyle::Dot, "dot", "Dot"),
    (LineStyle::Dash, "dash", "Dash"),
];

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SavedEquation {
    pub text: String,
    /// Index into the scheme's series colours (taken modulo their count).
    pub color: usize,
    pub style: String,
    pub hidden: bool,
}

pub fn style_key(style: LineStyle) -> &'static str {
    match style {
        LineStyle::Dot => "dot",
        LineStyle::Dash => "dash",
        _ => "solid",
    }
}

pub fn style_from_key(key: &str) -> Option<LineStyle> {
    STYLES.iter().find(|s| s.1 == key).map(|s| s.0)
}

/// At most [`MAX_EQUATION_CHARS`] characters of `text`.
pub fn clamp_text(text: &str) -> &str {
    match text.char_indices().nth(MAX_EQUATION_CHARS) {
        Some((i, _)) => &text[..i],
        None => text,
    }
}

/// Saved equations from settings, made safe to replay: blank entries are
/// dropped, text is clamped to the length limit (an over-long or malicious
/// expression then shows as an equation error instead of being parsed in
/// full), and at most [`MAX_EQUATIONS`] are kept. Each entry is read on its
/// own and field by field, so a damaged one (a colour that is not a number)
/// loses only that field, and an entry that is not an object only itself.
pub fn restore(value: Option<serde_json::Value>) -> Vec<SavedEquation> {
    let list: Vec<SavedEquation> = match value {
        Some(serde_json::Value::Array(items)) => items
            .into_iter()
            .filter(serde_json::Value::is_object)
            .map(|item| crate::settings::from_value_lenient(item).0)
            .collect(),
        _ => Vec::new(),
    };
    sanitize(list)
}

/// `"x^2;sin(x)"` (the dev hook format) → equations with distinct colours.
pub fn from_list(list: &str) -> Vec<SavedEquation> {
    sanitize(
        list.split(';')
            .enumerate()
            .map(|(i, t)| SavedEquation {
                text: t.into(),
                color: i,
                ..Default::default()
            })
            .collect(),
    )
}

fn sanitize(list: Vec<SavedEquation>) -> Vec<SavedEquation> {
    list.into_iter()
        .filter(|e| !e.text.trim().is_empty())
        .take(MAX_EQUATIONS)
        .map(|mut e| {
            e.text = clamp_text(&e.text).to_string();
            e.color %= 1 << 16;
            e
        })
        .collect()
}

/// The colour index for the next new equation.
pub fn next_color(saved: &[SavedEquation]) -> usize {
    saved.iter().map(|e| e.color + 1).max().unwrap_or(0)
}

/// "Number precision": how many significant digits each number typed in
/// a graph equation keeps, rounded on entry as a TI-84 does; or Off, the
/// decimals exactly as typed. Saved as the digits, or `null` for Off; a
/// saved value outside [`MIN`](Self::MIN)..=[`MAX`](Self::MAX) is refused,
/// so a settings file loads it as the default (field by field, see
/// `appcore::settings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Option<u8>", into = "Option<u8>")]
pub struct NumberPrecision(Option<u8>);

impl NumberPrecision {
    /// The fewest digits offered.
    pub const MIN: u8 = 5;
    /// The most digits offered; the slider's next position is Off.
    pub const MAX: u8 = 20;
    /// Exactly as typed.
    pub const OFF: NumberPrecision = NumberPrecision(None);
    /// The default: a TI-84 Plus CE's 14 digits.
    pub const DEFAULT: NumberPrecision = NumberPrecision(Some(14));
    /// The labelled notches: digits, a short mark label, and who rounds to
    /// them.
    pub const NOTCHES: [(u8, &'static str, &'static str); 4] = [
        (10, "Casio", "Casio display"),
        (12, "HP", "HP"),
        (14, "TI-84", "TI-84 Plus CE"),
        (15, "double", "Casio internal, double"),
    ];
    /// The slider's positions: the digits, then Off.
    pub const POSITIONS: std::ops::RangeInclusive<u8> = Self::MIN..=Self::MAX + 1;

    /// `digits` significant digits (`None`: Off), if offered.
    pub fn new(digits: Option<u8>) -> Option<NumberPrecision> {
        match digits {
            Some(d) if !(Self::MIN..=Self::MAX).contains(&d) => None,
            _ => Some(NumberPrecision(digits)),
        }
    }

    /// The digits kept, or `None` for exactly as typed (the graphing
    /// engine's `literal_digits`).
    pub fn digits(self) -> Option<u8> {
        self.0
    }

    /// The setting at a slider position (see [`POSITIONS`](Self::POSITIONS)),
    /// rounded and clamped to one.
    pub fn at_position(position: f64) -> NumberPrecision {
        let off = f64::from(Self::MAX + 1);
        if position.is_nan() {
            return Self::DEFAULT;
        }
        let p = position.round().clamp(f64::from(Self::MIN), off);
        if p >= off {
            Self::OFF
        } else {
            NumberPrecision(Some(p as u8))
        }
    }

    /// This setting's slider position.
    pub fn position(self) -> u8 {
        self.0.unwrap_or(Self::MAX + 1)
    }

    /// What it does, in words: "14 digits (TI-84 Plus CE)", "9 digits",
    /// "Off: exact as typed".
    pub fn describe(self) -> String {
        let Some(d) = self.0 else {
            return "Off: exact as typed".into();
        };
        match Self::NOTCHES.iter().find(|n| n.0 == d) {
            Some((_, _, who)) => format!("{d} digits ({who})"),
            None => format!("{d} digits"),
        }
    }
}

impl Default for NumberPrecision {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<Option<u8>> for NumberPrecision {
    type Error = String;
    fn try_from(digits: Option<u8>) -> Result<Self, String> {
        NumberPrecision::new(digits).ok_or_else(|| {
            format!(
                "number precision is {}..={} digits, or null for off",
                Self::MIN,
                Self::MAX
            )
        })
    }
}

impl From<NumberPrecision> for Option<u8> {
    fn from(p: NumberPrecision) -> Option<u8> {
        p.0
    }
}

/// What the number precision setting does, said alike by both twins.
pub const NUMBER_PRECISION_HELP: &str = "Each number typed in an equation is rounded to this \
     many significant digits, as a graphing calculator does. Off keeps it exactly as typed.";

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
    #[serde(default)]
    struct Saved {
        literal_digits: NumberPrecision,
        mode: String,
    }

    /// Old settings files (no field) load as 14; `null` is Off; anything
    /// outside 5..=20 is refused, and loads as the default field by field.
    #[test]
    fn number_precision_loads_and_saves() {
        let old: Saved = serde_json::from_str(r#"{"mode": "graphing"}"#).unwrap();
        assert_eq!(old.literal_digits, NumberPrecision::DEFAULT);
        assert_eq!(old.literal_digits.digits(), Some(14));
        let off: Saved = serde_json::from_str(r#"{"literal_digits": null}"#).unwrap();
        assert_eq!(off.literal_digits, NumberPrecision::OFF);
        for bad in ["4", "21", "99", "-1", "\"14\"", "14.5"] {
            let text = format!(r#"{{"literal_digits": {bad}, "mode": "graphing"}}"#);
            assert!(serde_json::from_str::<Saved>(&text).is_err(), "{bad}");
            let (s, complete) =
                crate::settings::from_value_lenient::<Saved>(serde_json::from_str(&text).unwrap());
            assert!(!complete);
            assert_eq!(s.literal_digits, NumberPrecision::DEFAULT, "{bad}");
            assert_eq!(s.mode, "graphing");
        }
        for p in NumberPrecision::POSITIONS {
            let p = NumberPrecision::at_position(f64::from(p));
            let text = serde_json::to_string(&p).unwrap();
            assert_eq!(serde_json::from_str::<NumberPrecision>(&text).unwrap(), p);
        }
        assert_eq!(
            serde_json::to_string(&NumberPrecision::OFF).unwrap(),
            "null"
        );
        assert_eq!(
            serde_json::to_string(&NumberPrecision::DEFAULT).unwrap(),
            "14"
        );
    }

    #[test]
    fn number_precision_slider_and_words() {
        let at = NumberPrecision::at_position;
        assert_eq!(at(5.0).digits(), Some(5));
        assert_eq!(at(20.0).digits(), Some(20));
        assert_eq!(at(21.0), NumberPrecision::OFF);
        assert_eq!(at(13.6).digits(), Some(14));
        assert_eq!(at(-3.0).digits(), Some(5));
        assert_eq!(at(1e9), NumberPrecision::OFF);
        assert_eq!(at(f64::NAN), NumberPrecision::DEFAULT);
        assert_eq!(NumberPrecision::OFF.position(), 21);
        assert_eq!(NumberPrecision::DEFAULT.position(), 14);
        assert_eq!(
            NumberPrecision::DEFAULT.describe(),
            "14 digits (TI-84 Plus CE)"
        );
        assert_eq!(at(10.0).describe(), "10 digits (Casio display)");
        assert_eq!(at(12.0).describe(), "12 digits (HP)");
        assert_eq!(at(15.0).describe(), "15 digits (Casio internal, double)");
        assert_eq!(at(9.0).describe(), "9 digits");
        assert_eq!(NumberPrecision::OFF.describe(), "Off: exact as typed");
    }

    #[test]
    fn hostile_saved_state_is_tamed() {
        let mut list = vec![serde_json::json!({"text": "√".repeat(20_000) + "x"})];
        for i in 0..30 {
            list.push(serde_json::json!({"text": format!("x^{i}"), "color": usize::MAX}));
        }
        list.push(serde_json::json!({"text": "   "}));
        let eqs = restore(Some(serde_json::Value::Array(list)));
        assert_eq!(eqs.len(), MAX_EQUATIONS);
        assert_eq!(eqs[0].text.chars().count(), MAX_EQUATION_CHARS);
        assert!(eqs.iter().all(|e| e.color < 1 << 16));
        assert!(restore(Some(serde_json::json!("garbage"))).is_empty());
    }

    #[test]
    fn one_damaged_equation_does_not_drop_the_others() {
        let eqs = restore(Some(serde_json::json!([
            {"text": "x^2", "color": 2},
            {"text": "sin(x)", "color": "red", "hidden": true},
            "not an equation",
            {"text": 5},
            {"text": "x+1", "style": "dash"},
        ])));
        let texts: Vec<&str> = eqs.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["x^2", "sin(x)", "x+1"]);
        assert_eq!(eqs[0].color, 2);
        assert_eq!(eqs[1].color, 0);
        assert!(eqs[1].hidden);
        assert_eq!(eqs[2].style, "dash");
    }

    #[test]
    fn styles_round_trip() {
        for (s, key, _) in STYLES {
            assert_eq!(style_from_key(key), Some(s));
            assert_eq!(style_key(s), key);
        }
        assert_eq!(style_from_key("zigzag"), None);
    }

    #[test]
    fn dev_list_assigns_colours() {
        let eqs = from_list("x;;y=x^2");
        assert_eq!(eqs.len(), 2);
        assert_eq!(next_color(&eqs), 3);
    }

    #[test]
    fn graphs_can_move_to_worker_threads() {
        fn send<T: Send + Clone + 'static>() {}
        send::<graphing::Graph>();
    }
}
