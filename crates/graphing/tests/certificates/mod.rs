//! The functions whose certificates the replay's tests go through: the
//! certify corpus (its fixtures and the review rounds' functions) and the
//! pool (the kgf set in degrees and grads, and the analysis sweep's base
//! functions, shifted, offset, scaled and stretched). Shared by
//! `certify_replay.rs` and `certify_mutants.rs`.

#![allow(dead_code)]

use graphing::functions::TrigUnit;

const CASES: &str = include_str!("../fixtures/certify/cases.tsv");
/// The certify corpus's test file (its REVIEW and issue tables are read).
pub const CORPUS: &str = include_str!("../certify_corpus.rs");

/// The review rounds' functions, read from `certify_corpus.rs`'s REVIEW
/// table (the first string of each entry).
pub fn review() -> Vec<String> {
    let start = CORPUS
        .find("const REVIEW")
        .expect("REVIEW in certify_corpus.rs");
    let body = &CORPUS[start..];
    let end = body.find("\n];").expect("end of REVIEW");
    let lines: Vec<&str> = body[..end].lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() == "(" {
            let s = lines[i + 1].trim();
            if let Some(s) = s.strip_prefix('"').and_then(|s| s.strip_suffix("\",")) {
                out.push(s.to_string());
            }
        }
    }
    out
}

pub fn corpus() -> Vec<(String, TrigUnit)> {
    let mut out: Vec<(String, TrigUnit)> = Vec::new();
    for line in CASES.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        out.push((f[2].to_string(), TrigUnit::Radians));
    }
    for s in review() {
        out.push((s, TrigUnit::Radians));
    }
    out
}

/// The sweep's base functions (`examples/sweep.rs`), alone and shifted,
/// offset, scaled and stretched.
const BASES: &[&str] = &[
    "x^2",
    "x^3-2x+1/(x-1)",
    "sin(x)",
    "tan(x)",
    "1/x",
    "ln(x)",
    "log(x)",
    "sqrt(x)",
    "e^x",
    "(x^2-1)/(x-1)",
    "x^0.9",
    "x/ln(x)",
    "1/ln(x)",
    "sin(x^2)",
    "floor(x)",
    "atan(x)",
    "x*e^(-x)",
    "sin(x)/x",
    "x*sin(x)",
    "1/(x-2)",
    "1/(x^2-4)",
    "cot(x)",
    "csc(x)",
    "sec(x)",
    "coth(x)",
    "csch(x)",
    "atan(1/x)",
    "e^(-1/x^2)",
    "e^(ln(x))",
    "1/(1+e^x)",
    "x*ln(x)",
    "x/x",
    "1/x^2",
    "tan(x)^2",
    "1/sin(x)",
    "ln(abs(x))",
    "sqrt(1/x)",
    "x^x",
    "x^-0.5",
    "x^-0.0001",
    "e^(-x^2)",
    "x^3-3x",
    "1/(x^2+1)",
    "sqrt(x^2+1)",
    "abs(x)",
    "x*abs(x)",
    "(x-1)*(x+2)",
    "(x-1)*(x-2)*(x+3)",
    "(x-1)^2*(x+1)",
    "(x-0.5)*(x+0.25)*(x-3)*(x+4)",
    "(x^2-1)*(x^2-4)*(x-0.5)",
    "(x-1)/((x-2)*(x+3))",
    "(x^2-4)/(x-2)",
    "(x+1)/(x^2-1)",
    "x^2/(x^2-1)",
    "(x^3-1)/(x-1)",
    "sin(x)+x/10",
    "ln(x^2+1)",
    "atan(x)+0.000000001",
    "1/(x^2+1)-0.5",
    "x^4-x^2",
    "x*e^x",
    "e^x-x",
    "x^(1/3)",
    "sqrt(4-x^2)",
    "ln(4-x^2)",
    "1/(sqrt(x)-1)",
    "x^2+1",
    "-x^2-1",
    "cos(x)^2",
    "sin(x)^2+1",
    "x+1/x",
    "e^(-x)",
    "2^x",
    "x^5-5x^3+4x",
    "(x-1)^(1/3)",
    "x^(2/3)",
    "1/(x^2+x+1)",
    "x^2*e^(-x)",
    "sin(x)*e^(-x/10)",
    "ln(x)/x",
    "x^3+x",
    "(x+1)^2/(x-1)",
    "sqrt(x)*ln(x)",
    "e^(1/x)",
    "e^(1/(x-1))",
    "x^2-2x+1",
    "2x+3",
    "x^4-4x^3+6x^2-4x+1",
    "(x^2+1)/(x^2-1)",
    "x/(x^2+1)",
    "atan(ln(abs(x)))",
    "cos(x)+sin(2x)",
    "x-sin(x)",
    "ln(x)^2",
    "sqrt(x^2-1)",
    "1/sqrt(1-x^2)",
    "x^2*ln(x)",
    "e^x/x",
    "x^3-6x^2+11x-6",
    "x^4-5x^2+4",
    "(x-3)^2-4",
    "100x^2",
    "0.01x^2",
    "x^2/100+x",
];

/// `x` replaced by `with` wherever it stands alone (not inside a name).
fn subst_x(e: &str, with: &str) -> String {
    let cs: Vec<char> = e.chars().collect();
    let mut out = String::new();
    for (i, &c) in cs.iter().enumerate() {
        let letter = |j: Option<&char>| j.is_some_and(|c| c.is_ascii_alphabetic());
        if c == 'x' && !letter(i.checked_sub(1).and_then(|j| cs.get(j))) && !letter(cs.get(i + 1)) {
            out.push_str(&format!("({with})"));
        } else {
            out.push(c);
        }
    }
    out
}

pub fn pool() -> Vec<(String, TrigUnit)> {
    let mut out = Vec::new();
    for b in BASES {
        out.push((b.to_string(), TrigUnit::Radians));
        out.push((subst_x(b, "x-1000"), TrigUnit::Radians));
        out.push((format!("({b})+0.0000001"), TrigUnit::Radians));
        out.push((format!("-3*({b})"), TrigUnit::Radians));
        out.push((subst_x(b, "x/1000"), TrigUnit::Radians));
    }
    out
}

/// The kgf set in the other angle units.
pub fn units() -> Vec<(String, TrigUnit)> {
    let mut out = Vec::new();
    for line in CASES.lines().filter(|l| l.starts_with('K')) {
        let src = line.split('\t').nth(2).unwrap_or_default();
        for u in [TrigUnit::Degrees, TrigUnit::Grads] {
            out.push((src.to_string(), u));
        }
    }
    out
}
