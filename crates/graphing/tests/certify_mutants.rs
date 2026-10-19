//! A mutation tester of the certified analysis' certificates.
//!
//! The replay (`tests/replay/`) checks each claim of a certificate, and
//! then each row against its claims. A row is a statement of its own (a
//! list of zeros, the pieces f is monotone on, a range, a period), and a
//! checker that looks at the premises without deriving the statement lets
//! a row through that its claims don't prove: every claim true, the row
//! false (review 19, R19-M-02: sin's monotone pieces removed from a
//! complete list; x² "increasing on the line" in a partial row with no
//! claims; a piece closed at a hole). Plants find such a hole one at a
//! time, by whoever thinks of it. This goes through them by construction:
//! for each genuine certificate, for each row, every operator below that
//! changes what the row says, and the strict replay must refuse each
//! result.
//!
//! Two kinds of hole in the checker have each come back in a new shape,
//! and have operators of their own (review 20): two enclosures meeting
//! taken for one number (an exact limit written as a rational within
//! 10⁻⁶³ of it; two zeros or poles listed as one item), and an obligation
//! let go because a place was beside, about or near another (a pole or
//! zero dropped with every claim about it, a gap in their place; pieces
//! joined over the place between them; a value f takes split out of the
//! range as a hole's).
//!
//! A mutant is **killed** when the replay refuses the mutated row itself
//! (a claim of it refuted or left open, or the row not following from its
//! claims), in each of four settings: the certificate as it is, with
//! every other row withheld but the domain, with every other row withheld,
//! and with every claim of the genuine certificate given to the row as its
//! own. (A row that is refused only for what another row says would pass
//! once that row is left out, which a certificate is free to do; and a row
//! that is refused only for want of a claim would pass with it, true as
//! every claim of the genuine certificate is.) A
//! mutant that is not killed is **accepted**: a finding, unless the
//! operator is content-preserving for that row, which each operator
//! decides and says why ([`Mutant::benign`]). Those are run too and
//! counted apart; none is asked to be refused.
//!
//! ```text
//! cargo test -p graphing --features mpfr-oracle --test certify_mutants       # a fixed few, seconds
//! cargo test --release -p graphing --features mpfr-oracle --test certify_mutants -- \
//!     --include-ignored --nocapture                                           # the corpus and the pool
//! ```
//!
//! The long runs assert more than an empty accepted list. A few mutants are
//! accepted because the mutated row is a true statement (a row borrowed
//! from another function that happens to hold; a proven enclosure narrowed
//! onto the number it holds): each is reviewed by hand in [`KNOWN`], with
//! the reason it is true. A long run fails on an accepted mutant that is not
//! there, and on an entry there that it did not accept (stale), so that the
//! table can neither miss a new finding nor keep a fixed one. (With
//! `MUTANTS_FILTER`, `_ROW`, `_SKIP` or `_ITEMS` set, only the first holds.)
//! The fixed few have no table: none is accepted. An end of a partial
//! monotone piece moved strictly inward is content-preserving by
//! construction ([`DRAWN_IN`]), decided from the mutation's numbers alone.
//!
//! `MUTANTS_FILTER=text` keeps the functions whose source contains `text`;
//! `MUTANTS_LOG=file` writes every accepted mutant, tabled or not (tab
//! separated: source, row, operator, what, setting) to `file.<run>`
//! (`file.corpus`, `file.pool`, `file.a_fixed_few`), one file per test, so
//! that tests run together don't truncate each other's; `MUTANTS_ROW=name`
//! keeps one row's;
//! `MUTANTS_PROGRESS=1` says each certificate as it is done; `MUTANTS_SKIP=a;b`
//! leaves those sources out (and says so); `MUTANTS_SLOW=seconds` names
//! each mutant replay that takes longer.
//!
//! A list of more than `MUTANTS_ITEMS` items (16 unless set; `all`: no
//! limit) has the operators that go item by item (an item removed, each
//! number, end and kind of an item changed) applied to its first and last
//! eight: a partial list of two hundred zeros of sin x² is two hundred
//! items the certifier found one way and the replay checks one way, and
//! every one of them mutated is some 2,500 mutants of one certificate. The
//! operators on the list as a whole, its status, scope and claims are
//! applied whatever its length.
#![cfg(feature = "mpfr-oracle")]

mod certificates;
mod replay;

use graphing::certify::{DEFAULT_BUDGET, certify_text};
use graphing::compile::CompileOptions;
use graphing::functions::TrigUnit;
use serde_json::{Value, json};
use std::collections::BTreeMap;

// ------------------------------------------------------------ certificates

fn certificate(src: &str, unit: TrigUnit) -> Option<Value> {
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    let a = certify_text(src, opts, DEFAULT_BUDGET, None).ok()?;
    serde_json::from_str(&serde_json::to_string(&a).ok()?).ok()
}

/// A double as the certificate writes it.
fn num(d: f64) -> Value {
    if d == f64::INFINITY {
        json!("inf")
    } else if d == f64::NEG_INFINITY {
        json!("-inf")
    } else {
        json!(d)
    }
}

fn enc(lo: f64, hi: f64) -> Value {
    json!({"lo": num(lo), "hi": num(hi)})
}

/// An enclosure `{lo, hi}`.
fn enc_of(v: &Value) -> Option<(f64, f64)> {
    let o = v.as_object()?;
    if o.len() != 2 {
        return None;
    }
    Some((replay::r(o.get("lo")?).ok()?, replay::r(o.get("hi")?).ok()?))
}

/// The row's status and body (`value`, `cert`), unless it is unknown.
fn body<'v>(v: &'v Value, row: &str) -> Option<(&'v str, &'v Value)> {
    let (status, b) = v.get(row)?.as_object()?.iter().next()?;
    (status != "Unknown").then_some((status.as_str(), b))
}

fn body_mut<'v>(v: &'v mut Value, row: &str) -> &'v mut Value {
    v[row]
        .as_object_mut()
        .expect("a row")
        .values_mut()
        .next()
        .expect("a row's body")
}

/// Every row but those of `keep` withheld (unknown).
fn withhold_all_but(v: &mut Value, keep: &[&str]) {
    for row in replay::ROWS {
        if !keep.contains(&row) {
            v[row] = json!({"Unknown": {"reason": "withheld"}});
        }
    }
}

// ------------------------------------------------------------ paths

#[derive(Clone, Debug, PartialEq)]
enum Seg {
    K(String),
    I(usize),
}

fn visit(v: &Value, path: &mut Vec<Seg>, f: &mut dyn FnMut(&[Seg], &Value)) {
    f(path, v);
    match v {
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                path.push(Seg::I(i));
                visit(x, path, f);
                path.pop();
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                path.push(Seg::K(k.clone()));
                visit(x, path, f);
                path.pop();
            }
        }
        _ => {}
    }
}

fn at<'v>(v: &'v mut Value, path: &[Seg]) -> &'v mut Value {
    let mut cur = v;
    for s in path {
        cur = match s {
            Seg::K(k) => &mut cur[k.as_str()],
            Seg::I(i) => &mut cur[*i],
        };
    }
    cur
}

fn show(path: &[Seg]) -> String {
    let mut s = String::new();
    for p in path {
        match p {
            Seg::K(k) => {
                s.push('.');
                s.push_str(k);
            }
            Seg::I(i) => s.push_str(&format!("[{i}]")),
        }
    }
    if s.is_empty() { "value".into() } else { s }
}

/// The paths in `v` of the nodes `is` holds of.
fn paths(v: &Value, is: &dyn Fn(&Value) -> bool) -> Vec<Vec<Seg>> {
    let mut out = Vec::new();
    visit(v, &mut Vec::new(), &mut |p, n| {
        if is(n) {
            out.push(p.to_vec());
        }
    });
    out
}

/// How many items of a long list the item-by-item operators take
/// (`MUTANTS_ITEMS`): None, all of them.
fn item_cap() -> Option<usize> {
    match std::env::var("MUTANTS_ITEMS") {
        Ok(v) if v == "all" => None,
        Ok(v) => Some(v.parse().expect("MUTANTS_ITEMS: a number, or all")),
        Err(_) => Some(16),
    }
}

/// Whether item `i` of a list of `n` is one the item-by-item operators
/// take: all of a list within the cap, the first and last half-caps of a
/// longer one.
fn taken(i: usize, n: usize) -> bool {
    item_cap().is_none_or(|cap| n <= cap || i < cap / 2 || i >= n - cap / 2)
}

/// [`paths`], but through items the item-by-item operators take.
fn sampled_paths(v: &Value, is: &dyn Fn(&Value) -> bool) -> Vec<Vec<Seg>> {
    paths(v, is)
        .into_iter()
        .filter(|p| {
            let mut cur = v;
            p.iter().all(|seg| match seg {
                Seg::K(k) => {
                    cur = &cur[k.as_str()];
                    true
                }
                Seg::I(i) => {
                    let n = cur.as_array().map_or(0, Vec::len);
                    cur = &cur[*i];
                    taken(*i, n)
                }
            })
        })
        .collect()
}

/// A piece's end `{"At": {"x": enclosure, "closed": bool}}`.
fn is_end(v: &Value) -> bool {
    v.get("At").is_some_and(|a| a.get("closed").is_some())
}

/// Whether replacing the enclosure `old` at `p` in `value` (a monotonicity
/// row's value) by `new` draws a piece's end strictly inward: `p` is
/// `[i].on.lo.At.x` or `[i].on.hi.At.x`, the new enclosure lies wholly on
/// the piece's side of the old one (past every number the old one holds),
/// and still wholly short of the piece's other end, so that the piece is
/// non-empty. Decided from the numbers alone, not from the replay.
fn drawn_in(value: &Value, p: &[Seg], old: (f64, f64), new: (f64, f64)) -> bool {
    let n = p.len();
    let [.., Seg::K(on), Seg::K(which), Seg::K(at), Seg::K(x)] = p else {
        return false;
    };
    if on != "on" || at != "At" || x != "x" {
        return false;
    }
    let mut cur = value;
    for s in &p[..n - 3] {
        cur = match s {
            Seg::K(k) => &cur[k.as_str()],
            Seg::I(i) => &cur[*i],
        };
    }
    match which.as_str() {
        // The low end up: past the old enclosure, short of the high end.
        "lo" => {
            new.0 >= old.1
                && match &cur["hi"] {
                    Value::String(s) => s == "PosInf",
                    h => enc_of(&h["At"]["x"]).is_some_and(|(hl, _)| new.1 < hl),
                }
        }
        // The high end down.
        "hi" => {
            new.1 <= old.0
                && match &cur["lo"] {
                    Value::String(s) => s == "NegInf",
                    l => enc_of(&l["At"]["x"]).is_some_and(|(_, lh)| lh < new.0),
                }
        }
        _ => false,
    }
}

// ------------------------------------------------------------ mutants

struct Mutant {
    op: &'static str,
    what: String,
    v: Value,
    /// Why the mutated row still says nothing but what is proven, where
    /// the operator is content-preserving for this row: such a mutant is
    /// not asked to be refused.
    benign: Option<&'static str>,
}

const DIRECTIONS: [&str; 3] = ["Increasing", "Decreasing", "Constant"];
const KINDS: [&str; 2] = ["Min", "Max"];
const SIDES: [&str; 2] = ["Left", "Right"];
const PARITIES: [&str; 3] = ["Even", "Odd", "Neither"];

const FEWER: &str = "a partial row lists what is proven, and says nothing of the rest: fewer items \
                     are still all proven";
const WIDER: &str = "an enclosure widened still holds the number its claims place in the narrower \
                     one";
const WEAKER_STATUS: &str = "a certified row is complete; called partial it claims its items \
                             alone, which are proven";
const SMALLER_SCOPE: &str = "complete over less: the same items, proven, and nothing said of what \
                             the smaller region leaves out";
const OPEN_END: &str = "f is monotone on a piece with an end left out as on the piece with it";
const CUT_SHORTER: &str = "a monotone piece cut shorter at an end is monotone still, and the \
                           piece beside it starts within the turn's enclosure as before";
const DRAWN_IN: &str = "an end of a piece of a partial monotone row moved strictly inward, the piece \
                        left non-empty: a sub-piece of a piece f is monotone on is one it is \
                        monotone on, and a partial row says nothing of the rest";
const ON_F_ITSELF: &str = "the row is checked on f itself (each piece of the domain and each of \
                           its ends; f at 0): it rests on no claim, and stands without them";
const BY_ANOTHER_ROW: &str = "not periodic follows from another row alone (a domain no shift \
                              maps onto itself, a monotone piece reaching ±∞): the row's own \
                              claims are not what it rests on";
const WINDOW_AS_PROVEN: &str = "a partial list called complete over a window: true just when \
                                every item in the window is listed, and the replay asks that of \
                                every item its claims place there, over a window they cover";
const NO_LINE_BESIDE_A_LEVEL: &str = "no oblique asymptote where the horizontal row lists one on \
                                      each side: f − (m·x + b) → 0 with m ≠ 0 and f → c can't \
                                      both be, so the row rests on that row, not its own claims";
const SAME_BY_PERIOD: &str = "a row complete over one period speaks for the line by f's period: \
                              over another window at least a period wide it says the same";
const UNPLACED: &str = "a kink's point left unplaced: some point of the box, a weaker claim, \
                        true of the one point it was";
const JOINED_DROPPED: &str = "f′ continuous across the kink left unsaid: a weaker claim, true \
                              still (an inflection that rested on it is no longer derived)";
const ONE_OF_TWO: &str = "a partial row lists what is proven: an enclosure holding two of its \
                          numbers holds the one it was listed for, and says nothing of the other";
const NO_EXACT_FORM: &str = "the exact form is how the enclosure reads, not a narrower claim: \
                             without it the enclosure stands";

