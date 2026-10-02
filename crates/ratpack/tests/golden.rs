// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Differential test against the original C++ ratpack.
//!
//! `tests/data/ratpack_golden.txt` was produced by the C++ oracle in
//! `tools/oracle/ratpack/` (driver.cpp built against the unmodified
//! Windows Calculator sources). Each line is
//!
//! ```text
//! <command>\t<expected result>
//! ```
//!
//! and is replayed here, in order, on a single thread (ratpack's globals are
//! per-thread and `ChangeConstants` is order dependent), requiring exact
//! textual equality of the result.
//!
//! Command grammar (rationals are `P/Q` with each number written as
//! `sign:exp:d0,d1,...` in base 2^31, least significant digit first, or
//! `$name` for a register; strings are `'...'`):
//!
//! * `CC radix precision` — `ChangeConstants`
//! * `SEP 'c'` — `SetDecimalSeparator`
//! * `BIN op A B` — add sub mul div rem shl shr and or xor mod pow root
//! * `UN op A` — neg frac int fact exp log log10 inv abs sinh cosh tanh asinh acosh atanh
//! * `TRIG op deg|rad|grad A` — sin cos tan asin acos atan
//! * `CMP A B` — `== != < > <= >=` as six 0/1 digits
//! * `STR radix float|sci|eng precision A` — `Rational::ToString`
//! * `U64 A` — `Rational::ToUInt64_t`
//! * `FROMI32 v`, `FROMU32 v`, `FROMU64 v`, `FROMNUM n` — constructors
//! * `CONST qword|dword|word|byte|exp|ln_ten|pi`
//! * `S2R mneg 'mantissa' eneg 'exponent' radix precision` — `StringToRat`
//! * a leading `@name` stores a rational result in register `name`.
//!
//! Results: `R:P/Q`, `S:string`, `U:u64`, `C:bits`, `NULL`, `ok`, or
//! `E:XXXXXXXX` for a thrown `CALC_E_*` code.

use std::collections::HashMap;

use ratpack::rational_math as rm;
use ratpack::{AngleType, CalcErr, CalcResult, Number, NumberFormat, Rational};

fn parse_num(s: &str) -> Number {
    let mut parts = s.splitn(3, ':');
    let sign: i32 = parts.next().unwrap().parse().unwrap();
    let exp: i32 = parts.next().unwrap().parse().unwrap();
    let digits = parts.next().unwrap();
    let mant = if digits.is_empty() {
        Vec::new()
    } else {
        digits.split(',').map(|d| d.parse().unwrap()).collect()
    };
    Number::new(sign, exp, mant)
}

fn parse_rat(s: &str, regs: &HashMap<String, Rational>) -> Rational {
    if let Some(name) = s.strip_prefix('$') {
        return regs
            .get(name)
            .unwrap_or_else(|| panic!("unknown register {s}"))
            .clone();
    }
    let (p, q) = s
        .split_once('/')
        .unwrap_or_else(|| panic!("bad rational {s}"));
    Rational::from_pq(parse_num(p), parse_num(q))
}

fn fmt_num(n: &Number) -> String {
    let digits: Vec<String> = n.mantissa().iter().map(|d| d.to_string()).collect();
    format!("{}:{}:{}", n.sign(), n.exp(), digits.join(","))
}

fn fmt_rat(r: &Rational) -> String {
    format!("R:{}/{}", fmt_num(r.p()), fmt_num(r.q()))
}

fn fmt_err(e: CalcErr) -> String {
    format!("E:{e:08X}")
}

fn unquote(s: &str) -> &str {
    s.strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or_else(|| panic!("bad string {s}"))
}

fn parse_fmt(s: &str) -> NumberFormat {
    match s {
        "float" => NumberFormat::Float,
        "sci" => NumberFormat::Scientific,
        "eng" => NumberFormat::Engineering,
        _ => panic!("bad format {s}"),
    }
}

fn parse_angle(s: &str) -> AngleType {
    match s {
        "deg" => AngleType::Degrees,
        "rad" => AngleType::Radians,
        "grad" => AngleType::Gradians,
        _ => panic!("bad angle {s}"),
    }
}

fn binop(op: &str, a: &Rational, b: &Rational) -> CalcResult<Rational> {
    match op {
        "add" => a.add(b),
        "sub" => a.sub(b),
        "mul" => a.mul(b),
        "div" => a.div(b),
        "rem" => a.rem(b),
        "shl" => a.shl(b),
        "shr" => a.shr(b),
        "and" => a.bitand(b),
        "or" => a.bitor(b),
        "xor" => a.bitxor(b),
        "mod" => rm::modulo(a, b),
        "pow" => rm::pow(a, b),
        "root" => rm::root(a, b),
        _ => panic!("bad binop {op}"),
    }
}

fn unop(op: &str, a: &Rational) -> CalcResult<Rational> {
    match op {
        "neg" => Ok(-a),
        "frac" => rm::frac(a),
        "int" => rm::integer(a),
        "fact" => rm::fact(a),
        "exp" => rm::exp(a),
        "log" => rm::log(a),
        "log10" => rm::log10(a),
        "inv" => rm::invert(a),
        "abs" => rm::abs(a),
        "sinh" => rm::sinh(a),
        "cosh" => rm::cosh(a),
        "tanh" => rm::tanh(a),
        "asinh" => rm::asinh(a),
        "acosh" => rm::acosh(a),
        "atanh" => rm::atanh(a),
        _ => panic!("bad unop {op}"),
    }
}

