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
| `x^3`, `x^(1/3)`, `(−8)^(1/3)` (constant exponent) | real powers | real powers and odd roots | unchanged: real powers and odd roots (`(−8)^(1/3)` = −2) |
| `x^0.5`, `x^π` at x < 0 | — | non-real | undefined (unchanged) |

Why: a power whose exponent varies with x is `e^(exponent·ln base)` in real
analysis. It has no real value for a negative base except at isolated points
(a whole exponent, or a fraction with an odd denominator), which no graph or
analysis can show faithfully. Treating it as defined there made the analysis
claim points no curve has (x^x at −1, −2, …). The rule is IEEE 1788's
`pow` (a positive base, or 0 to a positive power): simple, standard and
exactly what gets plotted. A constant exponent is different and follows IEEE
1788's `pown`/`rootn`: `x^3` and `x^(1/3)` are polynomial and root functions,
defined for negative x, and both calculators graph them on both sides.

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
| A list proven correct but not complete | shown, with a note: "Complete for a ≤ x ≤ b; there may be more outside." or "These are some of them; there may be more." (none found in the window: "Unable to calculate …" with "None for a ≤ x ≤ b; …") | the items are proven; claiming "that's all" isn't |
| A row not proven | "Unable to calculate …", listed in the too-complex footer, never "none" | "none" is a claim; an unfinished proof (budget, cancellation) is not one |

How numbers are written:

* **Exactly**, only when proven exact: a double the certifier pinned, or a
  closed form (a rational, a + b√c, a rational multiple of π, the special
  values of sin, cos, tan and their inverses) that exact arithmetic checks
  against the claim pinning the point down (f, f′ or f″ is 0 there exactly,
  or a side expression takes its level, on a box where that crossing is
  proven unique); excluded families from the trigonometric table; a value
  f takes at an exact point; an exact limit (rational functions, the
  simplifier's limits and periods). A rounded pole or hole is never shown
  as exact.
* **Otherwise to as many significant digits as the enclosure fixes, up to
  six and at least three** (every value in it rounds alike to that many),
  marked "≈" when the text has fewer than six (`≈1`, `≈0.5`, `≈1.414`:
  trailing zeros trimmed). No decimal is written past the sixth
  significant digit (`166254`, not `166253.7622`); below 10⁹ the digits
  before the point all stay (`545843449`), so a value and the integer it
  rounds to read alike. Accessibility reads "≈" as "approximately". The
  minimum is one switch, `MIN_SHOWN_DIGITS` in
  `crates/graphing/src/analysis/certified.rs`: 6 restores the strict rule
  (all six digits fixed, or the row is unknown).
* **Not at all** when fewer than three digits are fixed: the row is
  unknown. Two points or lines of a row that still read alike at fifteen
  digits leave it unknown too (they can't be told apart). A value
  that may be 0 is never written "0" or "≈0", and a closed range bound (a
  value f takes) is written only when exact or known to its last few
  doubles: "0", "none" and "attained" get no rounding allowance.
