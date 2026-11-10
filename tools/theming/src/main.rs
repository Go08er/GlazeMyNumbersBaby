//! Screenshot comparison for the theming acceptance (docs/theming.md, "How it
//! is measured"). `compare D.png G.png [--text D.json] [--exclude x,y,w,h]...
//! [--report out.png]`, or `compare --batch DIR_D DIR_G [...]`. See README.md.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// ---- thresholds, from docs/theming.md --------------------------------------

/// Flat pixels: per-channel tolerance and the share that must match.
const FLAT_TOL: i32 = 2;
const FLAT_MIN: f64 = 99.5;
/// Edges: the search radius (Chebyshev), the tolerance and the share.
const EDGE_RADIUS: i32 = 1;
const EDGE_TOL: i32 = 24;
const EDGE_MIN: f64 = 99.0;
/// Text: box sides within 1 px, mean ink colour within 6, blurred MAD 12.
const BOX_TOL: i32 = 1;
const INK_COLOUR_TOL: f64 = 6.0;
const BLUR_MAD_MAX: f64 = 12.0;
/// 16x16 at each window corner is always excluded.
const CORNER: usize = 16;
/// Where text ink starts to count (difference from the background), the
/// weight floor for the mean colour, and how far past D's box G is searched.
const INK_BBOX_MIN: i32 = 24;
const INK_WEIGHT_MIN: i32 = 6;
const WINDOW_PAD: i32 = 4;
/// Curves: a pixel is the curve's when within this of its colour; the
/// non-flat pixels within this many px of one are curve, not edge.
const CURVE_INK_TOL: i32 = 40;
const CURVE_HALO: i32 = 2;

type Rgb = [u8; 3];

pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
}

impl Img {
    pub fn new(w: usize, h: usize, fill: Rgb) -> Img {
        Img {
            w,
            h,
            px: vec![fill; w * h],
        }
    }
    fn at(&self, x: i32, y: i32) -> Rgb {
        let x = x.clamp(0, self.w as i32 - 1) as usize;
        let y = y.clamp(0, self.h as i32 - 1) as usize;
        self.px[y * self.w + x]
    }
    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }
    pub fn set(&mut self, x: usize, y: usize, c: Rgb) {
        self.px[y * self.w + x] = c;
    }
}

pub fn load(path: &Path) -> Result<Img, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let n = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    };
    let (w, h) = (info.width as usize, info.height as usize);
    let px = buf[..info.buffer_size()]
        .chunks_exact(n)
        .map(|c| if n < 3 { [c[0]; 3] } else { [c[0], c[1], c[2]] })
        .collect();
    Ok(Img { w, h, px })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn grow(&self, n: i32) -> Rect {
        Rect {
            x: self.x - n,
            y: self.y - n,
            w: self.w + 2 * n,
            h: self.h + 2 * n,
        }
    }
    fn clip(&self, w: usize, h: usize) -> Rect {
        let x0 = self.x.max(0);
        let y0 = self.y.max(0);
        let x1 = (self.x + self.w).min(w as i32);
        let y1 = (self.y + self.h).min(h as i32);
        Rect {
            x: x0,
            y: y0,
            w: (x1 - x0).max(0),
            h: (y1 - y0).max(0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub r: Rect,
    pub text: String,
    /// Set for a graph curve: its colour.
    pub curve: Option<Rgb>,
}

pub fn parse_runs(json: &str) -> Result<Vec<Run>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let a = v.as_array().ok_or("text boxes: not an array")?;
    let mut out = Vec::new();
    for b in a {
        let n = |k: &str| {
            b.get(k)
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .ok_or(format!("text box without {k}"))
        };
        let curve = match (b.get("kind").and_then(|k| k.as_str()), b.get("color")) {
            (Some("curve"), Some(c)) => {
                let c: Vec<u8> = c
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_u64().map(|v| v as u8))
                            .collect()
                    })
                    .unwrap_or_default();
                if c.len() != 3 {
                    return Err("curve without a colour".into());
                }
                Some([c[0], c[1], c[2]])
            }
            _ => None,
        };
        out.push(Run {
            r: Rect {
                x: n("x")?,
                y: n("y")?,
                w: n("w")?,
                h: n("h")?,
            },
            text: b
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            curve,
        });
    }
    Ok(out)
}

fn maxdiff(a: Rgb, b: Rgb) -> i32 {
    (0..3)
        .map(|i| (a[i] as i32 - b[i] as i32).abs())
        .max()
        .unwrap_or(0)
}