/// An item of a list row made up at `x` (nothing in the certificate
/// proves it).
fn made_up_item(row: &str, x: f64) -> Option<Value> {
    let p = enc(x, x);
    Some(match row {
        "x_intercepts" | "vertical" => json!({"At": p}),
        "extrema" => json!({"x": p, "y": enc(7.0, 7.0), "kind": "Min", "every": null}),
        "inflections" => json!({"x": p, "y": enc(7.0, 7.0), "every": null}),
        "monotonicity" => json!({"on": {
            "lo": {"At": {"x": p, "closed": true}},
            "hi": {"At": {"x": enc(x + 0.25, x + 0.25), "closed": true}}}, "dir": "Increasing"}),
        "range" => json!({
            "lo": {"At": {"x": p, "closed": true}},
            "hi": {"At": {"x": enc(x + 0.25, x + 0.25), "closed": true}}}),
        "horizontal" => json!({"side": "Right", "y": p, "exact": null, "looks_like": null}),
        "oblique" => json!({"side": "Right", "m": enc(1.0, 1.0), "b": p}),
        _ => return None,
    })
}

/// Where f has a pole or a hole, by the domain row: the first excluded
/// family's member, and the pieces' open ends at numbers.
fn poles_and_holes(v: &Value) -> Vec<f64> {
    let mut out = Vec::new();
    let Some((_, d)) = body(v, "domain") else {
        return out;
    };
    for f in d["value"]["excluded"].as_array().into_iter().flatten() {
        if let Some((lo, hi)) = enc_of(&f["x0"]) {
            out.push(lo / 2.0 + hi / 2.0);
        }
    }
    for p in paths(&d["value"]["pieces"], &is_end) {
        let mut pieces = d["value"]["pieces"].clone();
        let e = at(&mut pieces, &p).clone();
        if e["At"]["closed"] == json!(false)
            && let Some((lo, hi)) = enc_of(&e["At"]["x"])
        {
            out.push(lo / 2.0 + hi / 2.0);
        }
    }
    out.dedup();
    out.truncate(2);
    out
}

/// The row's value with every enclosure in it moved by `by`.
fn shifted(v: &Value, by: f64) -> Value {
    let mut out = v.clone();
    for p in paths(v, &|n| enc_of(n).is_some()) {
        let (lo, hi) = enc_of(at(&mut out, &p)).expect("an enclosure");
        *at(&mut out, &p) = enc(lo + by, hi + by);
    }
    out
}

/// The exact number a limit's exact form writes (`q`, `q·π`, `q·π^k`,
/// `π`, `-π`, q a whole number or `n/d`): (q, k). Read here on its own.
fn exact_parts(text: &str) -> Option<(rug::Rational, i32)> {
    let t = text.trim();
    let (q, k) = match t {
        "π" => ("1", 1),
        "-π" => ("-1", 1),
        _ => match t.split_once('·') {
            Some((q, "π")) => (q, 1),
            Some((q, p)) => (q, p.strip_prefix("π^")?.parse().ok()?),
            None => (t, 0),
        },
    };
    let whole = |s: &str| rug::Integer::from_str_radix(s, 10).ok();
    let q = match q.split_once('/') {
        Some((n, d)) => {
            let d = whole(d)?;
            if d == 0 {
                return None;
            }
            rug::Rational::from((whole(n)?, d))
        }
        None => rug::Rational::from(whole(q)?),
    };
    Some((q, k))
}

/// q·πᵏ written as the certifier writes it.
fn exact_text(q: &rug::Rational, k: i32) -> String {
    match k {
        0 => q.to_string(),
        1 => format!("{q}·π"),
        _ => format!("{q}·π^{k}"),
    }
}

/// Exact forms beside `text`'s number, each another number whose 160-bit
/// enclosure still meets the limit's: its coefficient moved by 10⁻⁶³ of
/// itself (by 10⁻⁶³ for 0), and for a power of π the rational within
/// 10⁻⁶³ of it (π/2 written 1570796…296/10⁶³: review 20, R20-M-02).
fn nearby_exact(text: &str) -> Vec<(&'static str, String)> {
    let Some((q, k)) = exact_parts(text) else {
        return Vec::new();
    };
    let ten63 = rug::Integer::from(rug::Integer::u_pow_u(10, 63));
    let tiny = rug::Rational::from((rug::Integer::from(1), ten63.clone()));
    let nudged = if q == 0 {
        tiny.clone()
    } else {
        &q * (rug::Rational::from(1) + &tiny)
    };
    let mut out = vec![("its coefficient moved by 10⁻⁶³", exact_text(&nudged, k))];
    if k != 0 && q != 0 {
        let pi = rug::Float::with_val(1024, rug::float::Constant::Pi);
        let mut v = rug::Float::with_val(1024, &q);
        for _ in 0..k.unsigned_abs() {
            if k > 0 {
                v *= &pi;
            } else {
                v /= &pi;
            }
        }
        v *= rug::Float::with_val(1024, &ten63);
        if let Some(n) = v.round().to_integer() {
            out.push((
                "a rational within 10⁻⁶³",
                exact_text(&rug::Rational::from((n, ten63)), 0),
            ));
        }
    }
    out
}

/// x moved n doubles up (or down).
fn doubles_out(mut x: f64, n: usize, up: bool) -> f64 {
    for _ in 0..n {
        x = if up { x.next_up() } else { x.next_down() };
    }
    x
}

/// What a claim places, where it is a place: an excluded point's box
/// (`at`), a box of x (`x`), the point a one-sided limit is at; and whether
/// the box is one the row's coverage counts (where f is decided, a gap).
fn claim_spot(c: &Value) -> Option<((f64, f64), bool)> {
    let (kind, b) = c.as_object()?.iter().next()?;
    let bx = |v: &Value| Some((replay::r(v.get("a")?).ok()?, replay::r(v.get("b")?).ok()?));
    match kind.as_str() {
        "Unbounded" | "Bounded" | "Removable" => Some((bx(b.get("at")?)?, false)),
        "Undefined" | "Gap" | "Value" | "Beyond" | "NoCross" | "Factors" | "OneCross"
        | "ExactAt" | "Touch" | "KinkSides" | "KinkAt" => Some((bx(b.get("x")?)?, true)),
        "GapClear" | "Defined" | "Continuous" | "Kink" => Some((bx(b.get("x")?)?, false)),
        "Limit" => {
            let at = b.get("at")?;
            let p = at.get("Right").or_else(|| at.get("Left"))?;
            let p = replay::r(p).ok()?;
            Some(((p, p), false))
        }
        _ => None,
    }
}

/// The row's claims with those about the places `spots` (within 16 doubles
/// of one) taken out, and a gap where they were (the place itself and the
/// boxes taken out that the row's coverage counted): what a certificate
/// that said nothing of those places but "a gap" would hold. None where the
/// gap would be over 64 doubles wide (no gap claim stands so) or nothing
/// is taken out.
fn gapped_claims(
    claims: &[Value],
    spots: &[(f64, f64)],
    clear: Option<&str>,
) -> Option<Vec<Value>> {
    let near = |s: (f64, f64)| {
        spots
            .iter()
            .any(|e| doubles_out(e.0, 16, false) <= s.0 && s.1 <= doubles_out(e.1, 16, true))
    };
    let mut kept = Vec::new();
    let mut hulls: Vec<(f64, f64)> = spots.to_vec();
    let mut taken = 0;
    for c in claims {
        match claim_spot(c) {
            Some((s, covers)) if near(s) => {
                taken += 1;
                if covers {
                    // (The hull it falls in.)
                    if let Some(h) = hulls.iter_mut().find(|h| {
                        doubles_out(h.0, 16, false) <= s.0 && s.1 <= doubles_out(h.1, 16, true)
                    }) {
                        h.0 = h.0.min(s.0);
                        h.1 = h.1.max(s.1);
                    }
                }
            }
            _ => kept.push(c.clone()),
        }
    }
    if taken == 0 {
        return None;
    }
    for (a, b) in hulls {
        if !(a.is_finite() && b.is_finite()) || doubles_out(a, 64, true) < b {
            return None;
        }
        kept.push(json!({"Gap": {"x": {"a": num(a), "b": num(b)}}}));
        if let Some(of) = clear {
            kept.push(json!({"GapClear": {"x": {"a": num(a), "b": num(b)}, "of": {"F": of}}}));
        }
    }
    Some(kept)
}

/// The members of the family `x0 + k·period` (enclosures, a few doubles
/// wider each way) inside the window [a, b], at most eight.
fn members_in(x0: (f64, f64), period: (f64, f64), (a, b): (f64, f64)) -> Vec<(f64, f64)> {
    if !(period.0 > 0.0 && a.is_finite() && b.is_finite()) {
        return Vec::new();
    }
    let k0 = ((a - x0.1) / period.0).floor() as i64;
    let k1 = ((b - x0.0) / period.0).ceil() as i64;
    if k1 - k0 > 8 {
        return Vec::new();
    }
    (k0..=k1)
        .map(|k| {
            let k = k as f64;
            let (lo, hi) = if k >= 0.0 {
                (x0.0 + k * period.0, x0.1 + k * period.1)
            } else {
                (x0.0 + k * period.1, x0.1 + k * period.0)
            };
            (doubles_out(lo, 4, false), doubles_out(hi, 4, true))
        })
        .filter(|m| m.1 >= a && m.0 <= b)
        .collect()
}

