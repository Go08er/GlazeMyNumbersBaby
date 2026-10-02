//! SVG path data (`M5 12h14a1 1 0 1 1 0 2z…`) → tiny-skia paths, for the
//! shared line icons (`appcore::icons`).

use std::f32::consts::PI;

use tiny_skia::{Path, PathBuilder};

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b',' | b'\n' | b'\t' | b'\r')
        {
            self.i += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip();
        let c = *self.s.get(self.i)?;
        if c.is_ascii_alphabetic() && !c.eq_ignore_ascii_case(&b'e') {
            self.i += 1;
            Some(c)
        } else {
            None
        }
    }

    fn at_number(&mut self) -> bool {
        self.skip();
        matches!(self.s.get(self.i), Some(b'0'..=b'9' | b'.' | b'-' | b'+'))
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let start = self.i;
        let s = self.s;
        if matches!(s.get(self.i), Some(b'-' | b'+')) {
            self.i += 1;
        }
        let mut dot = false;
        while let Some(&c) = s.get(self.i) {
            match c {
                b'0'..=b'9' => self.i += 1,
                b'.' if !dot => {
                    dot = true;
                    self.i += 1
                }
                b'e' | b'E' if matches!(s.get(self.i + 1), Some(b'0'..=b'9' | b'-' | b'+')) => {
                    self.i += 2;
                    while matches!(s.get(self.i), Some(b'0'..=b'9')) {
                        self.i += 1;
                    }
                    break;
                }
                _ => break,
            }
        }
        std::str::from_utf8(&s[start..self.i]).ok()?.parse().ok()
    }

    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

pub fn parse(d: &str) -> Option<Path> {
    let mut lx = Lexer {
        s: d.as_bytes(),
        i: 0,
    };
    let mut pb = PathBuilder::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    // Last control point, for smooth curves.
    let mut last_c: Option<(f32, f32)> = None;
    let mut last_q: Option<(f32, f32)> = None;
    let mut cmd = lx.command()?;
    loop {
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        let mut cc = None;
        let mut qc = None;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (nx, ny) = (lx.number()? + ox, lx.number()? + oy);
                pb.move_to(nx, ny);
                (x, y, sx, sy) = (nx, ny, nx, ny);
                // Further pairs are implicit line-tos.
                cmd = if rel { b'l' } else { b'L' };
                if !lx.at_number() {
                    match lx.command() {
                        Some(c) => cmd = c,
                        None => break,
                    }
                }
                continue;
            }
            b'L' => {
                (x, y) = (lx.number()? + ox, lx.number()? + oy);
                pb.line_to(x, y);
            }
            b'H' => {
                x = lx.number()? + ox;
                pb.line_to(x, y);
            }
            b'V' => {
                y = lx.number()? + oy;
                pb.line_to(x, y);
            }
            b'C' => {
                let (x1, y1) = (lx.number()? + ox, lx.number()? + oy);
                let (x2, y2) = (lx.number()? + ox, lx.number()? + oy);
                (x, y) = (lx.number()? + ox, lx.number()? + oy);
                pb.cubic_to(x1, y1, x2, y2, x, y);
                cc = Some((x2, y2));
            }
            b'S' => {
                let (x1, y1) = last_c.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let (x2, y2) = (lx.number()? + ox, lx.number()? + oy);
                (x, y) = (lx.number()? + ox, lx.number()? + oy);
                pb.cubic_to(x1, y1, x2, y2, x, y);
                cc = Some((x2, y2));
            }
            b'Q' => {
                let (x1, y1) = (lx.number()? + ox, lx.number()? + oy);
                (x, y) = (lx.number()? + ox, lx.number()? + oy);
                pb.quad_to(x1, y1, x, y);
                qc = Some((x1, y1));
            }
            b'T' => {
                let (x1, y1) = last_q.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                (x, y) = (lx.number()? + ox, lx.number()? + oy);
                pb.quad_to(x1, y1, x, y);
                qc = Some((x1, y1));
            }
            b'A' => {
                let (rx, ry, rot) = (lx.number()?, lx.number()?, lx.number()?);
                let (large, sweep) = (lx.flag()?, lx.flag()?);
                let (nx, ny) = (lx.number()? + ox, lx.number()? + oy);
                arc(&mut pb, (x, y), (rx, ry), rot, large, sweep, (nx, ny));
                (x, y) = (nx, ny);
            }
            b'Z' => {
                pb.close();
                (x, y) = (sx, sy);
            }
            _ => return None,
        }
        last_c = cc;
        last_q = qc;
        if !cmd.eq_ignore_ascii_case(&b'Z') && lx.at_number() {
            continue; // repeated arguments for the same command
        }
        match lx.command() {
            Some(c) => cmd = c,
            None => break,
        }
    }
    pb.finish()
}

