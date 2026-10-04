//! The certified analysis (`graphing::certify`) on the truth table: the 61
//! inputs of the Giac spike (`fixtures/certify/cases.tsv`, their prose
//! truth in `truth.txt`, encoded below as `TRUTH`) and the functions of
//! review rounds 9–11 (`REVIEW`). No row may be certified wrong: a
//! certified row must equal the truth, a partial one may only list true
//! items (and, over a window, all of them there), an unknown row is never
//! wrong.
//!
//! `cargo test -p graphing --test certify_corpus -- --nocapture` also
//! prints the coverage per row (certified/partial/unknown) beside the
//! current engine's (its grades on the same inputs in `engine_grades.tsv`,
//! and what it answers on the review functions), and the time and
//! evaluations each function took.

use std::time::Instant;

use graphing::analysis::{analyze_str, flags};
use graphing::certify::{
    Analysis, Bound, DEFAULT_BUDGET, DomainValue, Enc, ExtKind, Parity, Piece, Region, Row, Spot,
    Tail, certify_text,
};
use graphing::compile::{CompileOptions, compile_str};
use graphing::functions::TrigUnit;

const CASES: &str = include_str!("fixtures/certify/cases.tsv");
const ENGINE: &str = include_str!("fixtures/certify/engine_grades.tsv");

/// `truth.txt`, one line per id. Keys: D domain, XI x-intercepts, YI
/// y-intercept, P parity, T period, MIN/MAX extrema (local, strict; a
/// closed end of the domain where f turns away counts, as in Windows:
/// √x has a minimum at 0), INF inflections, VA/HA asymptotes, R
/// range. A key left out is not checked (the truth is debatable or not a
/// set the rows can state). Values are expressions; `~` marks a value
/// known to 6 digits, `-`/`+` one just below/above a double.
const TRUTH: &[(&str, &str)] = &[
    (
        "K01",
        "D=R | XI=0 | YI=0 | P=even | T=none | MIN=(0,0) | MAX=none | INF=none | VA=none | HA=none | R=[0,inf)",
    ),
    (
        "K02",
        "D=(-inf,1)U(1,inf) | XI=-1.288795~;-0.389391~ | YI=-1 | P=neither | T=none | MAX=(-0.872759~,0.546759~) | MIN=(1.471580~,2.364148~) | INF=none | VA=1 | HA=none | R=(-inf,0.546759~]U[2.364148~,inf)",
    ),
    (
        "K03",
        "D=R | XI=fam(0,pi) | YI=0 | P=odd | T=2*pi | MAX=fam(pi/2,2*pi,1) | MIN=fam(-pi/2,2*pi,-1) | INF=fam(0,pi,0) | VA=none | HA=none | R=[-1,1]",
    ),
    (
        "K04",
        "D=fam(pi/2,pi) | XI=fam(0,pi) | YI=0 | P=odd | T=pi | MIN=none | MAX=none | INF=fam(0,pi,0) | VA=fam(pi/2,pi) | HA=none | R=R",
    ),
    (
        "K05",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "K06",
        "D=(0,inf) | XI=1 | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=none | R=R",
    ),
    (
        "K07",
        "D=(0,inf) | XI=1 | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=none | R=R",
    ),
    (
        "K08",
        "D=[0,inf) | XI=0 | YI=0 | P=neither | T=none | MIN=(0,0) | MAX=none | INF=none | VA=none | HA=none | R=[0,inf)",
    ),
    (
        "K09",
        "D=R | XI=none | YI=1 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=L:0 | R=(0,inf)",
    ),
    (
        "K10",
        "D=(-inf,1)U(1,inf) | XI=-1 | YI=1 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=(-inf,2)U(2,inf)",
    ),
    (
        "K11",
        "D=[0,inf) | XI=0 | YI=0 | P=neither | T=none | MIN=(0,0) | MAX=none | INF=none | VA=none | HA=none | R=[0,inf)",
    ),
    (
        "K12",
        "D=(0,1)U(1,inf) | XI=none | YI=none | P=neither | T=none | MIN=(e,e) | MAX=none | INF=(e^2,e^2/2) | VA=1 | HA=none | R=(-inf,0)U[e,inf)",
    ),
    (
        "K13",
        "D=(0,1)U(1,inf) | XI=none | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=(e^-2,-1/2) | VA=1 | HA=R:0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "K14",
        "D=R | XI=inf | YI=0 | P=even | T=none | MIN=inf | MAX=inf | INF=inf | VA=none | HA=none | R=[-1,1]",
    ),
    (
        "K15",
        "D=R | YI=0 | P=neither | T=none | INF=none | VA=none | HA=none",
    ),
    (
        "K16",
        "D=R | XI=0 | YI=0 | P=odd | T=none | MIN=none | MAX=none | INF=(0,0) | VA=none | HA=L:-pi/2;R:pi/2 | R=(-pi/2,pi/2)",
    ),
    (
        "K17",
        "D=R | XI=0 | YI=0 | P=neither | T=none | MAX=(1,1/e) | MIN=none | INF=(2,2/e^2) | VA=none | HA=R:0 | R=(-inf,1/e]",
    ),
    (
        "K18",
        "D=(-inf,0)U(0,inf) | XI=fam(0,pi) | YI=none | P=even | T=none | MIN=inf | MAX=inf | INF=inf | VA=none | HA=0 | R=[-0.217234~,1)",
    ),
    (
        "K19",
        "D=R | XI=fam(0,pi) | YI=0 | P=even | T=none | MIN=inf | MAX=inf | INF=inf | VA=none | HA=none | R=R",
    ),
    (
        "K20",
        "D=(-inf,2)U(2,inf) | XI=none | YI=-1/2 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=2 | HA=0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "K21",
        "D=(-inf,-2)U(-2,2)U(2,inf) | XI=none | YI=-1/4 | P=even | T=none | MAX=(0,-1/4) | MIN=none | INF=none | VA=-2;2 | HA=0 | R=(-inf,-1/4]U(0,inf)",
    ),
    (
        "K22",
        "D=fam(0,pi) | XI=fam(pi/2,pi) | YI=none | P=odd | T=pi | MIN=none | MAX=none | INF=fam(pi/2,pi,0) | VA=fam(0,pi) | HA=none | R=R",
    ),
    (
        "K23",
        "D=fam(0,pi) | XI=none | YI=none | P=odd | T=2*pi | MIN=fam(pi/2,2*pi,1) | MAX=fam(-pi/2,2*pi,-1) | INF=none | VA=fam(0,pi) | HA=none | R=(-inf,-1]U[1,inf)",
    ),
    (
        "K24",
        "D=fam(pi/2,pi) | XI=none | YI=1 | P=even | T=2*pi | MIN=fam(0,2*pi,1) | MAX=fam(pi,2*pi,-1) | INF=none | VA=fam(pi/2,pi) | HA=none | R=(-inf,-1]U[1,inf)",
    ),
    (
        "K25",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=L:-1;R:1 | R=(-inf,-1)U(1,inf)",
    ),
    (
        "K26",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "K27",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=0 | R=(-pi/2,0)U(0,pi/2)",
    ),
    (
        "K28",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=even | T=none | MIN=none | MAX=none | INF=(-sqrt(2/3),e^-1.5);(sqrt(2/3),e^-1.5) | VA=none | HA=1 | R=(0,1)",
    ),
    (
        "K29",
        "D=(0,inf) | XI=none | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=(0,inf)",
    ),
    (
        "K30",
        "D=R | XI=none | YI=1/2 | P=neither | T=none | MIN=none | MAX=none | INF=(0,1/2) | VA=none | HA=L:1;R:0 | R=(0,1)",
    ),
    (
        "K31",
        "D=(0,inf) | XI=1 | YI=none | P=neither | T=none | MIN=(1/e,-1/e) | MAX=none | INF=none | VA=none | HA=none | R=[-1/e,inf)",
    ),
    (
        "K32",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=even | T=none | MIN=none | MAX=none | INF=none | VA=none | R={1}",
    ),
    (
        "K33",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=even | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=0 | R=(0,inf)",
    ),
    (
        "K34",
        "D=(5000000,inf) | XI=5000001 | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=5000000 | HA=none | R=R",
    ),
    (
        "K35",
        "D=(-inf,-20000)U(-20000,20000)U(20000,inf) | XI=none | YI=-1/400000000 | P=even | T=none | MAX=(0,-1/400000000) | MIN=none | INF=none | VA=-20000;20000 | HA=0 | R=(-inf,-1/400000000]U(0,inf)",
    ),
    (
        "K36",
        "D=fam(pi/2,pi) | XI=fam(0,pi) | YI=0 | P=even | T=pi | MIN=fam(0,pi,0) | MAX=none | INF=none | VA=fam(pi/2,pi) | HA=none | R=[0,inf)",
    ),
    (
        "K37",
        "D=fam(0,pi) | XI=none | YI=none | P=odd | T=2*pi | MIN=fam(pi/2,2*pi,1) | MAX=fam(-pi/2,2*pi,-1) | INF=none | VA=fam(0,pi) | HA=none | R=(-inf,-1]U[1,inf)",
    ),
    (
        "K38",
        "D=(-inf,0)U(0,inf) | XI=-1;1 | YI=none | P=even | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=none | R=R",
    ),
    (
        "K39",
        "D=(0,inf) | XI=none | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=R:0 | R=(0,inf)",
    ),
    (
        "K40",
        "D=(0,inf) | XI=none | YI=none | P=neither | T=none | MIN=(1/e,e^(-1/e)) | MAX=none | INF=none | VA=none | HA=none | R=[e^(-1/e),inf)",
    ),
    (
        "A01",
        "D=R | XI=none | YI=1- | P=even | T=none | INF=none | VA=none | R={1-}",
    ),
    (
        "A02",
        "D=R | XI=all | YI=0 | P=both | T=none | INF=none | VA=none | R={0}",
    ),
    (
        "A03",
        "D=fam(pi/2,pi) | XI=fam(0,pi) | YI=0 | P=odd | T=2*pi | MIN=none | MAX=none | INF=fam(0,pi,0) | VA=none | HA=none | R=(-1,1)",
    ),
    (
        "A04",
        "D=fam(pi/2,pi) | XI=none | YI=1 | P=even | T=pi | MIN=none | MAX=none | INF=none | VA=none | R={1}",
    ),
    (
        "A05",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(1522756~,2~) | INF=(-0.707107~,0.606531~);(0.707107~,0.606531~);(1522755.999293~,1.213061~);(1522756.000707~,1.213061~) | VA=none | HA=0 | R=(0,2~]",
    ),
    (
        "A06",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(1522756~,1~) | VA=none | HA=0 | R=(0,1~]",
    ),
    (
        "A07",
        "D=R | XI=none | YI=10000000000000 | P=neither | T=2*pi | MAX=fam(pi/2,2*pi,10000000000001) | MIN=fam(-pi/2,2*pi,9999999999999) | INF=fam(0,pi,10000000000000) | VA=none | HA=none | R=[9999999999999,10000000000001]",
    ),
    (
        "A08",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(100~,1~) | VA=none | HA=0 | R=(0,1~]",
    ),
    (
        "A09",
        "D=(-inf,16000000000001/16)U(16000000000001/16,inf) | XI=1000000000000 | YI=0.9999999999999375~ | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=16000000000001/16 | HA=1 | R=(-inf,1)U(1,inf)",
    ),
    (
        "A10",
        "D=R | XI=none | YI=1000000000000.0000001~ | P=neither | T=none | MIN=(1000000~,0.0000011~) | MAX=none | INF=none | VA=none | HA=none | R=[0.0000011~,inf)",
    ),
    (
        "A11",
        "D=R | YI=1 | P=even | T=2*pi | INF=none | VA=none | HA=none",
    ),
    (
        "A12",
        "D=fam(pi/2,pi) | XI=fam(-1,pi) | YI=1 | P=neither | T=pi | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=(1-pi/2,1+pi/2)",
    ),
    (
        "A13",
        "D=R | XI=inf | YI=0 | P=odd | T=none | MIN=inf | MAX=inf | INF=inf | VA=none | HA=none | R=(-2,2)",
    ),
    (
        "A14",
        "D=fam(pi,2*pi) | XI=none | YI=1/2 | P=even | T=2*pi | MIN=fam(0,2*pi,1/2) | MAX=none | INF=none | VA=fam(pi,2*pi) | HA=none | R=[1/2,inf)",
    ),
    (
        "A15",
        "D=R | XI=none | YI=1 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=R:0 | R=(0,inf)",
    ),
    (
        "A16",
        "D=R | XI=all | YI=0 | P=both | T=none | INF=none | VA=none | R={0}",
    ),
];

