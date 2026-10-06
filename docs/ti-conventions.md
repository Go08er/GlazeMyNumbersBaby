# Conventions borrowed from the TI-84 Plus CE

GMNB and DGMNB follow Windows Calculator. Where the TI-84 Plus CE does
something better (more correct, or more useful), they follow the TI-84 Plus CE
instead, and the choice is recorded here with its reason.

In graphing, the TI-84 Plus CE is also the default wherever Windows' behaviour
isn't known: its graphing engine is closed source, so only what the open
repository shows (tracing steps and precision, the analysis panel's texts) is
known to be Windows'.

"TI-84 Plus CE" here names a documented behaviour to match, nothing more: no
TI code, ROM, names or artwork are used.

## Powers in graphing

**Status: a chosen convention, not verified device parity.** TI's public
documentation confirms real odd-denominator roots of negative bases
(`(−2)^(3/5)`), but doesn't establish the rule below for exponents that vary
with x, nor the CE's literal `0^0` (TI's own article names a domain error for
the 82/83/85 and 1 for the 89/92, and doesn't mention the CE). The rule is
the continuous real branch `b^e = e^(e·ln b)`, chosen because it is what a
graph can show faithfully; the TI column records what its graphs show in
practice. Whether the exponent "varies with x" is decided on the expression as
typed (the original tree), so `x^(1+x−x)` keeps the rule even though it
simplifies to `x^1`.

Applies to graphed equations and their analysis (`crates/graphing`:
`functions::pow`, `pow_var`, `pow_int`, `pow_rational`, the compiler's `Pow`
and `PowVar` instructions, the extended-range and reference evaluators). The
calculator modes are unchanged (see 0⁰ below).

| Expression | Windows Calculator | TI-84 Plus CE (Real mode) | GMNB/DGMNB graphing |
| --- | --- | --- | --- |
| `x^x`, `(x−1)^x`, `2^x` (exponent varies with x) | graphing engine closed source | defined only for a positive base (a negative base gives a non-real answer, which isn't plotted) | **Chosen:** positive base only, or 0 to a positive power. `x^x` has domain x > 0 and no y-intercept |
| `(−2)^x` | — | non-real, nothing plotted, even at whole x | **Chosen:** undefined everywhere |
| `0^0`, so `x^0` at x = 0 | 1 in the calculator modes | not documented for the CE (DOMAIN on the 82/83/85) | **Chosen:** undefined, so `x^0` is 1 with a hole at 0 |
| `x^3`, `x^(1/3)`, `x^(−2/5)`, `(−8)^(1/3)` (exponent written as an integer or a ratio of integers) | real powers | real powers and odd roots | unchanged: real powers and odd roots (`x^(1/5)` is defined on all of ℝ; `(−8)^(1/3)` = −2) |
| `x^0.2`, `x^a` with a slider at 0.2, `x^1.0000000000000001` (any other constant exponent that is not exactly an integer) | — | not established | **Chosen:** the rule for an exponent that varies with x: `x^0.2` has domain [0, ∞), and a slider set to 0.2 behaves the same |
| `x^(0.5·2)`, `x^2.0`, `x^(0.1+0.9)` (a constant exponent exactly an integer, written otherwise) | — | not established | **Chosen:** the integer power (`x^(0.1+0.9)` is x, defined everywhere) |
| `x^0.5`, `x^π` at x < 0 | — | non-real | undefined (unchanged) |

Why: a power whose exponent varies with x is `e^(exponent·ln base)` in real
analysis. It has no real value for a negative base except at isolated points
(a whole exponent, or a fraction with an odd denominator), which no graph or
analysis can show faithfully. Treating it as defined there made the analysis
claim points no curve has (x^x at −1, −2, …). The rule is IEEE 1788's
`pow` (a positive base, or 0 to a positive power): simple, standard and
exactly what gets plotted. An exponent written as an integer or a ratio of
integers (`3`, `−2`, `1/3`, `(2/3)`, `−1/3`; taken in lowest terms) is
different and follows IEEE 1788's `pown`/`rootn`: `x^3` and `x^(1/3)` are
polynomial and root functions, defined for negative x, and both calculators
graph them on both sides. Only that written form gets odd roots (the
compiler's `syntactic_rational`): any other constant exponent, a decimal, a
slider or an expression, takes the positive-base rule unless its value is
an integer, even when it is a fraction with an odd denominator, so `x^0.2`
has domain [0, ∞) while `x^(1/5)` is defined everywhere.

**Literals are the decimals typed, to the digits set.** Each number typed in
an equation is rounded on entry to a number of significant decimal digits,
half away from zero, which a setting chooses: by default the TI-84 Plus
CE's 14 (its numbers are 14-digit decimals; it shows 10). The settings
slider also marks 10 (Casio's display), 12 (the HP Prime's Home view) and
15 (Casio's internal precision, and the most digits every double tells
apart); anything from 5 to 20 can be set, or off. The rounded decimal is
the number from then on, for the curve drawn, its trace, its analysis and
its certificate alike; off, the number is exactly the decimal typed. A
slider is read the same way: off, its value is the double it is set to;
under a digit limit it is, like a number typed, that value rounded to so
many digits, and that decimal from then on (the TI stores the decimal
too). To 14 digits a slider `a` set to 0.3 (or stepped to
0.30000000000000004) is 3/10, so `10^17·(a − 0.3) + x` is the line y = x;
off it is the double 0.299999999999999988897…, and that line is
x − 1.11022….

A number is that decimal, not the double nearest it: `0.1` is one tenth;
off (or at 16 digits or more) `1.0000000000000001` is 1 + 10⁻¹⁶, though a
double holds it as 1, while to 14 digits it is 1. Two numbers that read
differently to 15 digits or fewer are never the same double. With more
digits, or off, they can be, and each occurrence is still its own number:
off, `1.0000000000000001·x − 1·x` is 10⁻¹⁶·x, not 0, and beside a slider
`a` at 1, `a·x − 1.0000000000000001·x` is −10⁻¹⁶·x. Whether an exponent (or
a root's degree) is an integer is decided by its exact value, never by its
double: off, `x^1.0000000000000001` and `x^2.0000000000000001` take the
positive-base rule (domain [0, ∞), a minimum at 0), while `x^(0.1+0.9)` and
`x^2.0` are integer powers; only the integers of the written form `p/q`
must be typed exactly so to give odd roots. Likewise an exponent or a
root's degree typed as an odd integer is odd at any size: off (or at 16
digits or more), `x^9007199254740993` is −1 at −1 and
`root(−8, 9007199254740993)` is defined (just below −1), though the doubles
either side of that number are even; so is `root(−8, n)` for an odd n of
hundreds of digits. An exponent too long to carry exactly (`3^20000`) is
odd, even or no integer as its form shows (3^20000 is odd, 10^5000 even,
1.5^100000 no integer); where that isn't known (`x^nCr(2000, 1000)`), a
negative base's power is unknown. Arithmetic on literals and
sliders alone (+, −, ×, ÷, whole powers, |·| and the counts n!, n!!, nCr,
nPr of whole numbers of any size) is done exactly and rounded once
(`10^17·(0.1 + 0.2 − 0.3)` is 0, `0.1·3` is the double nearest 0.3,
`1/(0.1 + 0.2 − 0.3)` divides by zero, and off
`nCr(9007199254740993, 1) − 9007199254740992` is 1, though no double holds
the first number), for the curve drawn, its trace and
its analysis alike, while every exact value on the way fits in 2¹⁴ bits
(about 4,900 digits). Past that the arithmetic isn't carried out. A value
proven beyond the doubles on its own (`10^5000`, `171!`, `(1/2)^100000`)
is still the ±∞ or 0 it rounds to, but arithmetic on such a value has an
unknown value, not what rounding step by step would make of it:
`(10^5000 + 1) − 10^5000` is unknown, never 0. Nothing is drawn or traced
for an unknown value except where f's enclosure itself places the curve.
Anything else (π, e, sin, …) is computed in floating point as before. A
literal the doubles can't tell from its neighbour (off, 1 + 10⁻¹⁶ against
1) is enclosed by the doubles either side of it, so the analysis may leave
a row unknown that only that difference decides (off,
`2/(1.0000000000000001 − cos x)` is defined everywhere, but its domain row
says it can't tell).

0⁰: the limit of x^y at (0, 0) doesn't exist, so a function graphed through
it has no value there (x⁰ approaches 1 but 0^x approaches 0). The TI reports a
domain error. Windows' calculator modes give 1, the common algebraic
convention for a typed number, and keep doing so here: those modes are
tested against Microsoft's own engine.

## The certified analysis

Applies to the function analysis panel in both apps: `crates/graphing/src/certify`
proves each row, and `crates/graphing/src/analysis/certified.rs` writes the
panel from the proofs. Its rows follow Windows' key-graph-features panel; where
Windows' behaviour is known it is followed, otherwise the TI-84 Plus CE's,
otherwise the mathematically clearest choice, as below.

| Row | Choice | Why |
| --- | --- | --- |
| Minima, maxima | strict local extrema, never global ones: where f′ changes sign, **and a closed end of the domain where f rises or falls away from it** (√x has a minimum (0, 0); asin x has (−1, −π/2) and (1, π/2)) | Windows lists endpoint extrema; a local extremum at a domain end is one by the textbook definition |
| Minima, maxima, inflections of a constant stretch (`x/x`) | none: no strict turn, no change of concavity | a plateau has no strict extremum; nothing to point at |
| Monotonicity | each piece open, `(a, b)`, strictly increasing or decreasing on it; "constant" where f′ ≡ 0 | Windows' panel shows open intervals and has the "constant" text (`GraphingEnums.h`) |
| Parity of the zero function | "both even and odd" (on a domain symmetric about 0) | it is both; neither "even" nor "odd" alone is the whole answer |
| Period of a constant function | "constant: it has no fundamental period" | every number is a period and none is the least; "not periodic" would be false |
| Asymptotes of a line (a constant, m·x + b, holes aside) | none | its graph is the line itself, not something it approaches |
| Features of a periodic function | one family `x₀ + k·P` per feature (families evenly spaced by P/n merged: `kπ`) | Windows' panel shows families; P is the period the simplifier proves |
| Horizontal asymptotes | one line `y = c` (for both ends) when f approaches the same c at −∞ and +∞, exactly or alike to fifteen digits; otherwise one per end, +∞ first | one asymptote is one line; the original lists +∞ first |
| A list proven correct but not complete | shown, with a note: "Complete for a ≤ x ≤ b; there may be more outside." or "These are some of them; there may be more." (none found in the window: "Unable to calculate …" with "None for a ≤ x ≤ b; …") | the items are proven; claiming "that's all" isn't |
| A row not proven | "Unable to calculate …" (parity, periodicity and monotonicity: "… is unknown"), listed in the too-complex footer, never "none" | "none" is a claim; an unfinished proof (budget, cancellation) is not one |

How numbers are written:

* **Exactly**, only when proven exact: a double the certifier pinned, or a
  closed form (`crates/graphing/src/analysis/exact.rs`: a rational, a + b√c,
  a rational multiple of π, q·eᵏ and a + b·ln c with q, k, a, b, c rational,
  and the values of sin, cos, tan and their inverses at multiples of π/6
  and π/4) that exact arithmetic checks
  against the claim pinning the point down (f, f′ or f″ is 0 there exactly,
  or a side expression takes its level, on a box where that crossing is
  proven unique); excluded families from the trigonometric table; a value
  f takes at an exact point; an exact limit (rational functions, the
  simplifier's limits and periods). A rounded pole or hole is never shown
  as exact.
* **Otherwise to as many significant digits as the enclosure fixes, from
  three to six** (every value in it rounds alike to that many; large values
  too get no more than six), marked "≈" because the value isn't exact
  (`≈1`, `≈0.5`, `≈1.41421`: trailing zeros trimmed). Values of 10⁶ and
  more are written m×10ⁿ (`≈2.30062×10⁶`), the form chosen from the value
  rounded to those digits, so both ends of an enclosure straddling an
  integer or a power of ten read alike (`≈1×10⁶`); an exact integer keeps
  its exact text (`1000000`). Accessibility reads "≈" as
  "approximately". **The one exception to six:** two different numbers of
  a row that would read alike (two points, two excluded points or
  families, the two bounds of a range or of a monotone piece, and the ends
  of two pieces proven different, as about a gap) get the significant
  digits that tell them apart, up to fifteen (`x ∈ ℝ \ {≈0.841471,
  ≈0.8414711}`, `y ∈ [≈0.8414711, ≈0.8414713]`, `x ∈ (−∞, ≈0.841471] ∪
  [≈0.8414711, ∞)`); two ends that may be one number may read alike
  (`y ∈ (−∞, ≈1) ∪ (≈1, ∞)` for csch x + 1). Equal texts never make two
  numbers one:
  a set is written as a single point `{c}` only when its ends are proven
  one number (exact and equal, the same double, or one value f takes at
  one place). The minimum is one switch,
  `MIN_SHOWN_DIGITS` in `crates/graphing/src/analysis/certified.rs`: 6
  restores the strict rule (all six digits fixed, or the row is unknown).
  Tracing shares it (`crates/graphing/src/trace.rs`): a traced value with
  fewer digits fixed reads "unknown".
* **Not at all** when fewer than three digits are fixed: the row is
  unknown. Two different numbers (points, lines, excluded points or
  bounds) of a row that still read alike at fifteen digits leave it
  unknown too (they can't be told apart). A value
  that may be 0 is never written "0" or "≈0", and a closed range bound (a
  value f takes) is written only when exact or known to its last few
  doubles: "0", "none" and "attained" get no rounding allowance.