/// SVG elliptical arc → cubic Béziers (SVG 1.1 implementation notes, F.6).
fn arc(
    pb: &mut PathBuilder,
    (x1, y1): (f32, f32),
    (rx, ry): (f32, f32),
    rot_deg: f32,
    large: bool,
    sweep: bool,
    (x2, y2): (f32, f32),
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || (x1 == x2 && y1 == y2) {
        pb.line_to(x2, y2);
        return;
    }
    let phi = rot_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut co = (num / den).max(0.0).sqrt();
    if large == sweep {
        co = -co;
    }
    let cxp = co * rx * y1p / ry;
    let cyp = -co * ry * x1p / rx;
    let cx = cos * cxp - sin * cyp + (x1 + x2) / 2.0;
    let cy = sin * cxp + cos * cyp + (y1 + y2) / 2.0;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        if a.is_nan() { 0.0 } else { a }
    };
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut dtheta = angle(ux, uy, vx, vy);
    if !sweep && dtheta > 0.0 {
        dtheta -= 2.0 * PI;
    } else if sweep && dtheta < 0.0 {
        dtheta += 2.0 * PI;
    }
    let segs = (dtheta.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let delta = dtheta / segs as f32;
    let t = 4.0 / 3.0 * (delta / 4.0).tan();
    let point = |a: f32| {
        let (s, c) = a.sin_cos();
        (
            cx + rx * c * cos - ry * s * sin,
            cy + rx * c * sin + ry * s * cos,
        )
    };
    let deriv = |a: f32| {
        let (s, c) = a.sin_cos();
        (-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos)
    };
    let mut a = theta1;
    for _ in 0..segs {
        let b = a + delta;
        let (p0, p3) = (point(a), point(b));
        let (d0, d3) = (deriv(a), deriv(b));
        pb.cubic_to(
            p0.0 + t * d0.0,
            p0.1 + t * d0.1,
            p3.0 - t * d3.0,
            p3.1 - t * d3.1,
            p3.0,
            p3.1,
        );
        a = b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shared_icon_parses() {
        for (name, d) in [
            ("divide", appcore::icons::DIVIDE),
            ("backspace", appcore::icons::BACKSPACE),
            ("history", appcore::icons::HISTORY),
            ("standard", appcore::icons::STANDARD),
            ("currency", appcore::icons::CURRENCY),
            ("data", appcore::icons::DATA),
            ("settings", appcore::icons::SETTINGS),
            ("sparkle", appcore::icons::SPARKLE),
            ("copy", appcore::icons::COPY),
        ] {
            let p = parse(d).unwrap_or_else(|| panic!("{name} did not parse"));
            let b = p.bounds();
            assert!(b.left() >= 0.0 && b.right() <= 24.5, "{name}: {b:?}");
            assert!(b.top() >= 0.0 && b.bottom() <= 24.5, "{name}: {b:?}");
        }
    }

    #[test]
    fn numbers_without_separators() {
        // "2.5.5" is 2.5 then .5; "1-2" is 1 then -2.
        let p = parse("M2.5.5L1-2").unwrap();
        assert_eq!(p.points().len(), 2);
        assert_eq!(p.points()[0].x, 2.5);
        assert_eq!(p.points()[0].y, 0.5);
        assert_eq!(p.points()[1].y, -2.0);
    }
}