/// The functions of review rounds 9–11 (REVIEW_9/10/11.md), with what is
/// known of each. The reviews' `1e-7`, `1e13` are written out here: the
/// app reads `1e6` as 1·e·6 (`1e9*x` is kept, as that line).
const REVIEW: &[(&str, &str)] = &[
    // Limits a dominant-term argument can get wrong (1^∞, 0·∞): each row
    // the exact limit, or unknown.
    ("(1+1/x)^x", "HA=e"),
    ("(1-1/x)^x", "HA=1/e"),
    ("(1+2/x)^x", "HA=e^2"),
    ("(1+1/x)^(2x)", "HA=e^2"),
    ("(x/(x+1))^x", "HA=1/e"),
    ("x^(1/x)", "HA=R:1"),
    ("(1+1/x^2)^x", "HA=1"),
    ("x*ln(1+1/x)", "HA=1"),
    ("(1+1/x)^(x^2)", "HA=L:0"),
    // x^(1/ln x) = e^((1/ln x)·ln x) = e for x > 0, x ≠ 1: its limit at +∞
    // is e (a constant tail has its value as asymptote, as x/x has 1).
    ("x^(1/ln(x))", "HA=R:e"),
    ("x*sin(1/x)", "HA=1"),
    ("x^2*(1-cos(1/x))", "HA=1/2"),
    ("x*(e^(1/x)-1)", "HA=1"),
    ("sqrt(x^2+x)-x", "HA=R:1/2"),
    ("x*sin(1/x^2)", "HA=0"),
    ("(e^x+x)^(1/x)", "HA=R:e"),
    ("x*e^(-x)", "HA=R:0"),
    ("x^2*e^(-x)", "HA=R:0"),
    ("ln(x)/x", "HA=R:0"),
    (
        "x^2+0.0000001",
        "D=R | XI=none | YI=0.0000001 | P=even | T=none | MIN=(0,0.0000001) | MAX=none | INF=none | VA=none | HA=none | R=[0.0000001,inf)",
    ),
    (
        "-x^2-0.0000001",
        "D=R | XI=none | YI=-0.0000001 | P=even | T=none | MAX=(0,-0.0000001) | MIN=none | INF=none | VA=none | HA=none | R=(-inf,-0.0000001]",
    ),
    (
        "sin(x)^2+0.000000001",
        "D=R | XI=none | YI=0.000000001 | P=even | T=pi | VA=none | HA=none | R=[0.000000001,1+0.000000001]",
    ),
    (
        "(x-1000000000)*(x-1000000000.0625)",
        "D=R | XI=1000000000;1000000000.0625 | P=neither | T=none | MIN=(1000000000.03125,-0.0009765625) | MAX=none | INF=none | VA=none | HA=none | R=[-0.0009765625,inf)",
    ),
    (
        "(x-1000000000000)*(x-1000000000000.0625)",
        "D=R | XI=1000000000000;1000000000000.0625 | P=neither | T=none | MIN=(1000000000000.03125,-0.0009765625) | MAX=none | INF=none | VA=none | HA=none | R=[-0.0009765625,inf)",
    ),
    (
        "(x-1000000000000)^2+0.0000001+(x/1000000000000000)^2",
        "D=R | XI=none | P=neither | T=none | MIN=(1000000000000~,0.0000011~) | MAX=none | INF=none | VA=none | HA=none | R=[0.0000011~,inf)",
    ),
    (
        "(x-1000000)^2+1+(x/1000000000)^2",
        "D=R | XI=none | P=neither | T=none | MIN=(1000000~,1.000001~) | MAX=none | INF=none | VA=none | HA=none | R=[1.000001~,inf)",
    ),
    (
        "(x-1000000)^2+0.0000001+(x/1000000000)^2",
        "D=R | XI=none | P=neither | T=none | MIN=(1000000~,0.0000011~) | MAX=none | INF=none | VA=none | HA=none | R=[0.0000011~,inf)",
    ),
    (
        "atan(tan(x))+1",
        "D=fam(pi/2,pi) | XI=fam(-1,pi) | YI=1 | P=neither | T=pi | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=(1-pi/2,1+pi/2)",
    ),
    (
        "floor(cos(x))",
        "D=R | YI=1 | P=even | T=2*pi | INF=none | VA=none | HA=none",
    ),
    (
        "ceil(cos(x))",
        "D=R | YI=1 | P=even | T=2*pi | INF=none | VA=none | HA=none",
    ),
    (
        "ceil(sin(x))",
        "D=R | YI=0 | P=neither | T=2*pi | INF=none | VA=none | HA=none",
    ),
    (
        "floor(cos(x-1000000))",
        "D=R | T=2*pi | INF=none | VA=none | HA=none",
    ),
    (
        "floor(x)",
        "D=R | YI=0 | P=neither | T=none | INF=none | VA=none | HA=none",
    ),
    (
        "1/(1+cos(x))",
        "D=fam(pi,2*pi) | XI=none | YI=1/2 | P=even | T=2*pi | MIN=fam(0,2*pi,1/2) | MAX=none | INF=none | VA=fam(pi,2*pi) | HA=none | R=[1/2,inf)",
    ),
    (
        "e^(1/x)-1000000",
        "D=(-inf,0)U(0,inf) | XI=1/ln(1000000) | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=(-1/2,e^-2-1000000) | VA=0 | HA=-999999 | R=(-1000000,-999999)U(-999999,inf)",
    ),
    (
        "sin(x)+10^13",
        "D=R | XI=none | YI=10^13 | P=neither | T=2*pi | MAX=fam(pi/2,2*pi,10^13+1) | MIN=fam(-pi/2,2*pi,10^13-1) | VA=none | HA=none | R=[10^13-1,10^13+1]",
    ),
    (
        "cos(x)+10^13",
        "D=R | XI=none | YI=10^13+1 | P=even | T=2*pi | MAX=fam(0,2*pi,10^13+1) | MIN=fam(pi,2*pi,10^13-1) | VA=none | HA=none | R=[10^13-1,10^13+1]",
    ),
    (
        "sin(x)+10000000000000",
        "D=R | XI=none | YI=10000000000000 | P=neither | T=2*pi | VA=none | HA=none | R=[9999999999999,10000000000001]",
    ),
    (
        "sin(x)-sin(x+0.000000001)",
        "D=R | XI=fam(pi/2-0.0000000005,pi) | P=neither | T=2*pi | VA=none | HA=none | R=[-2*sin(0.0000000005),2*sin(0.0000000005)]",
    ),
    (
        "exp(-(x-100)^2)+exp(-x^2)",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(100~,1~) | VA=none | HA=0 | R=(0,1~]",
    ),
    (
        "exp(-(x-1000)^2)+exp(-x^2)",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(1000~,1~) | VA=none | HA=0 | R=(0,1~]",
    ),
    (
        "exp(-(x-1234)^2)+exp(-x^2)",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(1234~,1~) | VA=none | HA=0 | R=(0,1~]",
    ),
    (
        "2*exp(-(x-1522756)^2)+exp(-x^2)",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(1522756~,2~) | VA=none | HA=0 | R=(0,2~]",
    ),
    (
        "2*exp(-(x-1522756)^2/0.000001)+exp(-x^2)",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(1522756~,2~) | VA=none | HA=0 | R=(0,2~]",
    ),
    (
        "1/(1+(x-1522756)^2/0.000000000001)+exp(-x^2)",
        "D=R | XI=none | YI=1~ | P=neither | T=none | MAX=(0~,1~);(1522756~,1~) | VA=none | HA=0 | R=(0,1~]",
    ),
    (
        "exp(-(x-1234*1234)^2)+exp(-x^2)",
        "D=R | XI=none | P=neither | T=none | MAX=(0~,1~);(1522756~,1~) | VA=none | HA=0 | R=(0,1~]",
    ),
    (
        "3*exp(-(x-2^20)^2)+exp(-x^2)",
        "D=R | XI=none | P=neither | T=none | MAX=(0~,1~);(1048576~,3~) | VA=none | HA=0 | R=(0,3~]",
    ),
    (
        "exp(-(x-7*11*13)^2/0.01)+x^2/10^9",
        "D=R | XI=none | P=neither | T=none | VA=none | HA=none",
    ),
    (
        "(x-1000000000000)/(x-1000000000000.0625)",
        "D=(-inf,1000000000000.0625)U(1000000000000.0625,inf) | XI=1000000000000 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=1000000000000.0625 | HA=1 | R=(-inf,1)U(1,inf)",
    ),
    (
        "(x-1000000000)/(x-1000000000.0625)",
        "D=(-inf,1000000000.0625)U(1000000000.0625,inf) | XI=1000000000 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=1000000000.0625 | HA=1 | R=(-inf,1)U(1,inf)",
    ),
    (
        "(x-2^40)/(x-2^40-1/16)",
        "D=(-inf,2^40+1/16)U(2^40+1/16,inf) | XI=2^40 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=2^40+1/16 | HA=1 | R=(-inf,1)U(1,inf)",
    ),
    ("1+sqrt(-exp(-(x+800)))", "D=empty"),
    ("sqrt(-exp(-(x+800)))^0", "D=empty"),
    (
        "sqrt(exp(-(x+800)))*exp((x+800)/2)",
        "D=R | XI=none | YI=1 | P=even | INF=none | VA=none | R={1}",
    ),
    (
        "exp(-1000)*exp(1000)",
        "D=R | XI=none | YI=1 | P=even | T=none | VA=none | R={1}",
    ),
    (
        "exp(-1000)*(x+1)*exp(500)*exp(500)",
        "D=R | XI=-1 | YI=1 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=R",
    ),
    (
        "0/x",
        "D=(-inf,0)U(0,inf) | XI=all | YI=none | P=both | T=none | INF=none | VA=none | R={0}",
    ),
    (
        "0/x^2",
        "D=(-inf,0)U(0,inf) | XI=all | YI=none | P=both | T=none | INF=none | VA=none | R={0}",
    ),
    (
        "0/exp(x)",
        "D=R | XI=all | YI=0 | P=both | T=none | INF=none | VA=none | R={0}",
    ),
    (
        "0/exp(1/x)",
        "D=(-inf,0)U(0,inf) | XI=all | YI=none | P=both | T=none | INF=none | VA=none | R={0}",
    ),
    (
        "0*e^x",
        "D=R | XI=all | YI=0 | P=both | T=none | INF=none | VA=none | R={0}",
    ),
    (
        "0*x",
        "D=R | XI=all | YI=0 | P=both | T=none | INF=none | VA=none | R={0}",
    ),
    (
        "x^2/x^2",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=even | T=none | INF=none | VA=none | R={1}",
    ),
    (
        "e^x/e^x",
        "D=R | XI=none | YI=1 | P=even | T=none | INF=none | VA=none | R={1}",
    ),
    (
        "exp(x)/exp(x)",
        "D=R | XI=none | YI=1 | P=even | T=none | INF=none | VA=none | R={1}",
    ),
    (
        "ln(exp(x))",
        "D=R | XI=0 | YI=0 | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=R",
    ),
    (
        "ln(exp(-x^2))",
        "D=R | XI=0 | YI=0 | P=even | T=none | MAX=(0,0) | MIN=none | INF=none | VA=none | HA=none | R=(-inf,0]",
    ),
    (
        "exp(-x^2)/exp(-2*x^2)",
        "D=R | XI=none | YI=1 | P=even | T=none | MIN=(0,1) | MAX=none | INF=none | VA=none | HA=none | R=[1,inf)",
    ),
    (
        "ln(max(exp(1000),exp(2000)))",
        "D=R | XI=none | YI=2000 | P=even | T=none | VA=none | R={2000}",
    ),
    (
        "atan(1/exp(-1000))",
        "D=R | XI=none | YI=pi/2- | P=even | T=none | VA=none | R={pi/2-}",
    ),
    (
        "atan(1/exp(x))",
        "D=R | XI=none | YI=pi/4 | P=neither | T=none | MIN=none | MAX=none | INF=(0,pi/4) | VA=none | HA=L:pi/2;R:0 | R=(0,pi/2)",
    ),
    (
        "atan(exp(x)^-1)",
        "D=R | XI=none | YI=pi/4 | P=neither | T=none | MIN=none | MAX=none | INF=(0,pi/4) | VA=none | HA=L:pi/2;R:0 | R=(0,pi/2)",
    ),
    (
        "1/exp(x)",
        "D=R | XI=none | YI=1 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=R:0 | R=(0,inf)",
    ),
    (
        "exp(1/x)",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=(-1/2,e^-2) | VA=0 | HA=1 | R=(0,1)U(1,inf)",
    ),
    (
        "ln(1+e^x)",
        "D=R | XI=none | YI=ln(2) | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=L:0 | R=(0,inf)",
    ),
    (
        "ln(x)+10^6",
        "D=(0,inf) | XI=0+ | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=none | R=R",
    ),
    (
        "ln(x)-40",
        "D=(0,inf) | XI=e^40 | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=none | R=R",
    ),
    (
        "1/ln(x)",
        "D=(0,1)U(1,inf) | XI=none | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=(e^-2,-1/2) | VA=1 | HA=R:0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "1-1/ln(x)",
        "D=(0,1)U(1,inf) | XI=e | YI=none | P=neither | T=none | MIN=none | MAX=none | VA=1 | HA=R:1 | R=(-inf,1)U(1,inf)",
    ),
    ("sin(x+2^-1074)/(x+2^-1074)", "HA=0"),
    (
        "sin(x)cos(3x)+sin(5x)/x",
        "D=(-inf,0)U(0,inf) | YI=none | HA=0",
    ),
    (
        "sin(e^x)",
        "D=R | YI=sin(1) | P=neither | T=none | VA=none | R=[-1,1]",
    ),
    (
        "sin((x/1000000)^2)",
        "D=R | XI=inf | YI=0 | P=even | T=none | VA=none | HA=none | R=[-1,1]",
    ),
    (
        "sin(x^2)",
        "D=R | XI=inf | YI=0 | P=even | T=none | MIN=inf | MAX=inf | INF=inf | VA=none | HA=none | R=[-1,1]",
    ),
    (
        "sin(pi*x)",
        "D=R | XI=fam(0,1) | YI=0 | P=odd | T=2 | MAX=fam(1/2,2,1) | MIN=fam(-1/2,2,-1) | INF=fam(0,1,0) | VA=none | HA=none | R=[-1,1]",
    ),
    (
        "sin(x)/x",
        "D=(-inf,0)U(0,inf) | XI=fam(0,pi) | YI=none | P=even | T=none | MIN=inf | MAX=inf | INF=inf | VA=none | HA=0 | R=[-0.217234~,1)",
    ),
    (
        "sin(x)+sin(sqrt(2)*x)",
        "D=R | XI=inf | YI=0 | P=odd | T=none | MIN=inf | MAX=inf | INF=inf | VA=none | HA=none | R=(-2,2)",
    ),
    (
        "1/(x-1000000000000001)",
        "D=(-inf,1000000000000001)U(1000000000000001,inf) | XI=none | YI=-1/1000000000000001 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=1000000000000001 | HA=0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "1/(x-2)",
        "D=(-inf,2)U(2,inf) | XI=none | YI=-1/2 | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=2 | HA=0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "csch(x-1000)",
        "D=(-inf,1000)U(1000,inf) | XI=none | P=neither | T=none | MIN=none | MAX=none | VA=1000 | HA=0 | R=(-inf,0)U(0,inf)",
    ),
    (
        "e^(x-1000)",
        "D=R | XI=none | YI=0+ | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=L:0 | R=(0,inf)",
    ),
    (
        "atan(x)+0.000000001",
        "D=R | XI=-tan(0.000000001) | YI=0.000000001 | P=neither | T=none | MIN=none | MAX=none | INF=(0,0.000000001) | VA=none | HA=L:-pi/2+0.000000001;R:pi/2+0.000000001 | R=(-pi/2+0.000000001,pi/2+0.000000001)",
    ),
    (
        "atan(x)",
        "D=R | XI=0 | YI=0 | P=odd | T=none | MIN=none | MAX=none | INF=(0,0) | VA=none | HA=L:-pi/2;R:pi/2 | R=(-pi/2,pi/2)",
    ),
    (
        "(x-1000)^(x-1000)",
        "D=(1000,inf) | XI=none | YI=none | P=neither | T=none | MIN=(1000+1/e,e^(-1/e)) | MAX=none | INF=none | VA=none | HA=none | R=[e^(-1/e),inf)",
    ),
    (
        "x^-0.00001",
        "D=(0,inf) | XI=none | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=R:0 | R=(0,inf)",
    ),
    (
        "x^-0.0001",
        "D=(0,inf) | XI=none | YI=none | P=neither | T=none | MIN=none | MAX=none | INF=none | VA=0 | HA=R:0 | R=(0,inf)",
    ),
    (
        "10^-12*abs(x)",
        "D=R | XI=0 | YI=0 | P=even | T=none | MIN=(0,0) | MAX=none | VA=none | HA=none | R=[0,inf)",
    ),
    (
        "1000*x",
        "D=R | XI=0 | YI=0 | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=R",
    ),
    (
        "1000000000*x",
        "D=R | XI=0 | YI=0 | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=R",
    ),
    (
        "1e9*x",
        "D=R | XI=0 | YI=0 | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=R",
    ),
    (
        "1000000000000000*x",
        "D=R | XI=0 | YI=0 | P=odd | T=none | MIN=none | MAX=none | INF=none | VA=none | HA=none | R=R",
    ),
    (
        "0^(-x)",
        "D=(-inf,0) | XI=all | YI=none | T=none | VA=none | R={0}",
    ),
    ("atan(1/(x-x))", "D=empty"),
    ("ln(x-x)", "D=empty"),
    (
        "acot(1000000000000000)*1000000000000000",
        "D=R | XI=none | YI=1- | P=even | T=none | VA=none | R={1-}",
    ),
    (
        "acot(100000000000000000)*100000000000000000",
        "D=R | XI=none | YI=1- | P=even | T=none | VA=none | R={1-}",
    ),
    (
        "ln(exp(1000000000000000)^4)-4000000000000000",
        "D=R | XI=all | YI=0 | P=both | T=none | VA=none | R={0}",
    ),
    (
        "x/x",
        "D=(-inf,0)U(0,inf) | XI=none | YI=none | P=even | T=none | INF=none | VA=none | R={1}",
    ),
    (
        "tan(x)",
        "D=fam(pi/2,pi) | XI=fam(0,pi) | YI=0 | P=odd | T=pi | MIN=none | MAX=none | INF=fam(0,pi,0) | VA=fam(pi/2,pi) | HA=none | R=R",
    ),
    (
        "sqrt(x)",
        "D=[0,inf) | XI=0 | YI=0 | P=neither | T=none | MIN=(0,0) | MAX=none | INF=none | VA=none | HA=none | R=[0,inf)",
    ),
    (
        "x^3-2x+1/(x-1)",
        "D=(-inf,1)U(1,inf) | XI=-1.288795~;-0.389391~ | YI=-1 | P=neither | T=none | MAX=(-0.872759~,0.546759~) | MIN=(1.471580~,2.364148~) | INF=none | VA=1 | HA=none | R=(-inf,0.546759~]U[2.364148~,inf)",
    ),
    // Not from the reviews: a factor shared twice by one term and once by
    // the other (x·eˣ·(x + 1), zeros −1 and 0), which the zero-factor
    // reasoning once split wrongly.
    ("x*e^x*x+e^x*x", "D=R | XI=-1;0 | YI=0 | VA=none | HA=L:0"),
];