/// Every mutant of the row `row` of the certificate `base`; `other`: a
/// certificate of another function, for its row.
fn mutants_of(
    base: &Value,
    row: &'static str,
    other: Option<&Value>,
    sink: &mut dyn FnMut(Mutant),
) {
    let Some((status, b)) = body(base, row) else {
        return;
    };
    let certified = status == "Certified";
    let value = b["value"].clone();
    let covers = b["cert"]["covers"].clone();
    // Partial/Window still promises every feature inside that window.
    // Only Partial/Points may discard items or shorten pieces freely.
    let points_only = !certified && covers == json!("Points");
    let mut push = |op: &'static str,
                    what: String,
                    benign: Option<&'static str>,
                    edit: &dyn Fn(&mut Value)| {
        let mut v = base.clone();
        edit(&mut v);
        if v != *base {
            sink(Mutant {
                op,
                what,
                v,
                benign,
            });
        }
    };
    let set = |path: &[Seg], new: Value| {
        let path = path.to_vec();
        move |v: &mut Value| *at(&mut body_mut(v, row)["value"], &path) = new.clone()
    };

    // ---- lists: emptied, an item removed, an item added
    for p in paths(&value, &Value::is_array) {
        let mut vv = value.clone();
        let items = at(&mut vv, &p).as_array().cloned().unwrap_or_default();
        let fewer = points_only.then_some(FEWER);
        // (The range's ends name their sources pair by pair, beside the
        // row: they go with the pieces.)
        let listed = |items: Vec<Value>, ends: Option<Value>| {
            let p = p.clone();
            move |v: &mut Value| {
                *at(&mut body_mut(v, row)["value"], &p) = Value::Array(items.clone());
                if let Some(e) = &ends {
                    v["range_ends"] = e.clone();
                }
            }
        };
        let ends = (row == "range" && p.is_empty())
            .then(|| base["range_ends"].as_array().cloned())
            .flatten();
        if !items.is_empty() {
            push(
                "list emptied",
                format!("{}: its {} items removed", show(&p), items.len()),
                fewer,
                &listed(Vec::new(), ends.as_ref().map(|_| json!([]))),
            );
        }
        if items.len() > 1 {
            for i in (0..items.len()).filter(|i| taken(*i, items.len())) {
                let mut rest = items.clone();
                rest.remove(i);
                let ends = ends.clone().map(|mut e| {
                    if i < e.len() {
                        e.remove(i);
                    }
                    Value::Array(e)
                });
                push(
                    "item removed",
                    format!("{}: item {i} of {} removed", show(&p), items.len()),
                    fewer,
                    &listed(rest, ends),
                );
            }
        }
        // The same with one kind of the row's claims withheld. A list that
        // is refused only because a claim says what it leaves out (f is
        // unbounded at 1; f′ changes sign in this box) passes once that
        // claim is left out too, which a certificate is free to do: the
        // rest of its claims are as true as before.
        let mut kinds: Vec<String> = Vec::new();
        for c in b["cert"]["claims"].as_array().into_iter().flatten() {
            if let Some(k) = c.as_object().and_then(|o| o.keys().next())
                && !kinds.contains(k)
            {
                kinds.push(k.clone());
            }
        }
        for kind in &kinds {
            let without = |v: &mut Value| {
                if let Some(cs) = body_mut(v, row)["cert"]["claims"].as_array_mut() {
                    cs.retain(|c| c.get(kind.as_str()).is_none());
                }
            };
            if !items.is_empty() {
                let emptied = listed(Vec::new(), ends.as_ref().map(|_| json!([])));
                push(
                    "items removed, a claim kind withheld",
                    format!(
                        "{}: its {} items removed, with none of its {kind} claims",
                        show(&p),
                        items.len()
                    ),
                    fewer,
                    &|v: &mut Value| {
                        emptied(v);
                        without(v);
                    },
                );
            }
            if items.len() > 1 {
                for i in 0..items.len().min(3) {
                    let mut rest = items.clone();
                    rest.remove(i);
                    let ends = ends.clone().map(|mut e| {
                        if i < e.len() {
                            e.remove(i);
                        }
                        Value::Array(e)
                    });
                    let removed = listed(rest, ends);
                    push(
                        "items removed, a claim kind withheld",
                        format!(
                            "{}: item {i} of {} removed, with none of its {kind} claims",
                            show(&p),
                            items.len()
                        ),
                        fewer,
                        &|v: &mut Value| {
                            removed(v);
                            without(v);
                        },
                    );
                }
            }
        }
        // An item nothing proves, and that says something the list did
        // not: an existing one moved between two, far out, and onto a pole
        // or hole; or one made up where there is none.
        let mut added: Vec<(String, Value)> = Vec::new();
        let end = |x: f64| json!({"At": {"x": enc(x, x), "closed": true}});
        if row == "domain" && p == [Seg::K("pieces".into())] {
            // (A piece made up inside the domain says nothing new: about
            // a pole or hole it does.)
            for x in poles_and_holes(base) {
                added.push((
                    format!("a piece made up about the pole or hole at {x:e}"),
                    json!({"lo": end(x - 0.25), "hi": end(x + 0.25)}),
                ));
            }
        } else if row == "range" && p.is_empty() {
            // (A piece inside the listed range says nothing new: one past
            // every listed end does, where the range is bounded that way.)
            let finite: Vec<f64> = paths(&value, &|n| enc_of(n).is_some())
                .iter()
                .filter_map(|q| enc_of(at(&mut value.clone(), q)))
                .flat_map(|(lo, hi)| [lo, hi])
                .collect();
            let reaches = |inf: &str| !paths(&value, &|n| n == inf).is_empty();
            let top = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let bottom = finite.iter().copied().fold(f64::INFINITY, f64::min);
            if !reaches("PosInf") && top.is_finite() {
                added.push((
                    format!(
                        "a piece made up above the range, [{}, {}]",
                        top + 1.0,
                        top + 2.0
                    ),
                    json!({"lo": end(top + 1.0), "hi": end(top + 2.0)}),
                ));
            }
            if !reaches("NegInf") && bottom.is_finite() {
                added.push((
                    format!(
                        "a piece made up below the range, [{}, {}]",
                        bottom - 2.0,
                        bottom - 1.0
                    ),
                    json!({"lo": end(bottom - 2.0), "hi": end(bottom - 1.0)}),
                ));
            }
        } else if let Some(first) = items.first() {
            let moved = |by: f64| {
                let mut item = shifted(first, by);
                // (A monotone piece inside another of its direction says
                // nothing new: the copy runs the other way.)
                if let Some(d) = item.get("dir").and_then(Value::as_str) {
                    item["dir"] = json!(if d == "Increasing" {
                        "Decreasing"
                    } else {
                        "Increasing"
                    });
                }
                item
            };
            added.push(("a copy of item 0 moved by 0.37".into(), moved(0.37)));
            added.push((
                "a copy of item 0 moved by 12345.678".into(),
                moved(12345.678),
            ));
            let from = paths(first, &|n| enc_of(n).is_some())
                .first()
                .and_then(|q| enc_of(at(&mut first.clone(), q)));
            if let Some((lo, _)) = from {
                for x in poles_and_holes(base) {
                    added.push((
                        format!("a copy of item 0 moved onto the pole or hole at {x:e}"),
                        moved(x - lo),
                    ));
                }
            }
        } else if p.is_empty() {
            let mut at_x = vec![0.123, 12345.678];
            at_x.extend(poles_and_holes(base));
            for x in at_x {
                if let Some(item) = made_up_item(row, x) {
                    added.push((format!("an item made up at {x:e}"), item));
                }
            }
        }
        for (what, item) in added {
            // (An item the list has already says nothing new.)
            if items.contains(&item) {
                continue;
            }
            let mut more = items.clone();
            more.push(item);
            let ends = ends.clone().map(|mut e| {
                e.push(json!([["Other"], ["Other"]]));
                Value::Array(e)
            });
            push(
                "item added",
                format!("{}: {what}", show(&p)),
                None,
                &listed(more, ends),
            );
        }
    }

    // ---- enclosures: moved, narrowed, widened, exchanged
    let encs = sampled_paths(&value, &|n| enc_of(n).is_some());
    for p in &encs {
        let (lo, hi) = enc_of(at(&mut value.clone(), p)).expect("an enclosure");
        if !(lo.is_finite() && hi.is_finite()) {
            continue;
        }
        let name = show(p);
        // (A partial monotonicity row's end drawn in: DRAWN_IN. Not in a
        // complete row, where the pieces are all there are; not outward.)
        let inward = |new: (f64, f64)| {
            (row == "monotonicity" && points_only && drawn_in(&value, p, (lo, hi), new))
                .then_some(DRAWN_IN)
        };
        if lo == hi {
            let (up, down) = (lo.next_up(), lo.next_down());
            push(
                "number moved a double",
                format!("{name}: {lo:e} to the double above"),
                inward((up, up)),
                &set(p, enc(up, up)),
            );
            push(
                "number moved a double",
                format!("{name}: {lo:e} to the double below"),
                inward((down, down)),
                &set(p, enc(down, down)),
            );
        } else {
            let w = hi - lo;
            push(
                "enclosure moved clear",
                format!("{name}: [{lo:e}, {hi:e}] to just above itself"),
                None,
                &set(p, enc(hi.next_up(), hi.next_up() + w)),
            );
            push(
                "enclosure moved clear",
                format!("{name}: [{lo:e}, {hi:e}] to just below itself"),
                None,
                &set(p, enc(lo.next_down() - w, lo.next_down())),
            );
            // (An end of a monotone piece drawn in to the near side of
            // its enclosure only cuts the piece shorter.)
            let end_of = |which: &str| {
                row == "monotonicity"
                    && p.ends_with(&[
                        Seg::K(which.into()),
                        Seg::K("At".into()),
                        Seg::K("x".into()),
                    ])
            };
            push(
                "enclosure narrowed",
                format!("{name}: [{lo:e}, {hi:e}] to its low end"),
                end_of("hi").then_some(CUT_SHORTER),
                &set(p, enc(lo, lo)),
            );
            push(
                "enclosure narrowed",
                format!("{name}: [{lo:e}, {hi:e}] to its high end"),
                end_of("lo").then_some(CUT_SHORTER),
                &set(p, enc(hi, hi)),
            );
        }
        push(
            "number moved far",
            format!("{name}: [{lo:e}, {hi:e}] moved by 1"),
            inward((lo + 1.0, hi + 1.0)),
            &set(p, enc(lo + 1.0, hi + 1.0)),
        );
        push(
            "enclosure widened",
            format!("{name}: [{lo:e}, {hi:e}] by a double each way"),
            Some(WIDER),
            &set(p, enc(lo.next_down(), hi.next_up())),
        );
        // The next proven feature: the same field of the next item. (Not
        // in a points-only row, which a duplicate and one fewer leave
        // true; a window's lists every feature in it, as a complete row
        // does: review 21.)
        if !points_only
            && let Some(i) = p.iter().position(|s| matches!(s, Seg::I(_)))
            && let Seg::I(k) = &p[i]
        {
            let mut next = p.clone();
            next[i] = Seg::I(*k + 1);
            let mut vv = value.clone();
            let there = paths(&value, &|n| enc_of(n).is_some())
                .contains(&next)
                .then(|| at(&mut vv, &next).clone());
            if let Some(there) = there
                && enc_of(&there).is_some_and(|(a, b)| !(a <= lo && hi <= b))
            {
                push(
                    "number moved to the next item's",
                    format!("{name}: [{lo:e}, {hi:e}] to {there}"),
                    None,
                    &set(p, there.clone()),
                );
            }
        }
    }

    // ---- ends of pieces: open or closed, to infinity, in from infinity
    for p in sampled_paths(&value, &is_end) {
        let e = at(&mut value.clone(), &p).clone();
        let closed = e["At"]["closed"] == json!(true);
        let mut flipped = e.clone();
        flipped["At"]["closed"] = json!(!closed);
        push(
            if closed { "end opened" } else { "end closed" },
            format!("{}: {}", show(&p), e["At"]["x"]),
            (row == "monotonicity" && closed).then_some(OPEN_END),
            &set(&p, flipped),
        );
        let low = p.last() == Some(&Seg::K("lo".into()));
        push(
            "end moved to infinity",
            format!("{}: {}", show(&p), e["At"]["x"]),
            None,
            &set(&p, json!(if low { "NegInf" } else { "PosInf" })),
        );
    }
    if certified {
        for p in sampled_paths(&value, &|n| n == "NegInf" || n == "PosInf") {
            let low = *at(&mut value.clone(), &p) == json!("NegInf");
            let x = if low { -12345.678 } else { 12345.678 };
            for closed in [false, true] {
                push(
                    "end brought in from infinity",
                    format!(
                        "{}: to {x} ({})",
                        show(&p),
                        if closed { "closed" } else { "open" }
                    ),
                    None,
                    &set(&p, json!({"At": {"x": enc(x, x), "closed": closed}})),
                );
            }
        }
    }

    // ---- directions, kinds, sides, parities
    for set_of in [&DIRECTIONS[..], &KINDS[..], &SIDES[..], &PARITIES[..]] {
        for p in sampled_paths(&value, &|n| n.as_str().is_some_and(|s| set_of.contains(&s))) {
            let was = at(&mut value.clone(), &p)
                .as_str()
                .unwrap_or("")
                .to_string();
            for other in set_of.iter().filter(|o| **o != was) {
                push(
                    "kind changed",
                    format!("{}: {was} called {other}", show(&p)),
                    None,
                    &set(&p, json!(other)),
                );
            }
        }
    }

    // ---- one item or a family of them, every so far
    for p in sampled_paths(&value, &|n| {
        n.as_object().is_some_and(|o| {
            o.len() == 1
                && (o.get("At").is_some_and(|a| enc_of(a).is_some()) || o.contains_key("Every"))
        })
    }) {
        let s = at(&mut value.clone(), &p).clone();
        if let Some(x) = s.get("At") {
            push(
                "one called a family",
                format!("{}: {x} and every 1 from it", show(&p)),
                None,
                &set(&p, json!({"Every": {"x0": x, "period": enc(1.0, 1.0)}})),
            );
        } else {
            push(
                "a family called one",
                format!("{}: {} alone", show(&p), s["Every"]["x0"]),
                points_only.then_some(FEWER),
                &set(&p, json!({"At": s["Every"]["x0"]})),
            );
        }
    }
    for p in sampled_paths(&value, &|_| true)
        .into_iter()
        .filter(|p| p.last() == Some(&Seg::K("every".into())))
    {
        let was = at(&mut value.clone(), &p).clone();
        if was.is_null() {
            push(
                "one called a family",
                format!("{}: every 1 from it", show(&p)),
                None,
                &set(&p, enc(1.0, 1.0)),
            );
        } else {
            push(
                "a family called one",
                format!("{}: {was} dropped", show(&p)),
                points_only.then_some(FEWER),
                &set(&p, Value::Null),
            );
        }
    }

    // ---- a value there or not (the period, the y-intercept)
    if value.is_null() {
        for x in [1.0, 0.0] {
            push(
                "none called one",
                format!("value: none called {x}"),
                None,
                &set(&[], enc(x, x)),
            );
        }
    } else if enc_of(&value).is_some() {
        push(
            "one called none",
            format!("value: {value} called none"),
            None,
            &set(&[], Value::Null),
        );
    }

    // ---- how a horizontal asymptote reads
    for p in sampled_paths(&value, &|_| true) {
        match p.last() {
            Some(Seg::K(k)) if k == "exact" => {
                let was = at(&mut value.clone(), &p).clone();
                if !was.is_null() {
                    push(
                        "exact form changed",
                        format!("{}: {was} called 7", show(&p)),
                        None,
                        &set(&p, json!("7")),
                    );
                    // Another exact number whose enclosure meets the
                    // limit's: overlapping enclosures are no equality.
                    for (how, near) in nearby_exact(was.as_str().unwrap_or("")) {
                        push(
                            "exact form moved beside",
                            format!("{}: {was} called {near} ({how})", show(&p)),
                            None,
                            &set(&p, json!(near)),
                        );
                    }
                    push(
                        "exact form dropped",
                        format!("{}: {was}", show(&p)),
                        Some(NO_EXACT_FORM),
                        &set(&p, Value::Null),
                    );
                } else {
                    push(
                        "exact form changed",
                        format!("{}: none called 7", show(&p)),
                        None,
                        &set(&p, json!("7")),
                    );
                }
            }
            Some(Seg::K(k)) if k == "looks_like" => {
                let was = at(&mut value.clone(), &p).clone();
                let new = was.as_f64().map_or(7.0, |x| x + 1.0);
                push(
                    "exact form changed",
                    format!("{}: {was} called {new}", show(&p)),
                    None,
                    &set(&p, json!(new)),
                );
            }
            _ => {}
        }
    }

    // ---- a place dropped, and what the claims said of it but "a gap"
    //
    // From a complete row (over the line, a period or a window: review
    // 21) of poles, zeros or excluded families: an item
    // (or a family, each of its members in the row's window) left out,
    // with every claim about its place (an excluded point, a pole, a
    // zero's box, a one-sided limit there: within 16 doubles of it), and
    // a gap in their place (cleared of f's zeros, for the zeros). The
    // domain row stays the genuine one. A gap beside or about another
    // place, or holding one a claim places, was taken to hold nothing more
    // (review 20, R20-M-03; round 20's checker audit).
    let claims_now = b["cert"]["claims"].as_array().cloned().unwrap_or_default();
    let window_of = || -> Option<(f64, f64)> {
        let w = covers.get("Period").or_else(|| covers.get("Window"))?;
        Some((replay::r(w.get("a")?).ok()?, replay::r(w.get("b")?).ok()?))
    };
    if !points_only && matches!(row, "vertical" | "x_intercepts" | "domain") {
        let items_path: Vec<Seg> = if row == "domain" {
            vec![Seg::K("excluded".into())]
        } else {
            Vec::new()
        };
        let items = at(&mut value.clone(), &items_path)
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (i, item) in items.iter().enumerate() {
            if !taken(i, items.len()) {
                continue;
            }
            // (The places, and the family's x₀ and period.)
            type Family = Option<((f64, f64), (f64, f64))>;
            let (spots, family): (Vec<(f64, f64)>, Family) =
                if let Some(x) = item.get("At").and_then(enc_of) {
                    (vec![x], None)
                } else if let Some(e) = item
                    .get("Every")
                    .or(Some(item))
                    .filter(|e| e.get("x0").is_some())
                {
                    let (Some(x0), Some(per)) = (enc_of(&e["x0"]), enc_of(&e["period"])) else {
                        continue;
                    };
                    let spots = match window_of() {
                        Some(w) => members_in(x0, per, w),
                        None => vec![x0],
                    };
                    (spots, Some((x0, per)))
                } else {
                    continue;
                };
            let mut rest = items.clone();
            rest.remove(i);
            let new_claims = if row == "domain" {
                // (The family's own claim: the one naming its boxes.)
                let (x0, per) = family.expect("a family");
                let named = |c: &Value| {
                    c.get("Family").is_some_and(|f| {
                        let bx = |v: &Value| {
                            Some((replay::r(v.get("a")?).ok()?, replay::r(v.get("b")?).ok()?))
                        };
                        bx(&f["x0"]) == Some(x0) && bx(&f["period"]) == Some(per)
                    })
                };
                let kept: Vec<Value> = claims_now.iter().filter(|c| !named(c)).cloned().collect();
                (kept.len() < claims_now.len()).then_some(kept)
            } else {
                gapped_claims(&claims_now, &spots, (row == "x_intercepts").then_some("F0"))
            };
            let Some(new_claims) = new_claims else {
                continue;
            };
            let items_path = items_path.clone();
            push(
                "place dropped, its claims a gap",
                if row == "domain" {
                    format!(
                        "family {i} of {} dropped, with its Family claim",
                        items.len()
                    )
                } else {
                    format!(
                        "item {i} of {} dropped ({} place{} in the window), its claims a gap",
                        items.len(),
                        spots.len(),
                        if spots.len() == 1 { "" } else { "s" }
                    )
                },
                None,
                &|v: &mut Value| {
                    *at(&mut body_mut(v, row)["value"], &items_path) = Value::Array(rest.clone());
                    body_mut(v, row)["cert"]["claims"] = Value::Array(new_claims.clone());
                },
            );
        }
    }

    // ---- two places listed as one
    //
    // Two items of a list (points, not families) as one, its enclosure
    // holding both. A complete row lists each zero, turn or pole: one item
    // for two is one left out. (A partial row's wider enclosure is true of
    // the item it was.)
    if matches!(row, "x_intercepts" | "vertical" | "extrema" | "inflections") {
        let items = value.as_array().cloned().unwrap_or_default();
        let x_of = |it: &Value| -> Option<(f64, f64)> {
            match row {
                "x_intercepts" | "vertical" => enc_of(it.get("At")?),
                _ => it
                    .get("every")
                    .is_none_or(Value::is_null)
                    .then(|| enc_of(it.get("x")?))
                    .flatten(),
            }
        };
        for i in 0..items.len().saturating_sub(1) {
            if !taken(i, items.len()) {
                continue;
            }
            let (Some(a), Some(c)) = (x_of(&items[i]), x_of(&items[i + 1])) else {
                continue;
            };
            let hull = (a.0.min(c.0), a.1.max(c.1));
            let mut one = items[i].clone();
            match row {
                "x_intercepts" | "vertical" => one["At"] = enc(hull.0, hull.1),
                _ => {
                    one["x"] = enc(hull.0, hull.1);
                    if let (Some(ya), Some(yc)) =
                        (enc_of(&items[i]["y"]), enc_of(&items[i + 1]["y"]))
                    {
                        one["y"] = enc(ya.0.min(yc.0), ya.1.max(yc.1));
                    }
                }
            }
            let mut merged = items.clone();
            merged[i] = one;
            merged.remove(i + 1);
            push(
                "two places listed as one",
                format!(
                    "items {i} and {} as one, over [{:e}, {:e}]",
                    i + 1,
                    hull.0,
                    hull.1
                ),
                points_only.then_some(ONE_OF_TWO),
                &|v: &mut Value| body_mut(v, row)["value"] = Value::Array(merged.clone()),
            );
        }
    }

    // ---- a window from a place, the place left out
    //
    // A complete list (over the line, or a window) made complete over a
    // window that starts (ends) at an item's enclosure: the item and those
    // before (after) it left out, and every claim about the reals before
    // (after) the window with them. The item lies in the window, closed,
    // so the row leaves one of its features out: false by construction. A
    // zero, turn or inflection on a window's end, its far side signed by
    // no claim, was never derived, and "no extrema for 0 ≤ x ≤ 2" passed
    // for x² − 1 (review 21's audit of the window's promise).
    let windowed = (certified && covers == json!("Line")) || (!certified && window_of().is_some());
    if windowed && matches!(row, "x_intercepts" | "extrema" | "inflections") {
        let items = value.as_array().cloned().unwrap_or_default();
        let x_of = |it: &Value| -> Option<(f64, f64)> {
            match row {
                "x_intercepts" => enc_of(it.get("At")?),
                _ => it
                    .get("every")
                    .is_none_or(Value::is_null)
                    .then(|| enc_of(it.get("x")?))
                    .flatten(),
            }
        };
        let old = window_of();
        for (i, item) in items.iter().enumerate() {
            let Some((lo, hi)) = x_of(item) else {
                continue;
            };
            if !(taken(i, items.len()) && lo.is_finite() && hi.is_finite()) {
                continue;
            }
            let reach = hi.abs().max(lo.abs()) + 4.0;
            for from in [true, false] {
                // The window, the place on its end; the claims and items
                // on the window's side of that end.
                let (a, b) = match (old, from) {
                    (Some((_, b)), true) => (lo, b),
                    (Some((a, _)), false) => (a, hi),
                    (None, true) => (lo, hi + reach),
                    (None, false) => (lo - reach, hi),
                };
                if !(a < lo || b > hi) || a > b || !(a.is_finite() && b.is_finite()) {
                    continue;
                }
                let kept_claim = |c: &Value| match claim_spot(c) {
                    Some((s, _)) => {
                        if from {
                            s.0 >= lo
                        } else {
                            s.1 <= hi
                        }
                    }
                    None => {
                        let side = c
                            .as_object()
                            .and_then(|o| o.values().next())
                            .and_then(|b| b.get("side"))
                            .and_then(Value::as_str);
                        side.is_none_or(|s| s == if from { "Right" } else { "Left" })
                    }
                };
                let claims: Vec<Value> = claims_now
                    .iter()
                    .filter(|c| kept_claim(c))
                    .cloned()
                    .collect();
                let rest: Vec<Value> = items
                    .iter()
                    .filter(|it| x_of(it).is_some_and(|(l, h)| if from { l > hi } else { h < lo }))
                    .cloned()
                    .collect();
                push(
                    "window from a place, the place left out",
                    format!("item {i} at [{lo:e}, {hi:e}] left out, complete over [{a:e}, {b:e}]"),
                    None,
                    &|v: &mut Value| {
                        let mut body = body_mut(v, row).clone();
                        body["value"] = Value::Array(rest.clone());
                        body["cert"]["covers"] = json!({"Window": {"a": num(a), "b": num(b)}});
                        body["cert"]["claims"] = Value::Array(claims.clone());
                        v[row] = json!({ "Partial": body });
                    },
                );
            }
        }
    }

    // ---- a place behind a gap: on a window's end, or between two gaps
    //
    // The operators above withhold claims, or change the row; none puts a
    // true claim in the place of another. A zero, turn or inflection at a
    // double p is decided by its claims; in their place, a gap with p for
    // an end, cleared of f⁽ᵏ⁾'s zeros strictly inside it, and f⁽ᵏ⁾'s sign
    // from the gap's other end on to where they reached, decide nothing at
    // p, and are as true: so the row must ask p of something else. With p
    // on the end of a window the row is made complete over (its item left
    // out, or listed a double inside the window), or p inside the row's
    // region between two such gaps (its item left out), every premise is
    // true and the row false (review 22, R22-M-01: x² − 1's minimum at 0
    // left out of "no extrema for 0 ≤ x ≤ 2" passed, every claim strong).
    // A mutant whose new premises the replay's own claim check doesn't
    // prove is not made: it would test the premises, not the row.
    let order = match row {
        "x_intercepts" => Some(0),
        "extrema" | "monotonicity" => Some(1),
        "inflections" => Some(2),
        _ => None,
    };
    if let Some(k) = order
        && !points_only
    {
        let of = ["F0", "F1", "F2"][k];
        let items = value.as_array().cloned().unwrap_or_default();
        let x_of = |it: &Value| -> Option<(f64, f64)> {
            match row {
                "x_intercepts" => enc_of(it.get("At")?),
                _ => it
                    .get("every")
                    .is_none_or(Value::is_null)
                    .then(|| enc_of(it.get("x")?))
                    .flatten(),
            }
        };
        // The places: (what, index, p, the list without what it says of
        // p): each item at a double; for the monotone pieces, an end two
        // meet at, at a double (a turn), the two joined over it.
        let mut places: Vec<(String, usize, f64, Vec<Value>)> = Vec::new();
        if row == "monotonicity" {
            for j in 0..items.len().saturating_sub(1) {
                let end = &items[j]["on"]["hi"];
                let Some((p, q)) = enc_of(&end["At"]["x"]) else {
                    continue;
                };
                if !(taken(j, items.len()) && p == q && p.is_finite())
                    || items[j + 1]["on"]["lo"]["At"]["x"] != end["At"]["x"]
                {
                    continue;
                }
                let mut one = items[j].clone();
                one["on"]["hi"] = items[j + 1]["on"]["hi"].clone();
                let mut joined = items.clone();
                joined[j] = one;
                joined.remove(j + 1);
                places.push((
                    format!("pieces {j} and {} joined over", j + 1),
                    j,
                    p,
                    joined,
                ));
            }
        } else {
            for (i, item) in items.iter().enumerate() {
                let Some((p, hi)) = x_of(item) else {
                    continue;
                };
                if !(taken(i, items.len()) && p == hi && p.is_finite()) {
                    continue;
                }
                let mut rest = items.clone();
                rest.remove(i);
                places.push((format!("item {i} at"), i, p, rest));
            }
        }
        let old = window_of();
        let mut session = replay::Session::default();
        let gap = |a: f64, b: f64| {
            [
                json!({"Gap": {"x": {"a": num(a), "b": num(b)}}}),
                json!({"GapClear": {"x": {"a": num(a), "b": num(b)}, "of": {"F": of}}}),
            ]
        };
        let sign = |a: f64, b: f64, up: bool| {
            json!({"Beyond": {"x": {"a": num(a), "b": num(b)}, "of": {"F": of}, "c": 0.0,
                "above": up}})
        };
        // (body, list, claims) → the mutated certificate, made only where
        // its new claims are true: f⁽ᵏ⁾'s sign on each stretch filled in
        // is tried both ways.
        let mut make = |covers: Value,
                        status: &str,
                        list: Vec<Value>,
                        claims: Vec<Value>,
                        fills: &[(f64, f64)]|
         -> Option<Value> {
            let fills: Vec<(f64, f64)> = fills.iter().copied().filter(|f| f.0 < f.1).collect();
            for signs in 0..(1usize << fills.len()) {
                let mut cs = claims.clone();
                for (j, f) in fills.iter().enumerate() {
                    cs.push(sign(f.0, f.1, signs & (1 << j) == 0));
                }
                let mut v = base.clone();
                let mut body = body_mut(&mut v, row).clone();
                body["value"] = Value::Array(list.clone());
                body["cert"]["covers"] = covers.clone();
                body["cert"]["claims"] = Value::Array(cs);
                v[row] = json!({ status: body });
                let true_premises = replay::replay_in(&v, &mut session, Some(row)).is_ok_and(|r| {
                    r.claims.iter().all(|c| {
                        matches!(c.outcome.class, replay::Class::Strong | replay::Class::Weak)
                    })
                });
                if true_premises {
                    return Some(v);
                }
            }
            None
        };
        for (what, i, p, without) in places {
            let (below, above) = (p.next_down(), p.next_up());
            // The claims about p (their boxes hold it), and how far they
            // reached either side.
            let about_p = |c: &Value| claim_spot(c).is_some_and(|(s, _)| s.0 <= p && p <= s.1);
            let (reach_lo, reach_hi) = claims_now
                .iter()
                .filter(|c| about_p(c))
                .filter_map(|c| claim_spot(c).map(|(s, _)| s))
                .fold((p, p), |(a, b), s| (a.min(s.0), b.max(s.1)));
            // On a window's end: the window from p (or to p), as in "a
            // window from a place" above; the claims on its side kept but
            // those about p; a gap from p (to p) in their place. (Not for
            // monotone pieces: one cut at a window's end is as true.)
            let windowed = row != "monotonicity"
                && ((certified && (covers == json!("Line") || covers.get("Period").is_some()))
                    || (!certified && old.is_some()));
            if windowed {
                let reach = p.abs() + 4.0;
                for from in [true, false] {
                    let (a, b) = match (old, from) {
                        (Some((_, b)), true) => (p, b),
                        (Some((a, _)), false) => (a, p),
                        (None, true) => (p, p + reach),
                        (None, false) => (p - reach, p),
                    };
                    if !(a.is_finite() && b.is_finite() && a <= b) {
                        continue;
                    }
                    let kept: Vec<Value> = claims_now
                        .iter()
                        .filter(|c| !about_p(c))
                        .filter(|c| match claim_spot(c) {
                            Some((s, _)) => {
                                if from {
                                    s.0 >= p
                                } else {
                                    s.1 <= p
                                }
                            }
                            None => {
                                let side = c
                                    .as_object()
                                    .and_then(|o| o.values().next())
                                    .and_then(|b| b.get("side"))
                                    .and_then(Value::as_str);
                                side.is_none_or(|s| s == if from { "Right" } else { "Left" })
                            }
                        })
                        .cloned()
                        .collect();
                    let rest: Vec<Value> = items
                        .iter()
                        .filter(|it| {
                            x_of(it).is_some_and(|(l, h)| if from { l > p } else { h < p })
                        })
                        .cloned()
                        .collect();
                    let (g, fill) = if from {
                        ((p, above), (above, reach_hi))
                    } else {
                        ((below, p), (reach_lo, below))
                    };
                    let mut claims = kept.clone();
                    claims.extend(gap(g.0, g.1));
                    let inward = if from { above } else { below };
                    let mut moved_item = items[i].clone();
                    match row {
                        "x_intercepts" => moved_item["At"] = enc(inward, inward),
                        _ => moved_item["x"] = enc(inward, inward),
                    }
                    let mut moved = rest.clone();
                    moved.push(moved_item);
                    for (how, list) in [("left out", rest.clone()), ("moved a double in", moved)] {
                        let w = json!({"Window": {"a": num(a), "b": num(b)}});
                        if let Some(v) = make(w, "Partial", list, claims.clone(), &[fill]) {
                            push(
                                "window end behind a gap",
                                format!(
                                    "{what} {p:e} {how}, complete over [{a:e}, {b:e}], a gap \
                                     [{:e}, {:e}] in place of its claims",
                                    g.0, g.1
                                ),
                                None,
                                &|m: &mut Value| *m = v.clone(),
                            );
                        }
                    }
                }
            }
            // Inside the region: two gaps meeting at p.
            let inside = match old {
                Some((a, b)) => a < p && p < b,
                None => certified,
            };
            if inside {
                let mut claims: Vec<Value> =
                    claims_now.iter().filter(|c| !about_p(c)).cloned().collect();
                claims.extend(gap(below, p));
                claims.extend(gap(p, above));
                let status = if certified { "Certified" } else { "Partial" };
                if let Some(v) = make(
                    covers.clone(),
                    status,
                    without,
                    claims,
                    &[(reach_lo, below), (above, reach_hi)],
                ) {
                    let gone = if row == "monotonicity" {
                        ""
                    } else {
                        " left out"
                    };
                    push(
                        "place between two gaps",
                        format!(
                            "{what} {p:e}{gone}, gaps [{below:e}, {p:e}] and [{p:e}, {above:e}] \
                             in place of its claims"
                        ),
                        None,
                        &|m: &mut Value| *m = v.clone(),
                    );
                }
            }
        }
    }

    // ---- a kink in a gap
    //
    // A kink box (`Kink`, `KinkAt`, `KinkSides`) and the claims inside it
    // given way to a gap over it, cleared of f⁽ᵏ⁾'s zeros (true where
    // f⁽ᵏ⁾ changes sign at the kink with no zero: (x − 1)^(2/3)'s f′); the
    // gap as the box, a double wider each way (the signs beside it now
    // overlapping its ends), or two gaps overlapping; and the turn (or
    // inflection) at the kink left out of the row, or the monotone pieces
    // either side of it joined. Every premise true (each new one proven by
    // the replay's own check, else no mutant), the row false: a gap's
    // clearness says nothing of a point inside it where f⁽ᵏ⁾ doesn't exist
    // (review 22's audit).
    let kink_order = match row {
        "extrema" | "monotonicity" => Some(1),
        "inflections" => Some(2),
        _ => None,
    };
    if let Some(k) = kink_order {
        let of = ["F0", "F1", "F2"][k];
        let bx = |v: &Value| Some((replay::r(v.get("a")?).ok()?, replay::r(v.get("b")?).ok()?));
        let kinks: Vec<(f64, f64)> = claims_now
            .iter()
            .filter_map(|c| {
                let (kind, b) = c.as_object()?.iter().next()?;
                matches!(kind.as_str(), "Kink" | "KinkAt" | "KinkSides")
                    .then(|| bx(b.get("x")?))
                    .flatten()
            })
            .fold(Vec::new(), |mut v, x| {
                if !v.contains(&x) {
                    v.push(x);
                }
                v
            });
        let mut session = replay::Session::default();
        for (n, kb) in kinks.iter().enumerate() {
            if !(kb.0.is_finite() && kb.1.is_finite()) || doubles_out(kb.0, 60, true) < kb.1 {
                continue;
            }
            // The row without what it said of the kink.
            let new_value = match row {
                "monotonicity" => {
                    let items = value.as_array().cloned().unwrap_or_default();
                    let at_kink = |b: &Value| {
                        enc_of(&b["At"]["x"]).is_some_and(|(l, h)| kb.0 <= h && l <= kb.1)
                    };
                    let j = (0..items.len().saturating_sub(1)).find(|&j| {
                        at_kink(&items[j]["on"]["hi"]) && at_kink(&items[j + 1]["on"]["lo"])
                    });
                    j.map(|j| {
                        let mut one = items[j].clone();
                        one["on"]["hi"] = items[j + 1]["on"]["hi"].clone();
                        let mut fewer = items.clone();
                        fewer[j] = one;
                        fewer.remove(j + 1);
                        Value::Array(fewer)
                    })
                }
                _ => {
                    let items = value.as_array().cloned().unwrap_or_default();
                    let rest: Vec<Value> = items
                        .iter()
                        .filter(|it| !enc_of(&it["x"]).is_some_and(|(l, h)| kb.0 <= h && l <= kb.1))
                        .cloned()
                        .collect();
                    (rest.len() < items.len()).then_some(Value::Array(rest))
                }
            };
            let Some(new_value) = new_value else {
                continue;
            };
            let inside_kink =
                |c: &Value| claim_spot(c).is_some_and(|(s, _)| kb.0 <= s.0 && s.1 <= kb.1);
            let kept: Vec<Value> = claims_now
                .iter()
                .filter(|c| !inside_kink(c))
                .cloned()
                .collect();
            let (a, b) = *kb;
            let shapes: [(&str, Vec<(f64, f64)>); 3] = [
                ("over the kink box", vec![(a, b)]),
                (
                    "a double wider each way",
                    vec![(a.next_down(), b.next_up())],
                ),
                (
                    "two overlapping",
                    vec![(a.next_down(), b), (a, b.next_up())],
                ),
            ];
            for (how, gaps) in shapes {
                let mut claims = kept.clone();
                for g in &gaps {
                    claims.push(json!({"Gap": {"x": {"a": num(g.0), "b": num(g.1)}}}));
                    claims.push(
                        json!({"GapClear": {"x": {"a": num(g.0), "b": num(g.1)}, "of": {"F": of}}}),
                    );
                }
                let mut v = base.clone();
                body_mut(&mut v, row)["value"] = new_value.clone();
                body_mut(&mut v, row)["cert"]["claims"] = Value::Array(claims);
                let true_premises = replay::replay_in(&v, &mut session, Some(row)).is_ok_and(|r| {
                    r.claims.iter().all(|c| {
                        matches!(c.outcome.class, replay::Class::Strong | replay::Class::Weak)
                    })
                });
                if !true_premises {
                    continue;
                }
                push(
                    "kink in a gap",
                    format!(
                        "kink box {n} [{a:e}, {b:e}]: its claims a gap, {how}; what the row said of it left out"
                    ),
                    None,
                    &|m: &mut Value| *m = v.clone(),
                );
            }
        }
    }

    // ---- an end's extremum read from past the turn beside it
    //
    // An extremum at a closed end of the domain, called the other kind, in
    // a partial row whose only f′ claims lie past the next turn inward:
    // f′'s sign there is the other one, and nothing says what f does
    // between (a partial row asks no coverage). True only if f′ had the
    // far sign up to the end, which the turn between rules out.
    if row == "extrema" {
        let items = value.as_array().cloned().unwrap_or_default();
        let closed_ends: Vec<(f64, f64)> = body(base, "domain")
            .map(|(_, d)| {
                paths(&d["value"]["pieces"], &is_end)
                    .into_iter()
                    .filter_map(|p| {
                        let e = at(&mut d["value"]["pieces"].clone(), &p).clone();
                        (e["At"]["closed"] == json!(true))
                            .then(|| enc_of(&e["At"]["x"]))
                            .flatten()
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (i, item) in items.iter().enumerate() {
            let Some(xe) = enc_of(&item["x"]) else {
                continue;
            };
            if !closed_ends.contains(&xe) || !item["every"].is_null() {
                continue;
            }
            // The turn nearest the end, and which side the end is on.
            let Some((turn, low)) = items
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .filter_map(|(_, it)| enc_of(&it["x"]))
                .map(|t| (t, t.0 > xe.1))
                .min_by(|a, b| {
                    let d = |t: &(f64, f64)| (t.0 - xe.1).abs().min((xe.0 - t.1).abs());
                    d(&a.0).total_cmp(&d(&b.0))
                })
            else {
                continue;
            };
            let past = |c: &Value| {
                let Some((kind, b)) = c.as_object().and_then(|o| o.iter().next()) else {
                    return false;
                };
                let of_f1 = b.get("of") == Some(&json!({"F": "F1"}));
                let bx =
                    |v: &Value| Some((replay::r(v.get("a")?).ok()?, replay::r(v.get("b")?).ok()?));
                // (From the turn on: a box that starts at it, the turn's own
                // claim among them, and every one past it.)
                match kind.as_str() {
                    "Beyond" | "NoCross" | "Factors" | "OneCross" | "ExactAt" | "Touch"
                    | "Value"
                        if of_f1 =>
                    {
                        bx(&b["x"]).is_some_and(|x| if low { x.0 >= turn.0 } else { x.1 <= turn.1 })
                    }
                    "TailBeyond" | "TailChain" if of_f1 => {
                        let right = b["side"] == "Right";
                        right == low
                            && replay::r(&b["from"])
                                .is_ok_and(|f| if low { f >= turn.0 } else { f <= turn.1 })
                    }
                    "Value" => bx(&b["x"]) == Some(xe) && b.get("of") == Some(&json!({"F": "F0"})),
                    _ => false,
                }
            };
            let kept: Vec<Value> = b["cert"]["claims"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| past(c))
                .cloned()
                .collect();
            let mut one = item.clone();
            one["kind"] = json!(if item["kind"] == "Min" { "Max" } else { "Min" });
            let row_now = json!({"Partial": {"value": [one],
                "cert": {"covers": "Points", "claims": kept}}});
            push(
                "end extremum read from past a turn",
                format!(
                    "item {i}, at the closed end [{:e}, {:e}], called {}, with only the f′ claims \
                     past the turn at [{:e}, {:e}]",
                    xe.0, xe.1, one["kind"], turn.0, turn.1
                ),
                None,
                &|v: &mut Value| v[row] = row_now.clone(),
            );
        }
    }

    // ---- pieces joined over the place between them
    //
    // Two pieces meeting at one enclosure (the domain's either side of an
    // excluded point; monotone pieces either side of a turn or pole; the
    // range's either side of a value it leaves out) as one piece across it.
    if matches!(row, "domain" | "monotonicity" | "range") {
        let list_path: Vec<Seg> = if row == "domain" {
            vec![Seg::K("pieces".into())]
        } else {
            Vec::new()
        };
        let items = at(&mut value.clone(), &list_path)
            .as_array()
            .cloned()
            .unwrap_or_default();
        let piece_of = |it: &Value| -> Value {
            if row == "monotonicity" {
                it["on"].clone()
            } else {
                it.clone()
            }
        };
        for i in 0..items.len().saturating_sub(1) {
            if !taken(i, items.len()) {
                continue;
            }
            let (p, q) = (piece_of(&items[i]), piece_of(&items[i + 1]));
            if !(is_end(&p["hi"]) && p["hi"]["At"]["x"] == q["lo"]["At"]["x"]) {
                continue;
            }
            let joined = json!({"lo": p["lo"], "hi": q["hi"]});
            let mut one = items[i].clone();
            if row == "monotonicity" {
                one["on"] = joined;
            } else {
                one = joined;
            }
            let mut fewer = items.clone();
            fewer[i] = one;
            fewer.remove(i + 1);
            let ends = (row == "range")
                .then(|| base["range_ends"].as_array().cloned())
                .flatten()
                .filter(|e| e.len() == items.len())
                .map(|mut e| {
                    let hi = e[i + 1][1].clone();
                    e[i][1] = hi;
                    e.remove(i + 1);
                    Value::Array(e)
                });
            let list_path = list_path.clone();
            push(
                "pieces joined",
                format!(
                    "pieces {i} and {} joined over {}",
                    i + 1,
                    p["hi"]["At"]["x"]
                ),
                None,
                &|v: &mut Value| {
                    *at(&mut body_mut(v, row)["value"], &list_path) = Value::Array(fewer.clone());
                    if let Some(e) = &ends {
                        v["range_ends"] = e.clone();
                    }
                },
            );
        }
    }

    // ---- a value f takes, left out of the range as a hole's
    //
    // The listed piece that holds f's value at a double cut there, open
    // both sides, with a gap about that double (cleared of f′'s zeros) and
    // the value claimed as a removable point's: f defined there, the gap
    // passed for an excluded point (f = x as ℝ ∖ {0}: round 20's checker
    // audit).
    if row == "range" {
        let pieces = value.as_array().cloned().unwrap_or_default();
        let ends = base["range_ends"].as_array().cloned().unwrap_or_default();
        let finite = |b: &Value| enc_of(&b["At"]["x"]);
        let mut tried = 0;
        for c in &claims_now {
            let Some(v) = c.get("Value") else { continue };
            if v["of"] != json!({"F": "F0"}) || tried >= 3 {
                continue;
            }
            let (Ok(xa), Ok(xb), Ok(lo), Ok(hi)) = (
                replay::r(&v["x"]["a"]),
                replay::r(&v["x"]["b"]),
                replay::r(&v["lo"]),
                replay::r(&v["hi"]),
            ) else {
                continue;
            };
            if xa != xb || !(lo.is_finite() && hi.is_finite()) {
                continue;
            }
            // The piece the value lies strictly inside.
            let Some(i) = pieces.iter().position(|p| {
                let above = match finite(&p["lo"]) {
                    Some(e) => e.1 < lo,
                    None => p["lo"] == "NegInf",
                };
                let below = match finite(&p["hi"]) {
                    Some(e) => hi < e.0,
                    None => p["hi"] == "PosInf",
                };
                above && below
            }) else {
                continue;
            };
            tried += 1;
            let cut = json!({"At": {"x": enc(lo, hi), "closed": false}});
            let mut split = pieces.clone();
            split[i] = json!({"lo": pieces[i]["lo"], "hi": cut});
            split.insert(i + 1, json!({"lo": cut, "hi": pieces[i]["hi"]}));
            let hole = json!([{"Hole": enc(xa, xa)}]);
            let mut new_ends = ends.clone();
            if new_ends.len() == pieces.len() {
                let pair = new_ends[i].clone();
                new_ends[i] = json!([pair[0], hole]);
                new_ends.insert(i + 1, json!([hole, pair[1]]));
            }
            let g = json!({"a": num(xa.next_down()), "b": num(xa.next_up())});
            let mut more = claims_now.clone();
            more.push(json!({"Gap": {"x": g}}));
            more.push(json!({"GapClear": {"x": g, "of": {"F": "F1"}}}));
            more.push(
                json!({"Removable": {"near": g, "at": {"a": num(xa), "b": num(xa)},
                "lo": num(lo), "hi": num(hi)}}),
            );
            push(
                "a value left out as a hole's",
                format!("piece {i} cut at f({xa:e}) ∈ [{lo:e}, {hi:e}], a gap about {xa:e}"),
                None,
                &|v: &mut Value| {
                    body_mut(v, row)["value"] = Value::Array(split.clone());
                    body_mut(v, row)["cert"]["claims"] = Value::Array(more.clone());
                    v["range_ends"] = Value::Array(new_ends.clone());
                },
            );
        }
    }

    // ---- status and scope
    let window = covers
        .get("Window")
        .or_else(|| covers.get("Period"))
        .and_then(|w| Some((replay::r(w.get("a")?).ok()?, replay::r(w.get("b")?).ok()?)));
    let over = |kind: &str, (a, b): (f64, f64)| json!({kind: {"a": num(a), "b": num(b)}});
    let was_region = covers.as_str().is_some_and(|s| s == "Line")
        || covers.get("Window").is_some()
        || covers.get("Period").is_some();
    push(
        "status changed",
        format!(
            "{status} called {}",
            if certified { "Partial" } else { "Certified" }
        ),
        certified.then_some(WEAKER_STATUS),
        &|v: &mut Value| {
            let b = body_mut(v, row).clone();
            let mut m = serde_json::Map::new();
            m.insert(if certified { "Partial" } else { "Certified" }.into(), b);
            v[row] = Value::Object(m);
        },
    );
    let mut scopes: Vec<(String, Value, Option<&'static str>)> = Vec::new();
    let (wa, wb) = window.unwrap_or((-1.5, 2.5));
    let q = (wb - wa) / 4.0;
    scopes.push(("Line".into(), json!("Line"), None));
    scopes.push((
        "Points".into(),
        json!("Points"),
        (!certified).then_some(SMALLER_SCOPE),
    ));
    for kind in ["Window", "Period"] {
        let smaller = if kind == "Window" && was_region {
            Some(SMALLER_SCOPE)
        } else if kind == "Window" && !certified {
            Some(WINDOW_AS_PROVEN)
        } else {
            None
        };
        scopes.push((
            format!("{kind} [{wa}, {wb}]"),
            over(kind, (wa, wb)),
            smaller,
        ));
        scopes.push((
            format!("{kind} [{}, {}] (narrower)", wa + q, wb - q),
            over(kind, (wa + q, wb - q)),
            smaller,
        ));
        if window.is_some() {
            // (Another window at least a period wide, for a row over one
            // period: the same statement, of the line by f's period.)
            let same =
                (kind == "Period" && covers.get("Period").is_some()).then_some(SAME_BY_PERIOD);
            scopes.push((
                format!("{kind} [{}, {}] (wider)", wa - 1.0, wb + 1.0),
                over(kind, (wa - 1.0, wb + 1.0)),
                same,
            ));
            scopes.push((
                format!("{kind} [{}, {}] (moved)", wa + q, wb + q),
                over(kind, (wa + q, wb + q)),
                same,
            ));
        }
    }
    for (name, new, benign) in scopes {
        push(
            "scope changed",
            format!("covers {covers} called {name}"),
            benign,
            &|v: &mut Value| body_mut(v, row)["cert"]["covers"] = new.clone(),
        );
    }

    // ---- claims withheld
    let domain_value = body(base, "domain").map(|(_, d)| d["value"].clone());
    let no_families = domain_value
        .as_ref()
        .is_some_and(|d| d["excluded"].as_array().is_some_and(Vec::is_empty));
    let not_the_line = domain_value
        .as_ref()
        .is_some_and(|d| d["pieces"] != json!([{"lo": "NegInf", "hi": "PosInf"}]));
    let reaching = body(base, "monotonicity").is_some_and(|(status, m)| {
        status == "Certified"
            && m["value"].as_array().is_some_and(|ps| {
                ps.iter().any(|p| {
                    (p["on"]["lo"] == "NegInf" || p["on"]["hi"] == "PosInf")
                        && p["dir"] != "Constant"
                })
            })
    });
    let level_both = body(base, "horizontal").is_some_and(|(status, h)| {
        status == "Certified"
            && ["Left", "Right"].iter().all(|side| {
                h["value"]
                    .as_array()
                    .is_some_and(|hs| hs.iter().any(|x| x["side"] == *side))
            })
    });
    let rests_on_no_claim = match row {
        "oblique" if value.as_array().is_some_and(Vec::is_empty) && level_both => {
            Some(NO_LINE_BESIDE_A_LEVEL)
        }
        "domain" if no_families => Some(ON_F_ITSELF),
        "y_intercept" if value.is_null() => Some(ON_F_ITSELF),
        "period" if value.is_null() && ((no_families && not_the_line) || reaching) => {
            Some(BY_ANOTHER_ROW)
        }
        _ => None,
    };
    if b["cert"]["claims"]
        .as_array()
        .is_some_and(|c| !c.is_empty())
    {
        push(
            "claims withheld",
            "the row as it is, with none of its claims".into(),
            rests_on_no_claim,
            &|v: &mut Value| body_mut(v, row)["cert"]["claims"] = json!([]),
        );
    }

    // ---- a kink's sides claimed wrong: what f⁽ᵏ⁾ does on each side of a
    // kink box (`KinkSides`), its sign, its point, joined. (A premise, not
    // the row: refused by its check, whatever the row says.)
    let claims = b["cert"]["claims"].as_array().cloned().unwrap_or_default();
    for (i, c) in claims.iter().enumerate() {
        let Some(k) = c.get("KinkSides") else {
            continue;
        };
        let mut variants: Vec<(String, Value, Option<&'static str>)> = Vec::new();
        for side in ["left", "right"] {
            for sign in ["Neg", "Zero", "Pos"] {
                if k[side] != sign {
                    let mut v = k.clone();
                    v[side] = json!(sign);
                    variants.push((format!("{side} called {sign}"), v, None));
                }
            }
        }
        let joined = k["joined"].as_bool().unwrap_or(false);
        match k["at"].as_f64() {
            Some(at) => {
                for q in [at.next_up(), at.next_down()] {
                    let mut v = k.clone();
                    v["at"] = num(q);
                    variants.push((format!("placed at {q:e}, not {at:e}"), v, None));
                }
                let mut v = k.clone();
                v["at"] = Value::Null;
                v["joined"] = json!(false);
                variants.push(("left unplaced".into(), v, Some(UNPLACED)));
            }
            None => {
                let (a, bb) = (
                    replay::r(&k["x"]["a"]).unwrap_or(0.0),
                    replay::r(&k["x"]["b"]).unwrap_or(0.0),
                );
                let mid = a / 2.0 + bb / 2.0;
                let mut v = k.clone();
                v["at"] = num(mid);
                variants.push((format!("placed at {mid:e}"), v, None));
            }
        }
        let mut v = k.clone();
        v["joined"] = json!(!joined);
        variants.push((
            if joined { "joined dropped" } else { "joined" }.into(),
            v,
            joined.then_some(JOINED_DROPPED),
        ));
        for (what, new, benign) in variants {
            let k = k.clone();
            push(
                "kink claim changed",
                format!("{}: {what}", serde_json::to_string(&k).unwrap_or_default()),
                benign,
                &|v: &mut Value| {
                    body_mut(v, row)["cert"]["claims"][i] = json!({"KinkSides": new.clone()})
                },
            );
        }
    }

    // ---- the same row of another function (one of at most a thousand
    // claims: each is checked on f afresh, and five thousand bands of f″
    // of 1/(1 + e^(x/1000)), checked on 1/(1 + e^x) + 10⁻⁷, took the pool
    // run past an hour)
    if let Some(o) = other
        && let Some((_, ob)) = body(o, row)
        && (ob["value"] != value || ob["cert"]["covers"] != covers)
        && ob["cert"]["claims"]
            .as_array()
            .is_some_and(|c| c.len() <= 1000)
    {
        push(
            "row of another function",
            format!("the {row} row of {}", o["source"]),
            None,
            &|v: &mut Value| {
                v[row] = o[row].clone();
                if row == "range" {
                    v["range_ends"] = o["range_ends"].clone();
                }
            },
        );
    }

    // ---- a row made up, with no claims
    let line = json!([{"lo": "NegInf", "hi": "PosInf"}]);
    let made_up: Vec<Value> = match row {
        "domain" => vec![
            json!({"pieces": line, "excluded": []}),
            json!({"pieces": [{"lo": {"At": {"x": enc(0.0, 0.0), "closed": true}},
                "hi": "PosInf"}], "excluded": []}),
        ],
        "y_intercept" | "period" => vec![Value::Null, enc(7.0, 7.0)],
        "parity" => PARITIES.iter().map(|p| json!(p)).collect(),
        "monotonicity" => vec![
            json!([]),
            json!([{"on": line[0], "dir": "Increasing"}]),
            json!([{"on": line[0], "dir": "Constant"}]),
        ],
        "range" => vec![line.clone(), json!([made_up_item("range", 7.0)])],
        _ => vec![json!([]), json!([made_up_item(row, 0.123)])],
    };
    for value in made_up {
        // (Not over a window: an empty list made up there is true where f
        // has nothing in it, so no mutant of its own. "A window from a
        // place" above makes the false ones.)
        for (status, covers) in [("Certified", "Line"), ("Partial", "Points")] {
            // (A partial row that lists nothing says nothing.)
            let says_nothing = status == "Partial" && value.as_array().is_some_and(Vec::is_empty);
            // (The genuine row's own value made up again is that row with
            // its claims withheld, above.)
            if says_nothing || value == b["value"] {
                continue;
            }
            push(
                "row made up, no claims",
                format!("{status} over {covers}: {value}"),
                None,
                &|v: &mut Value| {
                    v[row] = json!({status: {"value": value,
                        "cert": {"covers": covers, "claims": []}}});
                    if row == "range" {
                        let n = value.as_array().map_or(0, Vec::len);
                        v["range_ends"] = json!(vec![json!([["Other"], ["Other"]]); n]);
                    }
                },
            );
        }
    }
}

// ------------------------------------------------------------ the run

/// The settings a mutant is replayed in: with every row, with the domain
/// alone beside it, with nothing beside it, and with every row and every
/// claim of the genuine certificate as its own (all of them true: no
/// number of true claims makes a false row follow).
const SETTINGS: [&str; 4] = [
    "whole",
    "with the domain alone",
    "alone",
    "with every claim of the certificate",
];

/// Every claim of the certificate, once each.
fn every_claim(v: &Value) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Value> = Vec::new();
    for row in replay::ROWS {
        if let Some((_, b)) = body(v, row) {
            for c in b["cert"]["claims"].as_array().into_iter().flatten() {
                if seen.insert(c.to_string()) {
                    out.push(c.clone());
                }
            }
        }
    }
    out
}

/// Why the replay refuses the row `row` of `v` in a setting, if it does.
/// (A replay that panics refuses nothing: said so.)
fn refused(
    v: &Value,
    row: &str,
    setting: usize,
    every: &[Value],
    s: &mut replay::Session,
) -> Option<String> {
    let mut v = v.clone();
    match setting {
        1 => withhold_all_but(&mut v, &[row, "domain"]),
        2 => withhold_all_but(&mut v, &[row]),
        3 if body(&v, row).is_some() => {
            body_mut(&mut v, row)["cert"]["claims"] = Value::Array(every.to_vec());
        }
        _ => {}
    }
    let replayed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        replay::replay_in(&v, s, Some(row))
    }));
    match replayed {
        Err(_) => {
            *s = replay::Session::default();
            None
        }
        Ok(Err(e)) => Some(format!("unreadable: {e}")),
        Ok(Ok(r)) => r.refusal(),
    }
}

#[derive(Default, Clone)]
struct Tally {
    generated: usize,
    killed: usize,
    /// Refused for being unreadable alone (an operator's own fault).
    unreadable: usize,
    accepted: usize,
    benign: usize,
    benign_accepted: usize,
}

struct Accepted {
    src: String,
    row: &'static str,
    op: &'static str,
    what: String,
    setting: &'static str,
}

#[derive(Default)]
struct Run {
    functions: usize,
    /// Certificates the strict replay refuses as they are: not mutated.
    refused: Vec<String>,
    tally: BTreeMap<(&'static str, &'static str), Tally>,
    accepted: Vec<Accepted>,
    /// (row, status, covers) of the genuine rows.
    shapes: BTreeMap<(&'static str, String, String), usize>,
    /// (seconds, certificate, mutants) of each certificate.
    took: Vec<(f64, String, usize)>,
}

fn covers_kind(c: &Value) -> String {
    match c {
        Value::String(s) => s.clone(),
        Value::Object(o) => o.keys().next().cloned().unwrap_or_default(),
        _ => "?".into(),
    }
}

/// The mutants of one function's certificate, replayed.
fn mutate(src: &str, unit: TrigUnit, other: Option<&Value>, run: &mut Run) {
    let Some(base) = certificate(src, unit) else {
        return;
    };
    let mut session = replay::Session::default();
    let label = format!("{src} [{unit:?}]");
    match replay::replay_in(&base, &mut session, None) {
        Err(e) => {
            run.refused.push(format!("{label}: {e}"));
            return;
        }
        Ok(r) => {
            if let Some(why) = r.refusal() {
                run.refused.push(format!("{label}: {why}"));
                return;
            }
        }
    }
    run.functions += 1;
    let every = every_claim(&base);
    // (MUTANTS_SLOW=seconds: say each replay of a mutant that takes longer.)
    let slow: Option<f64> = std::env::var("MUTANTS_SLOW")
        .ok()
        .and_then(|v| v.parse().ok());
    let only = std::env::var("MUTANTS_ROW").ok();
    for row in replay::ROWS {
        if only.as_ref().is_some_and(|r| r != row) {
            continue;
        }
        let Some((status, b)) = body(&base, row) else {
            continue;
        };
        *run.shapes
            .entry((row, status.to_string(), covers_kind(&b["cert"]["covers"])))
            .or_insert(0) += 1;
        mutants_of(&base, row, other, &mut |m: Mutant| {
            let t = run.tally.entry((row, m.op)).or_default();
            // Killed: refused in every setting. (The domain row alone and
            // with itself are one setting.)
            // (A row given every claim of the certificate has its own
            // back: no setting for the mutant that only withholds them.)
            let settings: &[usize] = match (row, m.op) {
                ("domain", "claims withheld") => &[0, 2],
                (_, "claims withheld" | "kink claim changed") => &[0, 1, 2],
                ("domain", _) => &[0, 2, 3],
                _ => &[0, 1, 2, 3],
            };
            let mut accepted_in = None;
            let mut unreadable = true;
            for &s in settings {
                let t1 = std::time::Instant::now();
                match refused(&m.v, row, s, &every, &mut session) {
                    None => {
                        accepted_in = Some(SETTINGS[s]);
                        break;
                    }
                    Some(why) => unreadable &= why.starts_with("unreadable"),
                }
                if let Some(slow) = slow
                    && t1.elapsed().as_secs_f64() > slow
                {
                    eprintln!(
                        "slow: {label}: {row} / {}: {} ({}): {:.1} s",
                        m.op,
                        m.what,
                        SETTINGS[s],
                        t1.elapsed().as_secs_f64()
                    );
                }
            }
            if m.benign.is_some() {
                t.benign += 1;
                t.benign_accepted += usize::from(accepted_in.is_some());
                return;
            }
            t.generated += 1;
            match accepted_in {
                None => {
                    t.killed += 1;
                    t.unreadable += usize::from(unreadable);
                }
                Some(setting) => {
                    t.accepted += 1;
                    run.accepted.push(Accepted {
                        src: label.clone(),
                        row,
                        op: m.op,
                        what: m.what,
                        setting,
                    });
                }
            }
        });
    }
}

/// Mutates every function of `fs` (in parallel), prints the table, and
/// returns the run.
fn run(name: &str, fs: Vec<(String, TrigUnit)>) -> Run {
    let filter = std::env::var("MUTANTS_FILTER").ok();
    // (MUTANTS_SKIP=a;b: those sources left out, said so in the report.)
    let skip: Vec<String> = std::env::var("MUTANTS_SKIP")
        .map(|v| v.split(';').map(str::to_string).collect())
        .unwrap_or_default();
    let fs: Vec<(String, TrigUnit)> = fs
        .into_iter()
        .filter(|(s, _)| filter.as_ref().is_none_or(|f| s.contains(f.as_str())))
        .filter(|(s, _)| {
            let out = skip.contains(s);
            if out {
                println!("skipped (MUTANTS_SKIP): {s}");
            }
            !out
        })
        .collect();
    // (Each function's "other", for its rows: the one two along.)
    let threads = certificates::threads();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..threads {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(sc, || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((s, u)) = fs.get(i) else { break };
                        let (os, ou) = &fs[(i + 2) % fs.len()];
                        let other = (os != s).then(|| certificate(os, *ou)).flatten();
                        let mut one = Run::default();
                        let t0 = std::time::Instant::now();
                        mutate(s, *u, other.as_ref(), &mut one);
                        let mutants: usize =
                            one.tally.values().map(|t| t.generated + t.benign).sum();
                        let secs = t0.elapsed().as_secs_f64();
                        if std::env::var_os("MUTANTS_PROGRESS").is_some() {
                            eprintln!(
                                "{}/{}: {s} [{u:?}]: {mutants} mutants, {secs:.1} s",
                                i + 1,
                                fs.len()
                            );
                        }
                        one.took.push((secs, format!("{s} [{u:?}]"), mutants));
                        done.lock().unwrap().push((i, one));
                    }
                })
                .expect("a mutation thread");
        }
    });
    let mut done = done.into_inner().unwrap();
    done.sort_by_key(|(i, _)| *i);
    let mut all = Run::default();
    for (_, one) in done {
        all.functions += one.functions;
        all.refused.extend(one.refused);
        all.accepted.extend(one.accepted);
        all.took.extend(one.took);
        for (k, t) in one.tally {
            let a = all.tally.entry(k).or_default();
            a.generated += t.generated;
            a.killed += t.killed;
            a.unreadable += t.unreadable;
            a.accepted += t.accepted;
            a.benign += t.benign;
            a.benign_accepted += t.benign_accepted;
        }
        for (k, n) in one.shapes {
            *all.shapes.entry(k).or_insert(0) += n;
        }
    }
    report(name, &all);
    all
}