fn trigop(op: &str, at: AngleType, a: &Rational) -> CalcResult<Rational> {
    match op {
        "sin" => rm::sin(a, at),
        "cos" => rm::cos(a, at),
        "tan" => rm::tan(a, at),
        "asin" => rm::asin(a, at),
        "acos" => rm::acos(a, at),
        "atan" => rm::atan(a, at),
        _ => panic!("bad trigop {op}"),
    }
}

fn constant(name: &str) -> Rational {
    match name {
        "qword" => ratpack::rat_qword(),
        "dword" => ratpack::rat_dword(),
        "word" => ratpack::rat_word(),
        "byte" => ratpack::rat_byte(),
        "exp" => ratpack::rat_exp(),
        "ln_ten" => ratpack::ln_ten(),
        "pi" => ratpack::pi(),
        _ => panic!("bad constant {name}"),
    }
}

/// Executes one command; returns the result text and a rational result for
/// register stores.
fn execute(t: &[&str], regs: &HashMap<String, Rational>) -> (String, Option<Rational>) {
    let rat = |r: CalcResult<Rational>| match r {
        Ok(r) => (fmt_rat(&r), Some(r)),
        Err(e) => (fmt_err(e), None),
    };
    let arg = |i: usize| parse_rat(t[i], regs);

    match t[0] {
        "CC" => {
            ratpack::change_constants(t[1].parse().unwrap(), t[2].parse().unwrap());
            ("ok".into(), None)
        }
        "SEP" => {
            ratpack::set_decimal_separator(unquote(t[1]).chars().next().unwrap());
            ("ok".into(), None)
        }
        "BIN" => rat(binop(t[1], &arg(2), &arg(3))),
        "UN" => rat(unop(t[1], &arg(2))),
        "TRIG" => rat(trigop(t[1], parse_angle(t[2]), &arg(3))),
        "CMP" => {
            let (a, b) = (arg(1), arg(2));
            let bits = [a == b, a != b, a < b, a > b, a <= b, a >= b];
            let s: String = bits.iter().map(|&x| if x { '1' } else { '0' }).collect();
            (format!("C:{s}"), None)
        }
        "STR" => {
            let r = arg(4).to_string_radix(
                t[1].parse().unwrap(),
                parse_fmt(t[2]),
                t[3].parse().unwrap(),
            );
            match r {
                Ok(s) => (format!("S:{s}"), None),
                Err(e) => (fmt_err(e), None),
            }
        }
        "U64" => match arg(1).to_u64() {
            Ok(v) => (format!("U:{v}"), None),
            Err(e) => (fmt_err(e), None),
        },
        "FROMI32" => rat(Ok(Rational::from(t[1].parse::<i32>().unwrap()))),
        "FROMU32" => rat(Ok(Rational::from(t[1].parse::<u32>().unwrap()))),
        "FROMU64" => rat(Ok(Rational::from(t[1].parse::<u64>().unwrap()))),
        "FROMNUM" => rat(Ok(Rational::from_number(parse_num(t[1])))),
        "CONST" => rat(Ok(constant(t[1]))),
        "S2R" => {
            let r = ratpack::string_to_rat(
                t[1] == "1",
                unquote(t[2]),
                t[3] == "1",
                unquote(t[4]),
                t[5].parse().unwrap(),
                t[6].parse().unwrap(),
            );
            match r {
                Ok(None) => ("NULL".into(), None),
                Ok(Some(r)) => (fmt_rat(&r), Some(r)),
                Err(e) => (fmt_err(e), None),
            }
        }
        other => panic!("bad command {other}"),
    }
}

#[test]
fn golden_against_cpp_oracle() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/ratpack_golden.txt");
    let data = std::fs::read_to_string(path).expect("golden file");

    let mut regs: HashMap<String, Rational> = HashMap::new();
    let mut cases = 0usize;
    let mut failures: Vec<String> = Vec::new();
    // RATPACK_GOLDEN_SLOW_MS=<ms>: report cases slower than that (profiling aid).
    let slow_ms: Option<f64> = std::env::var("RATPACK_GOLDEN_SLOW_MS")
        .ok()
        .and_then(|v| v.parse().ok());

    for (lineno, line) in data.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (cmd, expected) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("line {}: no tab", lineno + 1));
        let mut tokens: Vec<&str> = cmd.split_whitespace().collect();
        let reg = match tokens.first() {
            Some(t) if t.starts_with('@') => {
                let r = t[1..].to_string();
                tokens.remove(0);
                Some(r)
            }
            _ => None,
        };

        let start = std::time::Instant::now();
        let (actual, value) = execute(&tokens, &regs);
        if let Some(limit) = slow_ms {
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            if ms > limit {
                eprintln!(
                    "SLOW {ms:.1}ms: line {}: {}",
                    lineno + 1,
                    &cmd[..cmd.len().min(160)]
                );
            }
        }
        if let (Some(reg), Some(v)) = (reg, value) {
            regs.insert(reg, v);
        }
        cases += 1;
        if actual != expected {
            failures.push(format!(
                "line {}: {}\n    expected: {}\n    actual:   {}",
                lineno + 1,
                cmd,
                expected,
                actual
            ));
        }
    }

    if !failures.is_empty() {
        let shown: Vec<&String> = failures.iter().take(25).collect();
        panic!(
            "{} of {} golden cases differ from the C++ oracle; first {}:\n{}",
            failures.len(),
            cases,
            shown.len(),
            shown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    assert!(cases > 1000, "golden file looks truncated ({cases} cases)");
    eprintln!("{cases} golden cases match the C++ oracle");
}