// ---------------------------------------------------------------- values

/// A true value: exactly `v` (a double within an ulp of it), within `tol`
/// of `v`, or just below/above the double `v`.
#[derive(Clone, Copy, Debug)]
enum Val {
    Exact(f64),
    Approx(f64, f64),
    Below(f64),
    Above(f64),
}

fn eval(s: &str) -> f64 {
    match s {
        "inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        _ => compile_str(s, TrigUnit::Radians)
            .unwrap_or_else(|e| panic!("truth value {s}: {e:?}"))
            .eval_x(0.0),
    }
}

fn val(s: &str) -> Val {
    let s = s.trim();
    if let Some(t) = s.strip_suffix('~') {
        let v = eval(t);
        Val::Approx(v, 1e-5 * v.abs() + 1e-300)
    } else if let Some(t) = s.strip_suffix('-').filter(|t| !t.is_empty()) {
        Val::Below(eval(t))
    } else if let Some(t) = s.strip_suffix('+').filter(|t| !t.is_empty()) {
        Val::Above(eval(t))
    } else {
        Val::Exact(eval(s))
    }
}

impl Val {
    fn mid(self) -> f64 {
        match self {
            Val::Exact(v) | Val::Approx(v, _) | Val::Below(v) | Val::Above(v) => v,
        }
    }