fn report(name: &str, r: &Run) {
    println!(
        "\n{} certificates mutated ({} refused as they are, not mutated)",
        r.functions,
        r.refused.len()
    );
    for f in &r.refused {
        println!("  refused as it is: {f}");
    }
    let mut took: Vec<&(f64, String, usize)> = r.took.iter().collect();
    took.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (secs, label, mutants) in took.into_iter().take(5) {
        println!("slowest: {label}: {mutants} mutants, {secs:.1} s");
    }
    println!("\nthe genuine rows: row, status, covers: how many");
    for ((row, status, covers), n) in &r.shapes {
        println!("  {row:<13} {status:<10} {covers:<7} {n:>6}");
    }
    println!(
        "\n{:<13} {:<32} {:>9} {:>8} {:>9} {:>11} {:>20}",
        "row", "operator", "generated", "killed", "accepted", "(unreadable)", "content-preserving"
    );
    let mut total = Tally::default();
    for ((row, op), t) in &r.tally {
        println!(
            "{row:<13} {op:<32} {:>9} {:>8} {:>9} {:>11} {:>12} of {:<5}",
            t.generated, t.killed, t.accepted, t.unreadable, t.benign_accepted, t.benign
        );
        total.generated += t.generated;
        total.killed += t.killed;
        total.accepted += t.accepted;
        total.unreadable += t.unreadable;
        total.benign += t.benign;
        total.benign_accepted += t.benign_accepted;
    }
    println!(
        "{:<13} {:<32} {:>9} {:>8} {:>9} {:>11} {:>12} of {:<5}",
        "total",
        "",
        total.generated,
        total.killed,
        total.accepted,
        total.unreadable,
        total.benign_accepted,
        total.benign
    );
    println!("(content-preserving: accepted of generated; none is asked to be refused)");
    // A few of each accepted kind.
    let mut shown: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for a in &r.accepted {
        let n = shown.entry((a.row, a.op)).or_insert(0);
        *n += 1;
        if *n <= 4 {
            println!(
                "ACCEPTED {} / {} ({}): {}: {}",
                a.row, a.op, a.setting, a.src, a.what
            );
        }
    }
    if let Ok(path) = std::env::var("MUTANTS_LOG") {
        // (One file per run: `file.name`, so that tests running at once
        // don't truncate each other's.)
        let path = format!("{path}.{name}");
        let mut text = String::new();
        for a in &r.accepted {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                a.src, a.row, a.op, a.what, a.setting
            ));
        }
        std::fs::write(&path, text).unwrap_or_else(|e| panic!("MUTANTS_LOG={path}: {e}"));
    }
}