/// The most common colour on the border of `r` (the text's background).
fn ring_mode(img: &Img, r: Rect) -> Rgb {
    let mut counts: HashMap<Rgb, u32> = HashMap::new();
    let mut add = |x: i32, y: i32| {
        if img.inside(x, y) {
            *counts.entry(img.at(x, y)).or_default() += 1;
        }
    };
    for x in r.x..r.x + r.w {
        add(x, r.y);
        add(x, r.y + r.h - 1);
    }
    for y in r.y..r.y + r.h {
        add(r.x, y);
        add(r.x + r.w - 1, y);
    }
    counts
        .into_iter()
        .max_by_key(|(c, n)| (*n, *c))
        .map_or([0; 3], |(c, _)| c)
}

struct Ink {
    /// x0, y0, x1, y1 (inclusive).
    bbox: Option<[i32; 4]>,
    mean: Option<[f64; 3]>,
}

/// The ink in `win`: for text, whatever differs from the background `bg`
/// (box at a threshold; mean colour weighted by the difference); for a
/// curve, whatever is near its colour.
fn measure(img: &Img, win: Rect, bg: Rgb, curve: Option<Rgb>) -> Ink {
    let mut bbox: Option<[i32; 4]> = None;
    let (mut sum, mut wsum) = ([0.0f64; 3], 0.0f64);
    for y in win.y..win.y + win.h {
        for x in win.x..win.x + win.w {
            let p = img.at(x, y);
            let (in_box, weight) = match curve {
                Some(c) => {
                    let ink = maxdiff(p, c) <= CURVE_INK_TOL;
                    (ink, if ink { 1.0 } else { 0.0 })
                }
                None => {
                    let d = maxdiff(p, bg);
                    (
                        d > INK_BBOX_MIN,
                        if d > INK_WEIGHT_MIN { d as f64 } else { 0.0 },
                    )
                }
            };
            if in_box {
                bbox = Some(match bbox {
                    None => [x, y, x, y],
                    Some([a, b, c, d]) => [a.min(x), b.min(y), c.max(x), d.max(y)],
                });
            }
            if weight > 0.0 {
                for i in 0..3 {
                    sum[i] += weight * p[i] as f64;
                }
                wsum += weight;
            }
        }
    }
    Ink {
        bbox,
        mean: (wsum > 0.0).then(|| [sum[0] / wsum, sum[1] / wsum, sum[2] / wsum]),
    }
}