    /// Could the enclosure `[lo, hi]` hold the true value?
    fn fits(self, lo: f64, hi: f64) -> bool {
        match self {
            Val::Exact(v) if v.is_infinite() => lo == v && hi == v,
            Val::Exact(v) => {
                let slack = 2.0 * (v.abs().next_up() - v.abs()) + f64::from_bits(1);
                lo - slack <= v && v <= hi + slack
            }
            Val::Approx(v, tol) => lo - tol <= v && v <= hi + tol,
            Val::Below(v) => lo < v && hi >= v - 1e-15 * v.abs().max(1.0),
            Val::Above(v) => hi > v && lo <= v + 1e-15 * v.abs().max(1e-285),
        }
    }

    fn fits_enc(self, e: &Enc) -> bool {
        self.fits(e.lo.0, e.hi.0)
    }
}

/// Is the enclosed period `q` a whole multiple of `p`?
fn multiple(q: &Enc, p: f64) -> bool {
    let n = (q.mid() / p).round();
    n >= 1.0 && Val::Exact(n * p).fits(q.lo.0 - 1e-12 * q.mid(), q.hi.0 + 1e-12 * q.mid())
}

/// Is some member of `x0 + k·p` in the enclosure?
fn in_family(x0: f64, p: f64, e: &Enc) -> bool {
    let k = ((e.mid() - x0) / p).round();
    let t = x0 + k * p;
    let slack = 1e-9 * t.abs().max(1.0);
    e.lo.0 - slack <= t && t <= e.hi.0 + slack
}

