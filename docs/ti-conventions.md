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

Applies to graphed equations and their analysis (`crates/graphing`:
`functions::pow`, `pow_var`, `pow_int`, `pow_rational`, the compiler's `Pow`
and `PowVar` instructions, the extended-range and reference evaluators). The
calculator modes are unchanged (see 0⁰ below).

| Expression | Windows Calculator | TI-84 Plus CE (Real mode) | GMNB/DGMNB graphing |
| --- | --- | --- | --- |
| `x^x`, `(x−1)^x`, `2^x` (exponent varies with x) | graphing engine closed source | defined only for a positive base (a negative base gives a non-real answer, which isn't plotted) | **TI:** positive base only, or 0 to a positive power. `x^x` has domain x > 0 and no y-intercept |
| `(−2)^x` | — | non-real, nothing plotted, even at whole x | **TI:** undefined everywhere |
| `0^0`, so `x^0` at x = 0 | 1 in the calculator modes | ERR:DOMAIN | **TI:** undefined, so `x^0` is 1 with a hole at 0 |
| `x^3`, `x^(1/3)`, `(−8)^(1/3)` (constant exponent) | real powers | real powers and odd roots | unchanged: real powers and odd roots (`(−8)^(1/3)` = −2) |
| `x^0.5`, `x^π` at x < 0 | — | non-real | undefined (unchanged) |

Why: a power whose exponent varies with x is `e^(exponent·ln base)` in real
analysis. It has no real value for a negative base except at isolated points
(a whole exponent, or a fraction with an odd denominator), which no graph or
analysis can show faithfully. Treating it as defined there made the analysis
claim points no curve has (x^x at −1, −2, …). The TI's rule, which is also
IEEE 1788's `pow`, is simple, standard and exactly what gets plotted. A
constant exponent is different: `x^3` and `x^(1/3)` are polynomial and root
functions, defined for negative x, and both calculators graph them on both
sides.

0⁰: the limit of x^y at (0, 0) doesn't exist, so a function graphed through
it has no value there (x⁰ approaches 1 but 0^x approaches 0). The TI reports a
domain error. Windows' calculator modes give 1, the common algebraic
convention for a typed number, and keep doing so here: those modes are
tested against Microsoft's own engine.

## The certified analysis

Applies to `crates/graphing/src/certify`, the proof-carrying analysis that
will back the panel. Its rows follow what the panel already shows (Windows'
key-graph-features rows), with these choices where that isn't settled:

| Row | Choice | Why |
| --- | --- | --- |
| Minima, maxima | strict turning points only: where f′ changes sign. A closed end of the domain (√x at 0) is not listed | Windows' panel lists turning points, as the current engine does; the TI's fMin/fMax answer a different question (an extremum over an interval the user picks) |
| Minima, maxima, inflections of a constant stretch (`x/x`) | none: no strict turn, no change of concavity | the same rule; nothing to point at |
| Monotonicity of a constant stretch | "constant" | Windows' panel has that text (`GraphingEnums.h`) |
| Period of a constant function | not periodic | every number is a period, none is the least; the current engine and the simplifier say "not periodic" |
| Features of a periodic function | one family `x₀ + k·P` per feature in one period | Windows' panel shows families; P is the period the simplifier proves |
| Horizontal asymptote | the limit is proven to an enclosure; shown exactly only when the simplifier proves it exactly | a value the panel rounds is not a proof; how the panel shows "y ≈ 0.5" is decided when it is wired in |