/// A fixed few, one of each shape of row, and kinks: seconds.
const FEW: &[&str] = &[
    "x^3-2x+1/(x-1)",
    "sin(x)",
    "tan(x)",
    "sqrt(x)",
    "x/x",
    "(x^2+x+1)/(3x)",
    "atan(x)",
    "abs(x-1)",
    // Kinks: at no double, joined (an inflection), a vertical tangent no
    // double holds, a max of three, a cusp (f′ changing sign with no zero:
    // the kink's claims a gap, review 22's audit).
    "abs(x-0.1)",
    "x*abs(x)",
    "(x-0.1)^(1/3)",
    "max(x^2,2-x^2,-5)",
    "(x-1)^(2/3)",
];

#[test]
fn a_fixed_few_certificates_mutated_are_refused() {
    let fs = FEW
        .iter()
        .map(|s| (s.to_string(), TrigUnit::Radians))
        .collect();
    let r = run("a_fixed_few", fs);
    assert!(r.refused.is_empty(), "{:#?}", r.refused);
    assert_eq!(r.functions, FEW.len());
    assert!(
        r.accepted.is_empty(),
        "{} mutants accepted",
        r.accepted.len()
    );
}

/// A partial list complete within a window cannot omit its zeros there.
/// The checker already refuses these false rows; the mutation harness
/// must ask it to do so, rather than exempt them as content preserving.
#[test]
fn window_completeness_is_not_exempted_as_a_points_only_list() {
    let mut v = certificate("x^2-1", TrigUnit::Radians).unwrap();
    let mut b = body(&v, "x_intercepts").unwrap().1.clone();
    b["cert"]["covers"] = json!({"Window": {"a": -2.0, "b": 2.0}});
    v["x_intercepts"] = json!({"Partial": b});
    let genuine = replay::replay(&v).unwrap();
    assert!(genuine.row_problems().is_empty());
    let mut checked = 0;
    mutants_of(&v, "x_intercepts", None, &mut |m| {
        if !matches!(
            m.op,
            "list emptied" | "item removed" | "two places listed as one"
        ) {
            return;
        }
        assert!(m.benign.is_none(), "{}: {}", m.op, m.what);
        let r = replay::replay(&m.v).unwrap();
        assert!(
            r.rows
                .iter()
                .any(|r| r.name == "x_intercepts" && !r.problems.is_empty())
        );
        checked += 1;
    });
    assert!(checked >= 4, "{checked} destructive window mutants checked");
    body_mut(&mut v, "x_intercepts")["cert"]["covers"] = json!("Points");
    let mut exempted = 0;
    mutants_of(&v, "x_intercepts", None, &mut |m| {
        if m.op == "list emptied" {
            assert_eq!(m.benign, Some(FEWER));
            exempted += 1;
        }
    });
    assert_eq!(exempted, 1);
}