/// The family members in `[a, b]`.
fn members(x0: f64, p: f64, a: f64, b: f64) -> Vec<f64> {
    let k0 = ((a - x0) / p).ceil() as i64;
    let k1 = ((b - x0) / p).floor() as i64;
    (k0..=k1).map(|k| x0 + k as f64 * p).collect()
}

// ---------------------------------------------------------------- truth

/// A set of x values.
#[derive(Clone, Debug)]
enum Xs {
    List(Vec<Val>),
    Fam(f64, f64),
    /// Infinitely many, not one family.
    Many,
    /// Every x of the domain.
    All,
}

/// A set of points (x, y).
#[derive(Clone, Debug)]
enum Pts {
    List(Vec<(Val, Val)>),
    Fam(f64, f64, Val),
    Many,
}

/// A union of intervals, or the line minus a family, or nothing.
#[derive(Clone, Debug)]
enum Set {
    Pieces(Vec<(Val, bool, Val, bool)>),
    LineMinus(f64, f64),
}

#[derive(Clone, Debug, Default)]
struct Truth {
    d: Option<Set>,
    xi: Option<Xs>,
    yi: Option<Option<Val>>,
    p: Option<&'static str>,
    t: Option<Option<Val>>,
    min: Option<Pts>,
    max: Option<Pts>,
    inf: Option<Pts>,
    va: Option<Xs>,
    ha: Option<Vec<(Tail, Val)>>,
    r: Option<Set>,
}

