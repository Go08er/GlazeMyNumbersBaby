//! States, resolved by the app and not by the stylesheet (docs/theming.md,
//! "Names of the custom properties"): for a control and each combination
//! of states, the finished values.

use crate::types::*;

/// A control as it looks in one combination of states.
#[derive(Clone, Debug, PartialEq)]
pub struct Finished {
    /// The background in effect, with every overlay laid over it (topmost
    /// first).
    pub background: Fill,
    /// Text colour, any `fade` folded in.
    pub color: Rgba,
    /// Icon colour, any `fade` folded in.
    pub icon_color: Rgba,
    pub border: Option<Border>,
    pub shadow: Vec<Shadow>,
    pub opacity: f32,
    pub brightness: f32,
    pub saturation: f32,
    pub scale: f32,
    pub rotate: f32,
    pub transition: Transition,
    pub font_weight: f32,
}

/// The state combinations a control's properties are written for, by the
/// name used in the custom properties, and which states make each up, in
/// the order they apply (rest, `checked`, `hover` or `active`, `disabled`:
/// later ones win).
pub const COMBOS: [(&str, &[Which]); 7] = [
    ("hover", &[Which::Hover]),
    ("active", &[Which::Active]),
    ("checked", &[Which::Checked]),
    ("checked-hover", &[Which::Checked, Which::Hover]),
    ("checked-active", &[Which::Checked, Which::Active]),
    ("disabled", &[Which::Disabled]),
    ("checked-disabled", &[Which::Checked, Which::Disabled]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Hover,
    Active,
    Checked,
    Disabled,
}

impl Control {
    pub fn state(&self, which: Which) -> &State {
        match which {
            Which::Hover => &self.hover,
            Which::Active => &self.active,
            Which::Checked => &self.checked,
            Which::Disabled => &self.disabled,
        }
    }

    /// The control with `states` applied in order (none: at rest).
    pub fn finished(&self, states: &[Which]) -> Finished {
        let rest = Finished {
            background: self.background.clone(),
            color: self.color,
            icon_color: self.icon.color.unwrap_or(self.color),
            border: self.border,
            shadow: self.shadow.clone(),
            opacity: self.opacity,
            brightness: 1.0,
            saturation: 1.0,
            scale: 1.0,
            rotate: 0.0,
            transition: self.transition.clone(),
            font_weight: self.font.weight,
        };
        let states: Vec<&State> = states.iter().map(|w| self.state(*w)).collect();
        finish(rest, self.icon.color, &states)
    }
}

/// A field's states, in the order they apply: `error` wins over `focus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldWhich {
    Focus,
    Error,
}

/// The state combinations a field's properties are written for (in place
/// of a control's), by the name used in the custom properties.
pub const FIELD_COMBOS: [(&str, &[FieldWhich]); 3] = [
    ("focus", &[FieldWhich::Focus]),
    ("error", &[FieldWhich::Error]),
    ("error-focus", &[FieldWhich::Focus, FieldWhich::Error]),
];

impl Field {
    pub fn state(&self, which: FieldWhich) -> &State {
        match which {
            FieldWhich::Focus => &self.focus,
            FieldWhich::Error => &self.error,
        }
    }

    /// The field with `states` applied in order (none: at rest).
    pub fn finished(&self, states: &[FieldWhich]) -> Finished {
        let rest = Finished {
            background: self.background.clone(),
            color: self.color,
            icon_color: self.color,
            border: self.border,
            shadow: self.shadow.clone(),
            opacity: 1.0,
            brightness: 1.0,
            saturation: 1.0,
            scale: 1.0,
            rotate: 0.0,
            transition: Transition::default(),
            font_weight: self.font.weight,
        };
        let states: Vec<&State> = states.iter().map(|w| self.state(*w)).collect();
        finish(rest, None, &states)
    }
}

/// `rest` with `states` applied in order. `icon`: the icon's own colour at
/// rest, if it has one (else it follows the text's).
fn finish(rest: Finished, icon: Option<Rgba>, states: &[&State]) -> Finished {
    let mut background = rest.background.clone();
    let mut overlays: Vec<Rgba> = Vec::new();
    let mut color = rest.color;
    let mut icon_color = icon;
    let mut fade = 1.0;
    let mut f = rest;
    for s in states {
        if let Some(b) = &s.background {
            background = b.clone();
        }
        if let Some(o) = s.overlay {
            overlays.push(o);
        }
        if let Some(c) = s.color {
            color = c;
        }
        if let Some(c) = s.icon_color {
            icon_color = Some(c);
        }
        if let Some(k) = s.fade {
            fade *= k;
        }
        if let Some(b) = s.border {
            f.border = b;
        }
        if let Some(sh) = &s.shadow {
            f.shadow = sh.clone();
        }
        if let Some(o) = s.opacity {
            f.opacity = o;
        }
        if let Some(b) = s.brightness {
            f.brightness = b;
        }
        if let Some(v) = s.saturation {
            f.saturation = v;
        }
        if let Some(v) = s.scale {
            f.scale = v;
        }
        if let Some(v) = s.rotate {
            f.rotate = v;
        }
        if let Some(t) = &s.transition {
            f.transition = t.clone();
        }
        if let Some(w) = s.font_weight {
            f.font_weight = w;
        }
    }
    // An overlay always lands on top of the background then in effect:
    // the latest topmost.
    let mut layers: Vec<Paint> = overlays.into_iter().rev().map(Paint::Color).collect();
    layers.extend(background.0);
    f.background = Fill(layers);
    f.color = alpha(color, fade);
    // Without a colour of its own an icon takes its control's.
    f.icon_color = alpha(icon_color.unwrap_or(color), fade);
    f
}

/// `top` composited onto `bottom`.
pub fn over(top: Rgba, bottom: Rgba) -> Rgba {
    appcore::color::over(top, bottom)
}

impl Fill {
    /// The colours this fill shows over `under`, for contrast checks: a
    /// gradient is sampled at its two ends and its middle (the worst of
    /// each layer combined with the others' same sample).
    pub fn samples_over(&self, under: Rgba) -> Vec<Rgba> {
        let mut samples = vec![under; 3];
        for paint in self.0.iter().rev() {
            let layer = paint.samples();
            for (i, s) in samples.iter_mut().enumerate() {
                let top = match layer.len() {
                    0 => continue,
                    1 => layer[0],
                    _ => layer[i.min(layer.len() - 1)],
                };
                *s = over(top, *s);
            }
        }
        samples
    }
}