/// A window from a listed place, the place left out, is refused in every
/// setting: the zero, turn or inflection on the window's end is in it.
/// (Before review 21's audit the replay never derived a turn on a
/// window's end whose far side no claim signed: x² − 1's "no extrema for
/// 0 ≤ x ≤ 2" and x³'s "no inflections for 0 ≤ x ≤ 2" passed.)
#[test]
fn a_window_from_a_place_left_out_is_refused() {
    for (src, row) in [
        ("x^2-1", "extrema"),
        ("x^3", "inflections"),
        ("x^2-1", "x_intercepts"),
        ("x^3-3x", "extrema"),
    ] {
        let v = certificate(src, TrigUnit::Radians).unwrap();
        let every = every_claim(&v);
        let mut checked = 0;
        mutants_of(&v, row, None, &mut |m| {
            if m.op != "window from a place, the place left out" {
                return;
            }
            assert!(m.benign.is_none());
            let mut session = replay::Session::default();
            for (s, setting) in SETTINGS.iter().enumerate() {
                assert!(
                    refused(&m.v, row, s, &every, &mut session).is_some(),
                    "{src}: {row}: {} accepted ({setting})",
                    m.what,
                );
            }
            checked += 1;
        });
        assert!(checked >= 2, "{src}: {row}: {checked} mutants");
    }
}