/// Splits at `sep` outside parentheses and brackets.
fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

fn fam_args(s: &str) -> Option<Vec<&str>> {
    let inner = s.strip_prefix("fam(")?.strip_suffix(')')?;
    Some(split_top(inner, ','))
}

fn xs(s: &str) -> Xs {
    match s {
        "none" => Xs::List(Vec::new()),
        "inf" => Xs::Many,
        "all" => Xs::All,
        _ => match fam_args(s) {
            Some(a) => Xs::Fam(eval(a[0]), eval(a[1])),
            None => Xs::List(split_top(s, ';').into_iter().map(val).collect()),
        },
    }
}

fn pts(s: &str) -> Pts {
    match s {
        "none" => Pts::List(Vec::new()),
        "inf" => Pts::Many,
        _ => match fam_args(s) {
            Some(a) => Pts::Fam(eval(a[0]), eval(a[1]), val(a[2])),
            None => Pts::List(
                split_top(s, ';')
                    .into_iter()
                    .map(|p| {
                        let inner = p.trim().strip_prefix('(').and_then(|p| p.strip_suffix(')'));
                        let xy = split_top(inner.unwrap_or_else(|| panic!("point {p}")), ',');
                        (val(xy[0]), val(xy[1]))
                    })
                    .collect(),
            ),
        },
    }
}

fn set(s: &str) -> Set {
    match s {
        "R" => Set::Pieces(vec![(
            Val::Exact(f64::NEG_INFINITY),
            false,
            Val::Exact(f64::INFINITY),
            false,
        )]),
        "empty" => Set::Pieces(Vec::new()),
        _ => {
            if let Some(a) = fam_args(s) {
                return Set::LineMinus(eval(a[0]), eval(a[1]));
            }
            if let Some(v) = s.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                let v = val(v);
                return Set::Pieces(vec![(v, true, v, true)]);
            }
            Set::Pieces(
                split_top(s, 'U')
                    .into_iter()
                    .map(|p| {
                        let p = p.trim();
                        let lo_closed = p.starts_with('[');
                        let hi_closed = p.ends_with(']');
                        let ends = split_top(&p[1..p.len() - 1], ',');
                        (val(ends[0]), lo_closed, val(ends[1]), hi_closed)
                    })
                    .collect(),
            )
        }
    }
}

fn truth(s: &'static str) -> Truth {
    let mut t = Truth::default();
    for field in s.split('|') {
        let (k, v) = field
            .trim()
            .split_once('=')
            .unwrap_or_else(|| panic!("field {field}"));
        let v = v.trim();
        match k {
            "D" => t.d = Some(set(v)),
            "XI" => t.xi = Some(xs(v)),
            "YI" => t.yi = Some((v != "none").then(|| val(v))),
            "P" => t.p = Some(v),
            "T" => t.t = Some((v != "none").then(|| val(v))),
            "MIN" => t.min = Some(pts(v)),
            "MAX" => t.max = Some(pts(v)),
            "INF" => t.inf = Some(pts(v)),
            "VA" => t.va = Some(xs(v)),
            "HA" => {
                t.ha = Some(if v == "none" {
                    Vec::new()
                } else if v.contains(':') {
                    v.split(';')
                        .map(|p| {
                            let (side, y) = p.split_once(':').expect("side");
                            (if side == "L" { Tail::Left } else { Tail::Right }, val(y))
                        })
                        .collect()
                } else {
                    vec![(Tail::Left, val(v)), (Tail::Right, val(v))]
                })
            }
            "R" => t.r = Some(set(v)),
            _ => panic!("key {k}"),
        }
    }
    t
}

// ---------------------------------------------------------------- checks

/// How a row came out, and what's wrong with it if it is.
struct Check {
    kind: char,
    wrong: Option<String>,
    /// Why the row is unknown.
    why: Option<String>,
}

fn why<T>(r: &Row<T>) -> Option<String> {
    match r {
        Row::Unknown { reason } => Some(reason.clone()),
        _ => None,
    }
}

fn kind<T>(r: &Row<T>) -> char {
    match r {
        Row::Certified { .. } => 'C',
        Row::Partial { .. } => 'P',
        Row::Unknown { .. } => 'U',
    }
}

fn region<T>(r: &Row<T>) -> Option<&Region> {
    match r {
        Row::Certified { cert, .. } | Row::Partial { cert, .. } => Some(&cert.covers),
        Row::Unknown { .. } => None,
    }
}

fn bound_fits(b: &Bound, v: Val, closed: bool, low: bool) -> bool {
    match b {
        Bound::NegInf => low && matches!(v, Val::Exact(x) if x == f64::NEG_INFINITY),
        Bound::PosInf => !low && matches!(v, Val::Exact(x) if x == f64::INFINITY),
        Bound::At { x, closed: c } => *c == closed && !v.mid().is_infinite() && v.fits_enc(x),
    }
}

fn pieces_fit(got: &[Piece], want: &[(Val, bool, Val, bool)]) -> bool {
    got.len() == want.len()
        && got.iter().zip(want).all(|(g, (lo, lc, hi, hc))| {
            bound_fits(&g.lo, *lo, *lc, true) && bound_fits(&g.hi, *hi, *hc, false)
        })
}

fn domain_fits(d: &DomainValue, want: &Set) -> bool {
    match want {
        Set::Pieces(p) => d.excluded.is_empty() && pieces_fit(&d.pieces, p),
        Set::LineMinus(x0, per) => {
            d.pieces.len() == 1
                && d.pieces[0].lo == Bound::NegInf
                && d.pieces[0].hi == Bound::PosInf
                && d.excluded.len() == 1
                && Val::Exact(*per).fits_enc(&d.excluded[0].period)
                && in_family(*x0, *per, &d.excluded[0].x0)
        }
    }
}

fn check_domain(r: &Row<DomainValue>, want: &Option<Set>) -> Option<String> {
    let (Row::Certified { value, .. } | Row::Partial { value, .. }) = r else {
        return None;
    };
    let want = want.as_ref()?;
    (!domain_fits(value, want)).then(|| format!("domain {value:?}, truth {want:?}"))
}

fn check_range(r: &Row<Vec<Piece>>, want: &Option<Set>) -> Option<String> {
    let (Row::Certified { value, .. } | Row::Partial { value, .. }) = r else {
        return None;
    };
    let ok = match want.as_ref()? {
        Set::Pieces(p) => pieces_fit(value, p),
        Set::LineMinus(..) => false,
    };
    (!ok).then(|| format!("range {value:?}, truth {want:?}"))
}

