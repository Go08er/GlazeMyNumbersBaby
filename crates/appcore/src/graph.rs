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
/// full), and at most [`MAX_EQUATIONS`] are kept.
pub fn restore(value: Option<serde_json::Value>) -> Vec<SavedEquation> {
    let list: Vec<SavedEquation> = value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

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