/// True claims in the place of those about a place, the place left out of
/// the row: a gap from a zero, turn or inflection on a window's end, two
/// gaps meeting at one inside the region, a gap over a cusp. Every premise
/// of each mutant is true (the operators make none the replay's own check
/// doesn't prove), and each is refused in every setting. Before review
/// 22's audit the replay accepted every window-end mutant of a turn or an
/// inflection here (R22-M-01) and every cusp mutant but the gap over the
/// kink box alone.
#[test]
fn true_claims_in_the_place_of_a_place_are_refused() {
    for (src, row, op, least) in [
        ("x^2-1", "extrema", "window end behind a gap", 4),
        ("x^2-1", "extrema", "place between two gaps", 1),
        ("x^3", "inflections", "window end behind a gap", 4),
        ("x^3-3x", "extrema", "window end behind a gap", 8),
        ("x^2-1", "x_intercepts", "window end behind a gap", 8),
        ("x^2-1", "x_intercepts", "place between two gaps", 2),
        ("(x-1)^(2/3)", "extrema", "kink in a gap", 3),
        ("(x-1)^(2/3)", "monotonicity", "kink in a gap", 3),
    ] {
        let v = certificate(src, TrigUnit::Radians).unwrap();
        let every = every_claim(&v);
        let mut checked = 0;
        mutants_of(&v, row, None, &mut |m| {
            if m.op != op {
                return;
            }
            assert!(m.benign.is_none());
            let mut session = replay::Session::default();
            let r = replay::replay_in(&m.v, &mut session, Some(row)).unwrap();
            assert!(
                r.claims.iter().all(|c| matches!(
                    c.outcome.class,
                    replay::Class::Strong | replay::Class::Weak
                )),
                "{src}: {row}: {}: a premise is not true",
                m.what
            );
            for (s, setting) in SETTINGS.iter().enumerate() {
                assert!(
                    refused(&m.v, row, s, &every, &mut session).is_some(),
                    "{src}: {row}: {} accepted ({setting})",
                    m.what,
                );
            }
            checked += 1;
        });
        assert!(checked >= least, "{src}: {row}: {op}: {checked} mutants");
    }
}