/// x positions against a set: a certified row must be the set, a partial
/// one may only list members (and over a window, all members in it).
fn check_xs(r: &Row<Vec<Spot>>, want: &Option<Xs>, name: &str) -> Option<String> {
    let want = want.as_ref()?;
    let (value, certified) = match r {
        Row::Certified { value, .. } => (value, true),
        Row::Partial { value, .. } => (value, false),
        Row::Unknown { .. } => return None,
    };
    let member = |e: &Enc| match want {
        Xs::List(l) => l.iter().any(|v| v.fits_enc(e)),
        Xs::Fam(x0, p) => in_family(*x0, *p, e),
        Xs::All => true,
        Xs::Many => true,
    };
    for s in value {
        let ok = match s {
            Spot::At(e) => member(e),
            Spot::Every(f) => match want {
                Xs::Fam(x0, p) => multiple(&f.period, *p) && in_family(*x0, *p, &f.x0),
                _ => false,
            },
        };
        if !ok {
            return Some(format!("{name}: {s:?} is not in the truth {want:?}"));
        }
    }
    // Completeness: everything on the line, or in the window.
    let window = match region(r) {
        Some(Region::Line) if certified => Some((f64::NEG_INFINITY, f64::INFINITY)),
        Some(Region::Window { a, b } | Region::Period { a, b }) => Some((a.0, b.0)),
        _ => None,
    };
    let (a, b) = window?;
    let listed = |t: f64| {
        value.iter().any(|s| match s {
            Spot::At(e) => Val::Exact(t).fits_enc(e),
            Spot::Every(f) => in_family(t, f.period.mid(), &f.x0),
        })
    };
    let missing = match want {
        Xs::List(l) => l.iter().filter(|v| v.mid() >= a && v.mid() <= b).find(|v| {
            !value
                .iter()
                .any(|s| matches!(s, Spot::At(e) if v.fits_enc(e)))
        }),
        Xs::Fam(x0, p) => {
            if a.is_infinite() || b.is_infinite() {
                let whole = value
                    .iter()
                    .any(|s| matches!(s, Spot::Every(f) if in_family(*x0, *p, &f.x0)));
                return (!whole)
                    .then(|| format!("{name}: certified {value:?} for the family {x0} + k·{p}"));
            }
            return members(*x0, *p, a, b)
                .into_iter()
                .find(|t| !listed(*t))
                .map(|t| format!("{name}: {t} missing over [{a}, {b}] ({value:?})"));
        }
        Xs::Many | Xs::All => {
            return (a.is_infinite() || b.is_infinite())
                .then(|| format!("{name}: certified complete {value:?} for infinitely many"));
        }
    };
    missing.map(|v| format!("{name}: {v:?} missing ({value:?})"))
}

/// Points (x, y) against a set, as [`check_xs`].
fn check_pts(
    r: &Row<Vec<(Enc, Enc, Option<Enc>)>>,
    want: &Option<Pts>,
    name: &str,
) -> Option<String> {
    let want = want.as_ref()?;
    let (value, certified) = match r {
        Row::Certified { value, .. } => (value, true),
        Row::Partial { value, .. } => (value, false),
        Row::Unknown { .. } => return None,
    };
    for (x, y, every) in value {
        let ok = match (want, every) {
            (Pts::List(l), None) => l.iter().any(|(tx, ty)| tx.fits_enc(x) && ty.fits_enc(y)),
            (Pts::List(_), Some(_)) => false,
            (Pts::Fam(x0, p, ty), _) => {
                in_family(*x0, *p, x) && ty.fits_enc(y) && every.is_none_or(|q| multiple(&q, *p))
            }
            (Pts::Many, _) => true,
        };
        if !ok {
            return Some(format!(
                "{name}: ({x:?}, {y:?}) is not in the truth {want:?}"
            ));
        }
    }
    let window = match region(r) {
        Some(Region::Line) if certified => Some((f64::NEG_INFINITY, f64::INFINITY)),
        Some(Region::Window { a, b } | Region::Period { a, b }) => Some((a.0, b.0)),
        _ => None,
    };
    let (a, b) = window?;
    let listed = |t: f64| {
        value.iter().any(|(x, _, every)| match every {
            None => Val::Exact(t).fits_enc(x),
            Some(q) => in_family(t, q.mid(), x),
        })
    };
    match want {
        Pts::List(l) => l
            .iter()
            .filter(|(tx, _)| tx.mid() >= a && tx.mid() <= b)
            .find(|(tx, _)| !value.iter().any(|(x, _, _)| tx.fits_enc(x)))
            .map(|t| format!("{name}: {t:?} missing ({value:?})")),
        Pts::Fam(x0, p, _) => {
            if a.is_infinite() || b.is_infinite() {
                return Some(format!("{name}: certified complete {value:?} for a family"));
            }
            members(*x0, *p, a, b)
                .into_iter()
                .find(|t| !listed(*t))
                .map(|t| format!("{name}: {t} missing over [{a}, {b}] ({value:?})"))
        }
        Pts::Many => (a.is_infinite() || b.is_infinite())
            .then(|| format!("{name}: certified complete {value:?} for infinitely many")),
    }
}

fn map_row<T, U>(r: &Row<T>, f: impl Fn(&T) -> U) -> Row<U> {
    match r {
        Row::Certified { value, cert } => Row::Certified {
            value: f(value),
            cert: cert.clone(),
        },
        Row::Partial { value, cert } => Row::Partial {
            value: f(value),
            cert: cert.clone(),
        },
        Row::Unknown { reason } => Row::Unknown {
            reason: reason.clone(),
        },
    }
}

fn check_scalar<T: std::fmt::Debug>(
    r: &Row<T>,
    ok: impl Fn(&T) -> Option<bool>,
    name: &str,
) -> Option<String> {
    let (Row::Certified { value, .. } | Row::Partial { value, .. }) = r else {
        return None;
    };
    (!ok(value)?).then(|| format!("{name}: {value:?}"))
}

/// The rows, in table order.
const ROWS: [&str; 11] = [
    "D", "XI", "YI", "P", "T", "EXT", "INF", "MON", "R", "VA", "HA",
];

fn checks(a: &Analysis, t: &Truth) -> Vec<Check> {
    let ext = |k: ExtKind| {
        map_row(&a.extrema, |v| {
            v.iter()
                .filter(|e| e.kind == k)
                .map(|e| (e.x, e.y, e.every))
                .collect::<Vec<_>>()
        })
    };
    let ext_wrong = check_pts(&ext(ExtKind::Min), &t.min, "minima")
        .or_else(|| check_pts(&ext(ExtKind::Max), &t.max, "maxima"));
    let inf = map_row(&a.inflections, |v| {
        v.iter().map(|i| (i.x, i.y, i.every)).collect::<Vec<_>>()
    });
    let yi = check_scalar(
        &a.y_intercept,
        |v| {
            let want = t.yi.as_ref()?;
            Some(match (v, want) {
                (None, None) => true,
                (Some(e), Some(w)) => w.fits_enc(e),
                _ => false,
            })
        },
        "y-intercept",
    );
    let parity = check_scalar(
        &a.parity,
        |v| {
            let want = t.p?;
            Some(match want {
                "both" => matches!(v, Parity::Even | Parity::Odd),
                "even" => *v == Parity::Even,
                "odd" => *v == Parity::Odd,
                _ => *v == Parity::Neither,
            })
        },
        "parity",
    );
    let period = check_scalar(
        &a.period,
        |v| {
            let want = t.t.as_ref()?;
            Some(match (v, want) {
                (None, None) => true,
                (Some(e), Some(w)) => w.fits_enc(e),
                _ => false,
            })
        },
        "period",
    );
    let ha = check_scalar(
        &a.horizontal,
        |v| {
            let want = t.ha.as_ref()?;
            Some(
                v.len() == want.len()
                    && want
                        .iter()
                        .all(|(side, y)| v.iter().any(|h| h.side == *side && y.fits_enc(&h.y))),
            )
        },
        "horizontal",
    );
    let wrongs = [
        check_domain(&a.domain, &t.d),
        check_xs(&a.x_intercepts, &t.xi, "x-intercepts"),
        yi,
        parity,
        period,
        ext_wrong,
        check_pts(&inf, &t.inf, "inflections"),
        None,
        check_range(&a.range, &t.r),
        check_xs(&a.vertical, &t.va, "vertical"),
        ha,
    ];
    let kinds = [
        (kind(&a.domain), why(&a.domain)),
        (kind(&a.x_intercepts), why(&a.x_intercepts)),
        (kind(&a.y_intercept), why(&a.y_intercept)),
        (kind(&a.parity), why(&a.parity)),
        (kind(&a.period), why(&a.period)),
        (kind(&a.extrema), why(&a.extrema)),
        (kind(&a.inflections), why(&a.inflections)),
        (kind(&a.monotonicity), why(&a.monotonicity)),
        (kind(&a.range), why(&a.range)),
        (kind(&a.vertical), why(&a.vertical)),
        (kind(&a.horizontal), why(&a.horizontal)),
    ];
    kinds
        .into_iter()
        .zip(wrongs)
        .map(|((kind, why), wrong)| Check { kind, wrong, why })
        .collect()
}

