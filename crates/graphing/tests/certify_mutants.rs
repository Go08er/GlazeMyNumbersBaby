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
//! `MUTANTS_FILTER=text` keeps the functions whose source contains `text`;
//! `MUTANTS_LOG=file` writes every accepted mutant (tab separated: source,
//! row, operator, what, setting); `MUTANTS_ROW=name` keeps one row's;
//! `MUTANTS_PROGRESS=1` says each certificate as it is done.
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
const ON_F_ITSELF: &str = "the row is checked on f itself (each piece of the domain and each of \
                           its ends; f at 0): it rests on no claim, and stands without them";
const BY_ANOTHER_ROW: &str = "not periodic follows from another row alone (a domain no shift \
                              maps onto itself, a monotone piece reaching ±∞): the row's own \
                              claims are not what it rests on";
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
        let fewer = (!certified).then_some(FEWER);
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
                    format!("a piece made up above the range, [{}, {}]", top + 1.0, top + 2.0),
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
        if lo == hi {
            push(
                "number moved a double",
                format!("{name}: {lo:e} to the double above"),
                None,
                &set(p, enc(lo.next_up(), lo.next_up())),
            );
            push(
                "number moved a double",
                format!("{name}: {lo:e} to the double below"),
                None,
                &set(p, enc(lo.next_down(), lo.next_down())),
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
            None,
            &set(p, enc(lo + 1.0, hi + 1.0)),
        );
        push(
            "enclosure widened",
            format!("{name}: [{lo:e}, {hi:e}] by a double each way"),
            Some(WIDER),
            &set(p, enc(lo.next_down(), hi.next_up())),
        );
        // The next proven feature: the same field of the next item.
        if certified
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
                    format!("{}: to {x} ({})", show(&p), if closed { "closed" } else { "open" }),
                    None,
                    &set(&p, json!({"At": {"x": enc(x, x), "closed": closed}})),
                );
            }
        }
    }

    // ---- directions, kinds, sides, parities
    for set_of in [&DIRECTIONS[..], &KINDS[..], &SIDES[..], &PARITIES[..]] {
        for p in sampled_paths(&value, &|n| n.as_str().is_some_and(|s| set_of.contains(&s))) {
            let was = at(&mut value.clone(), &p).as_str().unwrap_or("").to_string();
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
                (!certified).then_some(FEWER),
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
                (!certified).then_some(FEWER),
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
        format!("{status} called {}", if certified { "Partial" } else { "Certified" }),
        certified.then_some(WEAKER_STATUS),
        &|v: &mut Value| {
            let b = body_mut(v, row).clone();
            let mut m = serde_json::Map::new();
            m.insert(
                if certified { "Partial" } else { "Certified" }.into(),
                b,
            );
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
        let smaller = (kind == "Window" && was_region).then_some(SMALLER_SCOPE);
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
            scopes.push((
                format!("{kind} [{}, {}] (wider)", wa - 1.0, wb + 1.0),
                over(kind, (wa - 1.0, wb + 1.0)),
                None,
            ));
            scopes.push((
                format!("{kind} [{}, {}] (moved)", wa + q, wb + q),
                over(kind, (wa + q, wb + q)),
                None,
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
    let rests_on_no_claim = match row {
        "domain" if no_families => Some(ON_F_ITSELF),
        "y_intercept" if value.is_null() => Some(ON_F_ITSELF),
        "period" if value.is_null() && ((no_families && not_the_line) || reaching) => {
            Some(BY_ANOTHER_ROW)
        }
        _ => None,
    };
    if b["cert"]["claims"].as_array().is_some_and(|c| !c.is_empty()) {
        push(
            "claims withheld",
            "the row as it is, with none of its claims".into(),
            rests_on_no_claim,
            &|v: &mut Value| body_mut(v, row)["cert"]["claims"] = json!([]),
        );
    }

    // ---- the same row of another function
    if let Some(o) = other
        && let Some((_, ob)) = body(o, row)
        && (ob["value"] != value || ob["cert"]["covers"] != covers)
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
        3 => {
            if body(&v, row).is_some() {
                body_mut(&mut v, row)["cert"]["claims"] = Value::Array(every.to_vec());
            }
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
                (_, "claims withheld") => &[0, 1, 2],
                ("domain", _) => &[0, 2, 3],
                _ => &[0, 1, 2, 3],
            };
            let mut accepted_in = None;
            let mut unreadable = true;
            for &s in settings {
                match refused(&m.v, row, s, &every, &mut session) {
                    None => {
                        accepted_in = Some(SETTINGS[s]);
                        break;
                    }
                    Some(why) => unreadable &= why.starts_with("unreadable"),
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
fn run(fs: Vec<(String, TrigUnit)>) -> Run {
    let filter = std::env::var("MUTANTS_FILTER").ok();
    let fs: Vec<(String, TrigUnit)> = fs
        .into_iter()
        .filter(|(s, _)| filter.as_ref().is_none_or(|f| s.contains(f.as_str())))
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
    report(&all);
    all
}

fn report(r: &Run) {
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

/// A fixed few, one of each shape of row: seconds.
const FEW: &[&str] = &[
    "x^3-2x+1/(x-1)",
    "sin(x)",
    "tan(x)",
    "sqrt(x)",
    "x/x",
    "(x^2+x+1)/(3x)",
    "atan(x)",
    "abs(x-1)",
];

#[test]
fn a_fixed_few_certificates_mutated_are_refused() {
    let fs = FEW
        .iter()
        .map(|s| (s.to_string(), TrigUnit::Radians))
        .collect();
    let r = run(fs);
    assert!(r.refused.is_empty(), "{:#?}", r.refused);
    assert_eq!(r.functions, FEW.len());
    assert!(
        r.accepted.is_empty(),
        "{} mutants accepted",
        r.accepted.len()
    );
}

#[test]
#[ignore = "long: every mutant of the corpus's certificates"]
fn the_corpus_mutated_is_refused() {
    let r = run(certificates::corpus());
    assert!(r.functions >= 150, "only {} mutated", r.functions);
    assert!(
        r.accepted.is_empty(),
        "{} mutants accepted",
        r.accepted.len()
    );
}

#[test]
#[ignore = "long: every mutant of the pool's certificates"]
fn the_pool_mutated_is_refused() {
    let mut fs = certificates::units();
    fs.extend(certificates::pool());
    let r = run(fs);
    assert!(r.functions >= 450, "only {} mutated", r.functions);
    assert!(
        r.accepted.is_empty(),
        "{} mutants accepted",
        r.accepted.len()
    );
}