// ------------------------------------------------------------ reviewed

/// Mutants a long run accepts, each reviewed: the mutated row is a TRUE
/// statement about f (the replay proved it), so accepting it is right. The
/// long runs fail on any accepted mutant not here, and on any entry here
/// that the run did not accept (stale: the table can't rot). An entry names
/// the run, the function with its trig unit, the row, the operator and the
/// mutant's `what`, and says why the statement is true.
///
/// `what` is matched exactly. Where it holds a double's digits (a number
/// moved, an enclosure borrowed from the next item) that is on purpose: the
/// statement is true of those numbers, and a changed enclosure is another
/// statement that has not been reviewed. Where it names only a row of
/// another function it is stable text, and the borrowed row is the one
/// the entry's reason was written for. The setting a mutant was first
/// accepted in is not part of the key: it depends on the order of the
/// settings, not on the statement.
struct Known {
    /// "corpus" or "pool".
    run: &'static str,
    src: &'static str,
    row: &'static str,
    op: &'static str,
    what: &'static str,
    why: &'static str,
}

// (Reasons shared by several entries.)
const ZERO_LIMIT: &str = "f → 0 on each side, as the borrowed row's enclosures (each holding 0, \
                          looks like 0) say";
const NO_ZEROS_NO_POLES: &str = "f is continuous and positive everywhere (≥ 1 or ≥ 10¹³−1), so \
                                 \"none\" is true";
const CSCH_LIKE_1_OVER: &str = "csch(x−1000) and 1/(x−c) both have range ℝ∖{0} (pieces open at \
                                an end enclosing 0) and tend to 0 on each side";

#[rustfmt::skip]
const KNOWN: &[Known] = &[
    // ---- the corpus: rows borrowed from another function that are true of f
    Known { run: "corpus", src: "10^16*(1.0000000000000001-1)+x [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=10^16*(1.0000000000000001-0.99999999999999999)+x""#,
        why: "f(0) = 1, which the borrowed enclosure [0, 3.33] holds" },
    Known { run: "corpus", src: "10^17*(0.1+0.2-0.3)+x [Radians]", row: "x_intercepts",
        op: "row of another function",
        what: r#"the x_intercepts row of "y=1.0000000000000001*x-1*x""#,
        why: "f = x (the constant is 0 exactly): zero at 0 alone; the borrowed row lists 0 as a partial list over points" },
    Known { run: "corpus", src: "10^17*(0.1+0.2-0.3)+x [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=1.0000000000000001*x-1*x""#,
        why: "f(0) = 0, the borrowed row's value exactly" },
    Known { run: "corpus", src: "10^17*(0.1+0.2-0.3)+x [Radians]", row: "y_intercept",
        op: "number moved far",
        what: "value: [-1.1102230246251565e1, 5.551115123125783e0] moved by 1",
        why: "f(0) = 0 exactly, and the moved enclosure [-10.1, 6.55] still holds it" },
    Known { run: "corpus", src: "1/(x-1000000000000001) [Radians]", row: "horizontal",
        op: "row of another function",
        what: r#"the horizontal row of "y=csch(x-1000)""#,
        why: ZERO_LIMIT },
    Known { run: "corpus", src: "1/(x-1000000000000001) [Radians]", row: "range",
        op: "row of another function",
        what: r#"the range row of "y=csch(x-1000)""#,
        why: CSCH_LIKE_1_OVER },
    Known { run: "corpus", src: "cos(x)^2/cos(x)^2 [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=2*exp(-(x-1522756)^2/(1/1000000))+exp(-x^2)""#,
        why: "f(0) = 1, which the borrowed [1, 1.0000000000000002] holds" },
    Known { run: "corpus", src: "ln(exp(1000000000000000)^4)-4000000000000000 [Radians]", row: "extrema",
        op: "row of another function",
        what: r#"the extrema row of "y=cos(x)^2/cos(x)^2""#,
        why: "f is the zero function; the borrowed row is no extremum within [-16, 16], as the genuine row (none) says" },
    Known { run: "corpus", src: "ln(exp(1000000000000000)^4)-4000000000000000 [Radians]", row: "inflections",
        op: "row of another function",
        what: r#"the inflections row of "y=cos(x)^2/cos(x)^2""#,
        why: "f is the zero function; no inflection within [-16, 16], as the genuine row (none) says" },
    Known { run: "corpus", src: "ln(exp(1000000000000000)^4)-4000000000000000 [Radians]", row: "parity",
        op: "row of another function",
        what: r#"the parity row of "y=tan(x)""#,
        why: "the zero function on the whole line is odd (f(−x) = −f(x) = 0), as well as even" },
    Known { run: "corpus", src: "sin(x)+10000000000000 [Radians]", row: "vertical",
        op: "row of another function",
        what: r#"the vertical row of "y=exp(-(x-100)^2)+exp(-x^2)""#,
        why: "f is continuous on the line: no vertical asymptote, as the borrowed \"none\" says" },
    Known { run: "corpus", src: "sin(x)+10000000000000 [Radians]", row: "x_intercepts",
        op: "row of another function",
        what: r#"the x_intercepts row of "y=exp(-(x-100)^2)+exp(-x^2)""#,
        why: "sin x + 10¹³ ≥ 10¹³ − 1 > 0: no zeros, as the borrowed \"none\" says" },
    Known { run: "corpus", src: "sqrt(exp(-(x+800)))*exp((x+800)/2) [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=exp(-1000)*(x+1)*exp(500)*exp(500)""#,
        why: "f(0) = 1, which the borrowed [0, 1.9e108] holds" },
    Known { run: "corpus", src: "x^(0.1+0.9) [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=10^16*(1.0000000000000001-1)+x""#,
        why: "f(0) = 0, which the borrowed [0, 2.22] holds" },
    Known { run: "corpus", src: "(x-1000000000)/(x-1000000000.0625) [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=sqrt(exp(-(x+800)))*exp((x+800)/2)""#,
        why: "f(0) = 0.99999999993…, which the borrowed [0, 1.16e12] holds" },
    Known { run: "corpus", src: "(x-1000000)^2+1/10000000+(x/1000000000)^2 [Radians]", row: "inflections",
        op: "row of another function",
        what: r#"the inflections row of "y=atan(tan(x))+1""#,
        why: "f is a quadratic with positive leading coefficient (f″ > 0): no inflection within [-16, 16], as the borrowed row (none over that window) says" },
    Known { run: "corpus", src: "(x-1000000)^2+1+(x/1000000000)^2 [Radians]", row: "inflections",
        op: "row of another function",
        what: r#"the inflections row of "y=atan(tan(x))+1""#,
        why: "f is a quadratic with positive leading coefficient (f″ > 0): no inflection within [-16, 16], as the borrowed row (none over that window) says" },

    // ---- the pool
    // (atan(1/x)'s monotone rows in degrees and grads, alone and borrowed
    // by csch(x), were here as partial rows. The certifier now certifies
    // them complete (p12-cert), which neither its claims withheld nor
    // atan's claims on csch can carry: both are refused.)
    Known { run: "pool", src: "csch(x) [Degrees]", row: "horizontal",
        op: "row of another function",
        what: r#"the horizontal row of "y=atan(1/x)""#,
        why: ZERO_LIMIT },
    Known { run: "pool", src: "csch(x) [Grads]", row: "horizontal",
        op: "row of another function",
        what: r#"the horizontal row of "y=atan(1/x)""#,
        why: ZERO_LIMIT },
    Known { run: "pool", src: "csch((x/1000)) [Radians]", row: "horizontal",
        op: "row of another function",
        what: r#"the horizontal row of "y=atan(1/(x-1000))""#,
        why: ZERO_LIMIT },
    Known { run: "pool", src: "(x-1)*(x-2)*(x+3) [Radians]", row: "monotonicity",
        op: "row of another function",
        what: r#"the monotonicity row of "y=((x-1)*(x-2)*(x+3))+0.0000001""#,
        why: "adding a constant leaves f′ = 3x²−7 and so the turns at ±√(7/3), which the borrowed end enclosures hold" },
    Known { run: "pool", src: "(e^x-x)+0.0000001 [Radians]", row: "monotonicity",
        op: "row of another function",
        what: r#"the monotonicity row of "y=e^(x/1000)-(x/1000)""#,
        why: "f′ = e^x − 1 turns at 0, decreasing before and increasing after, as the borrowed row (turn enclosure [-1.8e-12, 1.4e-12] holds 0) says" },
    Known { run: "pool", src: "(sin(x)^2+1)+0.0000001 [Radians]", row: "x_intercepts",
        op: "row of another function",
        what: r#"the x_intercepts row of "y=sin((x/1000))^2+1""#,
        why: NO_ZEROS_NO_POLES },
    Known { run: "pool", src: "(sin(x)^2+1)+0.0000001 [Radians]", row: "vertical",
        op: "row of another function",
        what: r#"the vertical row of "y=sin((x/1000))^2+1""#,
        why: NO_ZEROS_NO_POLES },
    Known { run: "pool", src: "sqrt((x/1000)) [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=e^(x-1000)""#,
        why: "f(0) = 0, which the borrowed [0, 5e-324] holds" },
    Known { run: "pool", src: "(x/1000)^4-(x/1000)^2 [Radians]", row: "y_intercept",
        op: "row of another function",
        what: r#"the y_intercept row of "y=(x-1000)*e^(x-1000)""#,
        why: "f(0) = 0, which the borrowed [-4.9e-321, 0] holds" },
    // Exact matches on the digits below: the statement is true of these numbers.
    Known { run: "pool", src: "(1/x)+0.0000001 [Radians]", row: "horizontal",
        op: "number moved to the next item's",
        what: r#"[0].y: [9.999999999999998e-8, 1.0000000000000001e-7] to {"hi":1.0000000000000001e-7,"lo":1e-7}"#,
        why: "the limit is 10⁻⁷ exactly, which lies between the double 1e-7 (just below it) and the one above, 1.0000000000000001e-7: the narrowed enclosure holds it" },
    Known { run: "pool", src: "(1/(x-2))+0.0000001 [Radians]", row: "horizontal",
        op: "number moved to the next item's",
        what: r#"[0].y: [9.999999999999998e-8, 1.0000000000000001e-7] to {"hi":1.0000000000000001e-7,"lo":1e-7}"#,
        why: "the limit is 10⁻⁷ exactly, which lies between the double 1e-7 (just below it) and the one above, 1.0000000000000001e-7: the narrowed enclosure holds it" },
    Known { run: "pool", src: "1/((x-1000)^2+(x-1000)+1) [Radians]", row: "inflections",
        op: "number moved far",
        what: "[0].x: [9.99e2, 9.99e2] moved by 1",
        why: "the list is partial, with inflections at 999 and 1000 and f = 1 at both (f″ ∝ u(u+1), u = x−1000): item 0 moved onto (1000, 1), the other inflection" },
];

/// Fails on an accepted mutant not in [`KNOWN`] and, in a run unfiltered
/// (no `MUTANTS_FILTER`, `MUTANTS_ROW`, `MUTANTS_SKIP`, `MUTANTS_ITEMS`),
/// on an entry of this run that was not accepted.
fn assert_only_known(name: &str, r: &Run) {
    let mine = || KNOWN.iter().filter(|k| k.run == name);
    let is = |a: &Accepted, k: &Known| {
        a.src == k.src && a.row == k.row && a.op == k.op && a.what == k.what
    };
    let new: Vec<String> = r
        .accepted
        .iter()
        .filter(|a| !mine().any(|k| is(a, k)))
        .map(|a| format!("{}\t{}\t{}\t{}\t{}", a.src, a.row, a.op, a.what, a.setting))
        .collect();
    let filtered = ["FILTER", "ROW", "SKIP", "ITEMS"]
        .iter()
        .any(|v| std::env::var_os(format!("MUTANTS_{v}")).is_some());
    let stale: Vec<String> = if filtered {
        Vec::new()
    } else {
        mine()
            .filter(|k| !r.accepted.iter().any(|a| is(a, k)))
            .map(|k| format!("{}\t{}\t{}\t{}", k.src, k.row, k.op, k.what))
            .collect()
    };
    let known = r
        .accepted
        .iter()
        .filter(|a| mine().any(|k| is(a, k)))
        .count();
    println!(
        "{name}: {known} accepted mutants reviewed true ({} entries), {} new, {} stale",
        mine().count(),
        new.len(),
        stale.len()
    );
    for k in mine() {
        println!(
            "  reviewed true: {} / {} / {}: {}: {}",
            k.src, k.row, k.op, k.what, k.why
        );
    }
    assert!(
        new.is_empty() && stale.is_empty(),
        "{} mutants accepted that are not in KNOWN (source, row, operator, what, setting):\n{}\n\
         {} entries of KNOWN not accepted this run (stale):\n{}",
        new.len(),
        new.join("\n"),
        stale.len(),
        stale.join("\n")
    );
}

#[test]
#[ignore = "long: every mutant of the corpus's certificates"]
fn the_corpus_mutated_is_refused() {
    let r = run("corpus", certificates::corpus());
    assert!(r.functions >= 150, "only {} mutated", r.functions);
    assert_only_known("corpus", &r);
}

#[test]
#[ignore = "long: every mutant of the pool's certificates"]
fn the_pool_mutated_is_refused() {
    let mut fs = certificates::units();
    fs.extend(certificates::pool());
    let r = run("pool", fs);
    assert!(r.functions >= 450, "only {} mutated", r.functions);
    assert_only_known("pool", &r);
}