/// The current engine on `src`: per row, answered ('A') or refused ('R').
fn engine(src: &str) -> [char; 11] {
    let r = analyze_str(&format!("y={src}"));
    let refused = |f: u32| {
        if r.too_complex_features & f != 0 {
            'R'
        } else {
            'A'
        }
    };
    if r.analysis_error_string().is_some() {
        return ['R'; 11];
    }
    [
        refused(flags::DOMAIN),
        refused(flags::ZEROS),
        refused(flags::Y_INTERCEPT),
        refused(flags::PARITY),
        refused(flags::PERIODICITY),
        refused(flags::MINIMA | flags::MAXIMA),
        refused(flags::INFLECTION_POINTS),
        refused(flags::MONOTONE_INTERVALS),
        refused(flags::RANGE),
        refused(flags::VERTICAL_ASYMPTOTES),
        refused(flags::HORIZONTAL_ASYMPTOTES),
    ]
}

/// The engine's grades on the truth table (C right, P partly, W wrong, R
/// refused), mapped to the same rows (its ASY covers VA and HA).
fn engine_grades(id: &str, variant: &str) -> Option<[char; 11]> {
    let line = ENGINE.lines().find(|l| {
        let mut f = l.split('\t');
        f.next() == Some(id) && f.next() == Some(variant)
    })?;
    let g: Vec<char> = line
        .split('\t')
        .skip(2)
        .map(|s| s.chars().next().unwrap_or('?'))
        .collect();
    // id var D XI YI P T R EXT INF ASY MON
    Some([
        g[0], g[1], g[2], g[3], g[4], g[6], g[7], g[9], g[5], g[8], g[8],
    ])
}

struct Run {
    set: &'static str,
    label: String,
    checks: Vec<Check>,
    engine: [char; 11],
    ms: f64,
    evals: u64,
}

fn run(set: &'static str, label: String, src: &str, t: &Truth, engine: [char; 11]) -> Run {
    let start = Instant::now();
    let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None);
    let ms = start.elapsed().as_secs_f64() * 1e3;
    let (checks, evals) = match a {
        Ok(a) => {
            assert!(
                a.evals <= DEFAULT_BUDGET,
                "{label}: {} evaluations, over the budget",
                a.evals
            );
            (checks(&a, t), a.evals)
        }
        // Not analysed: every row unknown.
        Err(e) => (
            ROWS.iter()
                .map(|_| Check {
                    kind: 'U',
                    wrong: None,
                    why: Some(e.clone()),
                })
                .collect(),
            0,
        ),
    };
    Run {
        set,
        label,
        checks,
        engine,
        ms,
        evals,
    }
}

#[test]
fn no_row_is_certified_wrong() {
    let mut runs = Vec::new();
    for line in CASES.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let (id, variant, src) = (f[0], f[1], f[2]);
        let t = TRUTH
            .iter()
            .find(|(i, _)| *i == id)
            .unwrap_or_else(|| panic!("no truth for {id}"));
        let set = if id.starts_with('K') {
            "kgfset"
        } else {
            "adversarial"
        };
        let grades = engine_grades(id, variant).unwrap_or(['?'; 11]);
        runs.push(run(set, format!("{id} {src}"), src, &truth(t.1), grades));
    }
    for (src, t) in REVIEW {
        runs.push(run("review", src.to_string(), src, &truth(t), engine(src)));
    }

    // Per function.
    println!(
        "\n{:<58} {:>8} {:>8}  rows {}",
        "function",
        "ms",
        "evals",
        ROWS.join(" ")
    );
    for r in &runs {
        let kinds: String = r.checks.iter().map(|c| format!("{:>3}", c.kind)).collect();
        println!(
            "{:<58} {:>8.1} {:>8} {kinds}",
            trunc(&r.label, 58),
            r.ms,
            r.evals
        );
    }

    // Why each kgfset row is unknown.
    println!("\nkgfset rows still unknown:");
    for r in runs.iter().filter(|r| r.set == "kgfset") {
        for (c, row) in r.checks.iter().zip(ROWS) {
            if let Some(why) = &c.why {
                println!("  {:<40} {row:<4} {why}", trunc(&r.label, 40));
            }
        }
    }

    // Coverage per row and set, beside the current engine's.
    println!(
        "\ncertified/partial/unknown per row (engine: C/P/W/R graded on the truth table; A/R answered or refused on the reviews)"
    );
    for set in ["kgfset", "adversarial", "review"] {
        let rs: Vec<&Run> = runs.iter().filter(|r| r.set == set).collect();
        println!("\n{set} ({} functions)", rs.len());
        println!(
            "{:<5} {:>10} {:>8} {:>8}   engine",
            "row", "certified", "partial", "unknown"
        );
        for (i, row) in ROWS.iter().enumerate() {
            let count = |k: char| rs.iter().filter(|r| r.checks[i].kind == k).count();
            let mut grades: Vec<(char, usize)> = Vec::new();
            for r in &rs {
                let g = r.engine[i];
                match grades.iter_mut().find(|(c, _)| *c == g) {
                    Some((_, n)) => *n += 1,
                    None => grades.push((g, 1)),
                }
            }
            grades.sort();
            let eng: Vec<String> = grades.iter().map(|(c, n)| format!("{c}{n}")).collect();
            println!(
                "{row:<5} {:>10} {:>8} {:>8}   {}",
                count('C'),
                count('P'),
                count('U'),
                eng.join(" ")
            );
        }
        let answered = rs
            .iter()
            .filter(|r| r.checks.iter().all(|c| c.kind != 'U'))
            .count();
        println!(
            "fully answered (every row certified or partial): {answered}/{}",
            rs.len()
        );
        let mut ms: Vec<f64> = rs.iter().map(|r| r.ms).collect();
        ms.sort_by(f64::total_cmp);
        let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
        let total: f64 = ms.iter().sum();
        println!(
            "time: median {:.0} ms, p90 {:.0} ms, at most {:.0} ms, {total:.0} ms in all",
            at(0.5),
            at(0.9),
            at(1.0)
        );
    }

    let wrong: Vec<String> = runs
        .iter()
        .flat_map(|r| {
            r.checks.iter().zip(ROWS).filter_map(move |(c, row)| {
                c.wrong
                    .as_ref()
                    .map(|w| format!("{} [{row}]: {w}", r.label))
            })
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "rows certified wrong:\n{}",
        wrong.join("\n")
    );
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