fn blur(img: &Img, x: i32, y: i32) -> [f64; 3] {
    let mut s = [0.0; 3];
    for dy in -1..=1 {
        for dx in -1..=1 {
            let p = img.at(x + dx, y + dy);
            for i in 0..3 {
                s[i] += p[i] as f64;
            }
        }
    }
    s.map(|v| v / 9.0)
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub run: Run,
    pub failures: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Opts {
    pub exclude: Vec<Rect>,
}

#[derive(Debug)]
pub struct Outcome {
    pub flat_total: u64,
    pub flat_ok: u64,
    pub edge_total: u64,
    pub edge_ok: u64,
    pub runs: Vec<RunResult>,
    pub skipped_runs: usize,
    /// Per pixel of D: 1 failing flat, 2 failing edge.
    pub marks: Vec<u8>,
}

fn pct(ok: u64, total: u64) -> f64 {
    if total == 0 {
        100.0
    } else {
        100.0 * ok as f64 / total as f64
    }
}

impl Outcome {
    pub fn flat_pct(&self) -> f64 {
        pct(self.flat_ok, self.flat_total)
    }
    pub fn edge_pct(&self) -> f64 {
        pct(self.edge_ok, self.edge_total)
    }
    pub fn text_failed(&self) -> usize {
        self.runs.iter().filter(|r| !r.failures.is_empty()).count()
    }
    pub fn flat_pass(&self) -> bool {
        self.flat_pct() >= FLAT_MIN
    }
    pub fn edge_pass(&self) -> bool {
        self.edge_pct() >= EDGE_MIN
    }
    pub fn text_pass(&self) -> bool {
        self.text_failed() == 0
    }
    pub fn pass(&self) -> bool {
        self.flat_pass() && self.edge_pass() && self.text_pass()
    }
}

pub fn compare(d: &Img, g: &Img, runs: &[Run], opts: &Opts) -> Result<Outcome, String> {
    if d.w != g.w || d.h != g.h {
        return Err(format!(
            "size differs: D {}x{}, G {}x{}",
            d.w, d.h, g.w, g.h
        ));
    }
    let (w, h) = (d.w, d.h);
    let mut excl = vec![false; w * h];
    let c = CORNER.min(w).min(h);
    let mut rects: Vec<Rect> = opts.exclude.clone();
    for (cx, cy) in [(0, 0), (w - c, 0), (0, h - c), (w - c, h - c)] {
        rects.push(Rect {
            x: cx as i32,
            y: cy as i32,
            w: c as i32,
            h: c as i32,
        });
    }
    for r in &rects {
        let r = r.clip(w, h);
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                excl[y as usize * w + x as usize] = true;
            }
        }
    }
    let is_excl = |x: i32, y: i32| excl[y as usize * w + x as usize];

    // Runs whose centre is excluded are not compared.
    let mut kept: Vec<&Run> = Vec::new();
    let mut skipped_runs = 0;
    for run in runs {
        let (cx, cy) = (run.r.x + run.r.w / 2, run.r.y + run.r.h / 2);
        let r = run.r.clip(w, h);
        if r.w == 0 || r.h == 0 || !d.inside(cx, cy) || is_excl(cx, cy) {
            skipped_runs += 1;
        } else {
            kept.push(run);
        }
    }

    // Pixels of D that are not edges: inside a text box, or around a curve.
    let mut exempt = vec![false; w * h];
    for run in &kept {
        let r = run.r.clip(w, h);
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                match run.curve {
                    None => exempt[y as usize * w + x as usize] = true,
                    Some(col) if maxdiff(d.at(x, y), col) <= CURVE_INK_TOL => {
                        for dy in -CURVE_HALO..=CURVE_HALO {
                            for dx in -CURVE_HALO..=CURVE_HALO {
                                let (ex, ey) = (x + dx, y + dy);
                                if d.inside(ex, ey) {
                                    exempt[ey as usize * w + ex as usize] = true;
                                }
                            }
                        }
                    }
                    Some(_) => {}
                }
            }
        }
    }

    let mut out = Outcome {
        flat_total: 0,
        flat_ok: 0,
        edge_total: 0,
        edge_ok: 0,
        runs: Vec::new(),
        skipped_runs,
        marks: vec![0; w * h],
    };
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if is_excl(x, y) {
                continue;
            }
            let p = d.at(x, y);
            let flat = x >= 2
                && y >= 2
                && x + 2 < w as i32
                && y + 2 < h as i32
                && (-2..=2).all(|dy| (-2..=2).all(|dx| d.at(x + dx, y + dy) == p));
            let i = y as usize * w + x as usize;
            if flat {
                out.flat_total += 1;
                if maxdiff(p, g.at(x, y)) <= FLAT_TOL {
                    out.flat_ok += 1;
                } else {
                    out.marks[i] = 1;
                }
            } else if !exempt[i] {
                out.edge_total += 1;
                let near = (-EDGE_RADIUS..=EDGE_RADIUS).any(|dy| {
                    (-EDGE_RADIUS..=EDGE_RADIUS).any(|dx| {
                        g.inside(x + dx, y + dy) && maxdiff(p, g.at(x + dx, y + dy)) <= EDGE_TOL
                    })
                });
                if near {
                    out.edge_ok += 1;
                } else {
                    out.marks[i] = 2;
                }
            }
        }
    }

    for run in kept {
        let box_ = run.r.clip(w, h);
        let win = run.r.grow(WINDOW_PAD).clip(w, h);
        let (bgd, bgg) = (ring_mode(d, win), ring_mode(g, win));
        let win = if run.curve.is_some() {
            box_.grow(CURVE_HALO).clip(w, h)
        } else {
            win
        };
        let (id, ig) = (
            measure(d, win, bgd, run.curve),
            measure(g, win, bgg, run.curve),
        );
        let mut failures = Vec::new();
        match (id.bbox, ig.bbox) {
            (Some(a), Some(b)) => {
                let names = ["left", "top", "right", "bottom"];
                for k in 0..4 {
                    if (a[k] - b[k]).abs() > BOX_TOL {
                        failures.push(format!(
                            "ink {} edge D {} G {} (off by {})",
                            names[k],
                            a[k],
                            b[k],
                            b[k] - a[k]
                        ));
                    }
                }
            }
            (Some(_), None) => failures.push("no ink in G".into()),
            (None, Some(_)) => failures.push("ink in G, none in D".into()),
            (None, None) => {}
        }
        if let (Some(a), Some(b)) = (id.mean, ig.mean) {
            let dm = (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max);
            if dm > INK_COLOUR_TOL {
                failures.push(format!("mean ink colour differs by {dm:.1}"));
            }
        }
        let mut sum = 0.0;
        for y in box_.y..box_.y + box_.h {
            for x in box_.x..box_.x + box_.w {
                let (a, b) = (blur(d, x, y), blur(g, x, y));
                sum += (0..3).map(|i| (a[i] - b[i]).abs()).sum::<f64>();
            }
        }
        let mad = sum / (box_.w as f64 * box_.h as f64 * 3.0);
        if mad > BLUR_MAD_MAX {
            failures.push(format!("blurred difference {mad:.1} over {BLUR_MAD_MAX}"));
        }
        out.runs.push(RunResult {
            run: run.clone(),
            failures,
        });
    }
    Ok(out)
}

pub fn write_report(path: &Path, d: &Img, o: &Outcome) -> Result<(), String> {
    let mut rgb: Vec<u8> = Vec::with_capacity(d.w * d.h * 3);
    for (i, p) in d.px.iter().enumerate() {
        let c = match o.marks[i] {
            1 => [255, 0, 0],
            2 => [255, 210, 0],
            _ => [p[0] / 2 + 127, p[1] / 2 + 127, p[2] / 2 + 127],
        };
        rgb.extend_from_slice(&c);
    }
    let mut put = |x: i32, y: i32| {
        if d.inside(x, y) {
            let i = (y as usize * d.w + x as usize) * 3;
            rgb[i..i + 3].copy_from_slice(&[230, 0, 230]);
        }
    };
    for r in o.runs.iter().filter(|r| !r.failures.is_empty()) {
        let b = r.run.r.grow(1);
        for x in b.x..b.x + b.w {
            put(x, b.y);
            put(x, b.y + b.h - 1);
        }
        for y in b.y..b.y + b.h {
            put(b.x, y);
            put(b.x + b.w - 1, y);
        }
    }
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), d.w as u32, d.h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgb))
        .map_err(|e| e.to_string())
}

// ---- command line ------------------------------------------------------------

fn parse_rect(s: &str) -> Result<Rect, String> {
    let v: Vec<i32> = s
        .split(',')
        .map(|t| t.trim().parse::<i32>().map_err(|e| format!("{s}: {e}")))
        .collect::<Result<_, _>>()?;
    match v[..] {
        [x, y, w, h] => Ok(Rect { x, y, w, h }),
        _ => Err(format!("{s}: expected x,y,w,h")),
    }
}

fn verdict(b: bool) -> &'static str {
    if b { "PASS" } else { "FAIL" }
}

fn print_single(o: &Outcome) {
    println!(
        "flat pixels: {:.3}% of {} match (need >= {FLAT_MIN}%)  {}",
        o.flat_pct(),
        o.flat_total,
        verdict(o.flat_pass())
    );
    println!(
        "edges:       {:.3}% of {} match (need >= {EDGE_MIN}%)  {}",
        o.edge_pct(),
        o.edge_total,
        verdict(o.edge_pass())
    );
    println!(
        "text:        {} of {} runs match, every run must ({} skipped as excluded)  {}",
        o.runs.len() - o.text_failed(),
        o.runs.len(),
        o.skipped_runs,
        verdict(o.text_pass())
    );
    for r in o.runs.iter().filter(|r| !r.failures.is_empty()).take(20) {
        println!(
            "  run {:?} at {},{} {}x{}: {}",
            r.run.text,
            r.run.r.x,
            r.run.r.y,
            r.run.r.w,
            r.run.r.h,
            r.failures.join("; ")
        );
    }
    if o.text_failed() > 20 {
        println!("  ... and {} more", o.text_failed() - 20);
    }
}

fn run_pair(
    dp: &Path,
    gp: &Path,
    text: Option<&Path>,
    opts: &Opts,
    report: Option<&Path>,
) -> Result<Outcome, String> {
    let (d, g) = (load(dp)?, load(gp)?);
    let runs = match text {
        Some(t) => {
            parse_runs(&std::fs::read_to_string(t).map_err(|e| format!("{}: {e}", t.display()))?)
                .map_err(|e| format!("{}: {e}", t.display()))?
        }
        None => Vec::new(),
    };
    let o = compare(&d, &g, &runs, opts)?;
    if let Some(r) = report {
        write_report(r, &d, &o)?;
    }
    Ok(o)
}

const USAGE: &str = "usage:
  compare D.png G.png [--text D.json] [--exclude x,y,w,h]... [--report out.png]
  compare --batch DIR_D DIR_G [--map FILE] [--d-prefix dgmnb] [--g-prefix gmnb]
          [--skip SUBSTRING]... [--exclude x,y,w,h]... [--report-dir DIR]
D.json defaults, in batch, to the file next to D's picture with .json.
A map file has lines `D-name G-name [exclude=x,y,w,h]...` (names without .png).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match real_main(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("compare: {e}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

type Pair = (String, String, Vec<Rect>);

fn read_map(m: &Path) -> Result<Vec<Pair>, String> {
    let s = std::fs::read_to_string(m).map_err(|e| format!("{}: {e}", m.display()))?;
    let mut pairs = Vec::new();
    for l in s
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let mut f = l.split_whitespace();
        let (Some(a), Some(b)) = (f.next(), f.next()) else {
            return Err(format!("map line {l:?}: two names expected"));
        };
        let mut ex = Vec::new();
        for x in f {
            let r = x
                .strip_prefix("exclude=")
                .ok_or(format!("map line {l:?}: unknown field {x}"))?;
            ex.push(parse_rect(r)?);
        }
        pairs.push((
            a.trim_end_matches(".png").into(),
            b.trim_end_matches(".png").into(),
            ex,
        ));
    }
    Ok(pairs)
}

fn real_main(args: &[String]) -> Result<bool, String> {
    let (mut pos, mut text, mut report) = (Vec::new(), None, None);
    let mut opts = Opts::default();
    let (mut batch, mut map, mut report_dir) = (false, None, None);
    let (mut dpre, mut gpre) = ("dgmnb".to_string(), "gmnb".to_string());
    let mut skip: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |n: &str| it.next().cloned().ok_or(format!("{n} needs a value"));
        match a.as_str() {
            "--text" => text = Some(PathBuf::from(val("--text")?)),
            "--exclude" => opts.exclude.push(parse_rect(&val("--exclude")?)?),
            "--report" => report = Some(PathBuf::from(val("--report")?)),
            "--batch" => batch = true,
            "--map" => map = Some(PathBuf::from(val("--map")?)),
            "--report-dir" => report_dir = Some(PathBuf::from(val("--report-dir")?)),
            "--d-prefix" => dpre = val("--d-prefix")?,
            "--g-prefix" => gpre = val("--g-prefix")?,
            "--skip" => skip.push(val("--skip")?),
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            _ => pos.push(a.clone()),
        }
    }
    if !batch {
        let [d, g] = &pos[..] else {
            return Err("two pictures expected".into());
        };
        let o = run_pair(
            Path::new(d),
            Path::new(g),
            text.as_deref(),
            &opts,
            report.as_deref(),
        )?;
        print_single(&o);
        println!("{}", verdict(o.pass()));
        return Ok(o.pass());
    }
    let [dd, gd] = &pos[..] else {
        return Err("--batch DIR_D DIR_G expected".into());
    };
    let (dd, gd) = (PathBuf::from(dd), PathBuf::from(gd));
    let pairs: Vec<Pair> = if let Some(m) = map {
        read_map(&m)?
    } else {
        let mut names: Vec<String> = std::fs::read_dir(&dd)
            .map_err(|e| format!("{}: {e}", dd.display()))?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| {
                n.strip_prefix(&format!("{dpre}-"))?
                    .strip_suffix(".png")
                    .map(String::from)
            })
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|c| (format!("{dpre}-{c}"), format!("{gpre}-{c}"), Vec::new()))
            .collect()
    };
    if let Some(r) = &report_dir {
        std::fs::create_dir_all(r).map_err(|e| e.to_string())?;
    }
    println!(
        "{:<34} {:>8} {:>8} {:>10}  verdict",
        "case", "flat %", "edge %", "text bad"
    );
    let (mut all, mut n_fail) = (true, 0);
    for (dn, gn, ex) in &pairs {
        if skip.iter().any(|s| dn.contains(s.as_str())) {
            println!("{dn:<34} skipped");
            continue;
        }
        let mut o = Opts {
            exclude: opts.exclude.clone(),
        };
        o.exclude.extend(ex.iter().copied());
        let tj = dd.join(format!("{dn}.json"));
        let rp = report_dir.as_ref().map(|r| r.join(format!("{dn}.png")));
        let res = run_pair(
            &dd.join(format!("{dn}.png")),
            &gd.join(format!("{gn}.png")),
            tj.exists().then_some(tj.as_path()),
            &o,
            rp.as_deref(),
        );
        match res {
            Ok(o) => {
                println!(
                    "{dn:<34} {:>8.3} {:>8.3} {:>4}/{:<5}  {}",
                    o.flat_pct(),
                    o.edge_pct(),
                    o.text_failed(),
                    o.runs.len(),
                    verdict(o.pass())
                );
                if !o.pass() {
                    all = false;
                    n_fail += 1;
                }
            }
            Err(e) => {
                println!("{dn:<34} ERROR {e}");
                all = false;
                n_fail += 1;
            }
        }
    }
    println!(
        "{} pairs, {} failing (flat >= {FLAT_MIN}%, edges >= {EDGE_MIN}%, every text run)",
        pairs.len(),
        n_fail
    );
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: Rgb = [240, 240, 240];
    const FG: Rgb = [20, 40, 160];

    fn rect_img(x0: usize, x1: usize) -> Img {
        let mut i = Img::new(100, 100, BG);
        for y in 30..70 {
            for x in x0..x1 {
                i.set(x, y, FG);
            }
        }
        i
    }

    /// Stripes standing in for glyphs, inside a 22x10 box at (30,45).
    fn text_img(ink: Rgb) -> Img {
        let mut i = Img::new(100, 100, BG);
        for y in 45..55 {
            for x in 30..52 {
                if x % 3 == 0 {
                    i.set(x, y, ink);
                }
            }
        }
        i
    }

    fn run() -> Run {
        Run {
            r: Rect {
                x: 30,
                y: 45,
                w: 22,
                h: 10,
            },
            text: "x".into(),
            curve: None,
        }
    }

    #[test]
    fn identical_passes() {
        let o = compare(&rect_img(20, 80), &rect_img(20, 80), &[], &Opts::default()).unwrap();
        assert!(o.pass(), "{} {}", o.flat_pct(), o.edge_pct());
        let o = compare(
            &text_img([0, 0, 0]),
            &text_img([0, 0, 0]),
            &[run()],
            &Opts::default(),
        )
        .unwrap();
        assert!(o.pass() && o.runs.len() == 1);
    }

    #[test]
    fn three_px_shift_fails_flat() {
        let o = compare(&rect_img(20, 80), &rect_img(23, 83), &[], &Opts::default()).unwrap();
        assert!(!o.flat_pass(), "{}", o.flat_pct());
    }

    #[test]
    fn one_px_border_shift_passes() {
        let o = compare(&rect_img(20, 80), &rect_img(21, 81), &[], &Opts::default()).unwrap();
        assert!(
            o.flat_pass() && o.edge_pass(),
            "{} {}",
            o.flat_pct(),
            o.edge_pct()
        );
    }

    #[test]
    fn recoloured_text_fails_text() {
        let o = compare(
            &text_img([0, 0, 0]),
            &text_img([60, 0, 0]),
            &[run()],
            &Opts::default(),
        )
        .unwrap();
        assert!(o.flat_pass() && o.edge_pass());
        assert!(!o.text_pass());
    }

    #[test]
    fn size_mismatch_is_an_error() {
        let (a, b) = (Img::new(10, 10, BG), Img::new(11, 10, BG));
        assert!(compare(&a, &b, &[], &Opts::default()).is_err());
    }

    #[test]
    fn excluded_corner_is_ignored() {
        let a = Img::new(100, 100, BG);
        let mut b = Img::new(100, 100, BG);
        for y in 0..10 {
            for x in 0..10 {
                b.set(x, y, [0, 0, 0]);
            }
        }
        assert!(compare(&a, &b, &[], &Opts::default()).unwrap().pass());
    }

    #[test]
    fn parses_boxes() {
        let r = parse_runs(
            r#"[{"x":1,"y":2,"w":3,"h":4,"text":"a"},{"x":0,"y":0,"w":9,"h":9,"text":"curve","kind":"curve","color":[1,2,3]}]"#,
        )
        .unwrap();
        assert_eq!(
            r[0].r,
            Rect {
                x: 1,
                y: 2,
                w: 3,
                h: 4
            }
        );
        assert_eq!(r[1].curve, Some([1, 2, 3]));
    }
}
