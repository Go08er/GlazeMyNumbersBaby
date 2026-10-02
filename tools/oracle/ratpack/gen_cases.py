#!/usr/bin/env python3
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
#
# Deterministic generator of differential-test commands for the ratpack
# oracle (driver.cpp). Writes one command per line to stdout.
#
# Rationals are written as "P/Q" with each Number "sign:exp:d0,d1,..." in the
# internal base 2^31, least significant digit first, or as "$reg" for a
# register set by an earlier "@reg COMMAND" line.

import random
import sys
from decimal import Decimal, getcontext
from fractions import Fraction as F

BASEX = 2**31
rng = random.Random(0x5EED_CA1C)
getcontext().prec = 220

out = []


def emit(line):
    out.append(line)


# ---------------------------------------------------------------------------
# Encoding helpers
# ---------------------------------------------------------------------------


def digits(n):
    assert n >= 0
    if n == 0:
        return [0]
    d = []
    while n:
        d.append(n % BASEX)
        n //= BASEX
    return d


def enc_num(sign, exp, n):
    return f"{sign}:{exp}:{','.join(map(str, digits(n)))}"


def enc_raw(sign, exp, ds):
    return f"{sign}:{exp}:{','.join(map(str, ds))}"


def frac(x):
    """Encodes an int / Fraction / decimal string as P/Q with exps 0."""
    if isinstance(x, str):
        x = F(Decimal(x))
    x = F(x)
    p, q = x.numerator, x.denominator
    s = -1 if p < 0 else 1
    return enc_num(s, 0, abs(p)) + "/" + enc_num(1, 0, q)


def dec_frac(d, places):
    """Decimal value truncated to `places` fractional digits, as a Fraction."""
    scaled = int((d * (Decimal(10) ** places)).to_integral_value(rounding="ROUND_DOWN"))
    return F(scaled, 10**places)


# ---------------------------------------------------------------------------
# Value pools
# ---------------------------------------------------------------------------

PI_DIGITS = (
    "3.14159265358979323846264338327950288419716939937510582097494459230781640628620899862803482534211706798214808651328230"
    "664709384460955058223172535940812848111745028410270193852110555964462294895493038196"
)
PI = Decimal(PI_DIGITS)
SQRT2 = Decimal(2).sqrt()
SQRT3 = Decimal(3).sqrt()
E = Decimal(1).exp()
LN2 = Decimal(2).ln()
LN10 = Decimal(10).ln()
PHI = (1 + Decimal(5).sqrt()) / 2

INTS = [
    0, 1, -1, 2, -2, 3, 5, 7, 9, 10, 12, 16, 31, 32, 33, 63, 64, 100, -100, 127, 128, 255, 256,
    360, 400, 1000, 4095, 65535, 65536, 99999, 100000, 100001,
    2**31 - 1, 2**31, -(2**31), 2**32 - 1, 2**32, 2**53 + 1, 2**63 - 1, 2**63, -(2**63),
    2**64 - 1, 2**64, 10**18, 10**20, 3**40, -(10**25), 123456789, -987654321,
]

FRACS = [
    F(1, 2), F(-1, 2), F(1, 3), F(2, 3), F(-1, 3), F(1, 7), F(22, 7), F(355, 113), F(1, 10),
    F(3, 4), F(5, 8), F(-5, 8), F(1, 1000), F(123456789, 1000), F(1, 3**20), F(2**40, 3**25),
    F(7, 2**33), F(1, 2**31), F(3, 2**62), F(10**30, 7), F(1, 10**30), F(-1, 10**40),
]

DECIMALS = [
    "0.1", "0.2", "0.3", "0.5", "1.5", "2.5", "-2.5", "3.14159", "0.001", "1e-10", "1e-31",
    "1e-32", "1e-33", "1e-40", "1e-50", "1e-100", "1e30", "1e32", "1e33", "1e50", "1e100",
    "1e400", "-1e400", "1.23456789012345678901234567890123456789",
    "9.99999999999999999999999999999999", "0.999999999999999999999999999999999999",
    "1.0000000000000000000000000000001", "99999999999999999999999999999999", "123456.789",
    "-0.0001234", "0.00000000000000000000000000000000012345", "12345678901234567890.5",
]

IRR = []
for name, val in [
    ("sqrt2", SQRT2), ("sqrt3", SQRT3), ("pi", PI), ("e", E), ("ln2", LN2), ("ln10", LN10),
    ("pi/2", PI / 2), ("pi/3", PI / 3), ("pi/4", PI / 4), ("pi/6", PI / 6), ("2pi", 2 * PI),
    ("3pi/2", 3 * PI / 2), ("-pi", -PI), ("pi*1e10", PI * Decimal(10) ** 10), ("e^10", E**10),
    ("sqrt2/2", SQRT2 / 2), ("1/pi", 1 / PI), ("phi", PHI),
]:
    IRR.append(dec_frac(val, 150))
    IRR.append(dec_frac(val, 40))

# Raw, non-canonical representations (as CEngine/ratpak can produce them).
RAWS = [
    enc_num(1, 0, 1) + "/" + enc_num(-1, 0, 3),  # negative q (from divrat by a negative)
    enc_num(-1, 0, 5) + "/" + enc_num(-1, 0, 7),  # both negative
    enc_num(1, 2, 5) + "/" + enc_num(1, 1, 3),  # p and q with exponents
    enc_num(1, 1, 1) + "/" + enc_num(1, 0, 1),  # 2^31
    enc_num(1, 3, 12345) + "/" + enc_num(1, 0, 7),
    enc_num(1, 0, 7) + "/" + enc_num(1, 3, 1),  # 7 / 2^93
    enc_num(1, 40, 1) + "/" + enc_num(1, 0, 1),  # 2^1240 (~1e373)
    enc_num(1, 0, 1) + "/" + enc_num(1, 40, 1),
    enc_num(-1, 200, 999) + "/" + enc_num(1, 0, 3),  # ~ -1e1873
    enc_num(1, 0, 0) + "/" + enc_num(1, 0, 5),  # 0/5
    enc_num(-1, 0, 0) + "/" + enc_num(1, 0, 1),  # -0
    enc_num(1, 7, 0) + "/" + enc_num(1, 0, 1),  # zero with an exponent
]

# Huge magnitudes to provoke overflow / wrap-around paths.
HUGE = [
    enc_num(1, 1000, 1) + "/" + enc_num(1, 0, 1),
    enc_num(1, 0, 1) + "/" + enc_num(1, 1000, 1),
    enc_num(1, 5000, 3) + "/" + enc_num(1, 0, 1),
    enc_num(-1, 0, 7) + "/" + enc_num(1, 5000, 1),
]


def rand_rat(max_digits=4, exp_chance=0.15):
    def rnum(sign_ok):
        nd = rng.randint(1, max_digits)
        ds = [rng.randrange(BASEX) for _ in range(nd)]
        if ds[-1] == 0:
            ds[-1] = 1
        if nd == 1 and rng.random() < 0.3:
            ds = [rng.randint(1, 1000)]
        exp = rng.randint(1, 3) if rng.random() < exp_chance else 0
        sign = -1 if (sign_ok and rng.random() < 0.4) else 1
        return enc_raw(sign, exp, ds)

    return rnum(True) + "/" + rnum(False)


def rand_small():
    """A 'calculator sized' value: a decimal with up to 12 digits."""
    m = rng.randint(1, 10**rng.randint(1, 12))
    e = rng.randint(-8, 8)
    s = -1 if rng.random() < 0.35 else 1
    return frac(F(s * m) * F(10) ** e)


VALUES = (
    [frac(i) for i in INTS]
    + [frac(f) for f in FRACS]
    + [frac(d) for d in DECIMALS]
    + [frac(f) for f in IRR]
    + RAWS
    + HUGE
    + [rand_rat() for _ in range(25)]
    + [rand_small() for _ in range(25)]
)

MODERATE = (
    [frac(i) for i in INTS if abs(i) < 2**70]
    + [frac(f) for f in FRACS]
    + [frac(d) for d in DECIMALS if "e" not in d or d in ("1e-10", "1e30", "1e-31")]
    + [frac(f) for f in IRR[::3]]
    + RAWS[:6]
    + [rand_small() for _ in range(15)]
)

# ---------------------------------------------------------------------------
# Command groups
# ---------------------------------------------------------------------------

BINOPS = ["add", "sub", "mul", "div", "rem", "mod", "pow", "root", "shl", "shr", "and", "or", "xor"]
UNOPS = ["neg", "frac", "int", "fact", "exp", "log", "log10", "inv", "abs",
         "sinh", "cosh", "tanh", "asinh", "acosh", "atanh"]
TRIGOPS = ["sin", "cos", "tan"]
ITRIGOPS = ["asin", "acos", "atan"]
ANGLES = ["deg", "rad", "grad"]
FMTS = ["float", "sci", "eng"]


def safe_for(op, a):
    """Filters out inputs that are merely slow (not interesting)."""
    return True


TRIG_INPUTS = [frac(x) for x in [
    0, 30, 45, 60, 90, 120, 135, 150, 180, 210, 270, 300, 360, 390, 450, 720, -90, -180, -270,
    F(899999999999, 10**10), F(900000000001, 10**10), 100, 200, 300, 400, 10**10, 10**20,
    10**50, 10**99, -(10**30), F(1, 10**20), 1, -1, F(1, 2), 2, 3, 7, 1000, -1000, F(10**15 + 1, 10**5),
    dec_frac(PI, 150), dec_frac(PI / 2, 150), dec_frac(PI, 40), dec_frac(PI / 2, 40),
    dec_frac(3 * PI / 2, 150), dec_frac(2 * PI, 150), dec_frac(PI / 4, 150), dec_frac(-PI / 2, 150),
    dec_frac(PI * 100, 150), F(1, 3), F(-22, 7),
]]

ITRIG_INPUTS = [frac(x) for x in [
    0, 1, -1, F(1, 2), F(-1, 2), F(85, 100), F(8500001, 10**7), F(84999, 10**5), F(9, 10),
    F(99, 100), F(999999999999, 10**12), F(10000000001, 10**10), F(3, 2), 2, F(20000001, 10**7),
    -2, 10, 10**10, F(-1, 10**30), dec_frac(SQRT3 / 2, 150), dec_frac(SQRT2 / 2, 150),
    dec_frac(1 / SQRT3, 150), dec_frac(SQRT3, 150), F(1, 3), F(-7, 10), F(1, 10**50),
]]

# Magnitudes near the exp() limit (and sinh of large negatives, which runs
# the Taylor series for tens of thousands of terms) are slow and produce
# multi-kilobyte numerators; they are covered once, in BIG_UNARY.
HYP_INPUTS = [frac(x) for x in [
    0, F(1, 2), F(-1, 2), 1, -1, F(99, 100), F(101, 100), 10, -10, 100, -100, 1000, -30,
    F(-100000001, 10**4), 100001, -100001, F(1, 10**40), 2, -2,
    F(85, 100), F(-86, 100), F(999999, 10**6), F(-999999, 10**6), dec_frac(PI, 150), F(1, 3),
]]

EXPLOG_INPUTS = [frac(x) for x in [
    0, 1, -1, F(1, 2), 2, 10, 100, 1000, F(1000001, 10), -100001, 5000, -5000,
    F(1, 10**50), F(10**13 + 1, 10**13), 10**100, F(1, 10**100), 2**64, F(-5, 3),
    dec_frac(E, 150), dec_frac(LN10, 150), F(1, 3), 7, F(123456789, 1000),
]] + [HUGE[0], HUGE[1], RAWS[6]]

BIG_UNARY = [
    ("exp", frac(100000)), ("exp", frac(-100000)), ("exp", frac(99999)),
    ("sinh", frac(99999)), ("sinh", frac(100000)), ("cosh", frac(100000)), ("tanh", frac(100000)),
    ("sinh", frac(-10000)), ("sinh", frac(10000)), ("cosh", frac(-10000)), ("tanh", frac(-1000)),
    ("sinh", frac(-1000)), ("log", HUGE[2]), ("log", HUGE[3]),
]

# gamma() at precision 128 takes ~0.5s per non-integer argument (in C++
# too), so most groups only use the integer / near-integer ones.
FACT_INPUTS = [frac(x) for x in [
    0, 1, 2, 3, 5, 10, 20, 50, 100, 170, 171, 3249, 3250, -1, -2, -1000, -1001, F(1, 2), F(-1, 2),
    F(3, 2), F(5, 2), F(37, 10), F(-37, 10), F(1, 10**40), F(10**40 + 1, 10**39), F(-1, 3),
    F(999, 1000), dec_frac(PI, 40), F(-99999, 10**5),
]]
FACT_INPUTS_FAST = [frac(x) for x in [
    0, 1, 2, 3, 5, 10, 20, 50, 100, 170, 171, 3250, -1, -2, -1000, -1001, F(3, 2),
    F(10**200 + 1, 10**200),
]]

POW_PAIRS = [(frac(a), frac(b)) for a, b in [
    (2, 10), (2, F(1, 2)), (4, F(1, 2)), (8, F(1, 3)), (-8, F(1, 3)), (-8, F(2, 3)), (-8, F(1, 2)),
    (0, 0), (0, -1), (0, 2), (0, F(1, 2)), (2, -2), (10, 100), (10, 1000), (10, 9999), (10, 10000),
    (10, 100000), (2, 100000), (-2, 3), (-2, F(1, 2)), (-27, F(1, 3)), (2, F(1, 3)), (16, F(1, 4)),
    (10**10, F(1, 10)), (F(5, 2), F(5, 2)), (7, 1), (1, 10**10), (F(10000001, 10**7), 10**9),
    (F(1, 2), 1000), (F(1, 2), -1000), (-1, F(1, 3)), (-1, F(2, 6)), (-1, F(1, 4)), (3, F(-1, 2)),
    (dec_frac(E, 150), dec_frac(PI, 150)), (2, dec_frac(SQRT2, 150)), (100, F(3, 2)),
    (-32, F(3, 5)), (-32, F(4, 10)), (1000, F(1, 3)), (F(1, 1000), F(1, 3)), (9, F(1, 2)),
]]

ROOT_PAIRS = [(frac(a), frac(b)) for a, b in [
    (27, 3), (-27, 3), (16, 4), (-16, 4), (2, 2), (2, 0), (0, 2), (1000000, 6), (10, F(1, 2)),
    (-8, -3), (dec_frac(PI, 150), 2), (2**64, 64), (F(1, 4), 2), (5, 7),
]]

SHIFT_PAIRS = [(frac(a), frac(b)) for a, b in [
    (1, 0), (1, 1), (1, 31), (1, 32), (1, 63), (1, 64), (5, 100000), (5, 100001), (5, -3),
    (10**30, 3), (F(127, 10), 2), (F(-127, 10), 2), (256, 4), (255, 1), (1, -5), (7, -100000),
    (7, -100001), (0, 5), (2**64 - 1, 1), (3, F(5, 2)), (3, F(-5, 2)), (2**31, 99999),
    # NB: no huge right shifts such as 2**31-1: ratpowi32(2, n) only trims
    # when p and q are both large, so 2^(2^31) is computed in full (in C++
    # too) and never finishes.
]]

BIT_PAIRS = [(frac(a), frac(b)) for a, b in [
    (0xFF, 0x0F), (2**64 - 1, 2**32), (-5, 3), (5, -3), (F(57, 10), F(32, 10)), (2**100, 2**100 + 1),
    (0, 2**64 - 1), (2**64 - 1, 2**64 - 1), (12345678901234567, 98765432109876543), (F(1, 2), 1),
    (2**63, 2**63 - 1), (-(2**63), 2**64 - 1),
]]

REM_PAIRS = [(frac(a), frac(b)) for a, b in [
    (25, 4), (25, -4), (-25, 4), (-25, -4), (426, 56478), (56478, 426), (-643, 8756), (643, -8756),
    (1000, 250), (1000, -250), (0, 5), (5, 0), (0, 0), (F(250, 100), 89), (F(1000, 3), -10),
    (834345, F(103, 100)), (834345, F(-103, 100)), (dec_frac(PI, 150), dec_frac(E, 150)),
    (10**30, 7), (-(10**30), 7), (F(1, 3), F(1, 7)), (F(-1, 3), F(1, 7)), (2**64, 3), (7, F(1, 10**40)),
]]

DIV_PAIRS = [(frac(a), frac(b)) for a, b in [
    (1, 0), (0, 0), (0, 5), (0, -5), (1, 3), (-1, 3), (2, F(-1, 3)), (10**40, F(1, 10**40)),
    (1, 7), (22, 7), (F(1, 3), F(1, 3)),
]]


def gen_arith_core(tag):
    emit(f"# ---- arithmetic ({tag})")
    for a, b in POW_PAIRS:
        emit(f"BIN pow {a} {b}")
    for a, b in ROOT_PAIRS:
        emit(f"BIN root {a} {b}")
    for a, b in SHIFT_PAIRS:
        emit(f"BIN shl {a} {b}")
        emit(f"BIN shr {a} {b}")
    for a, b in BIT_PAIRS:
        for op in ("and", "or", "xor"):
            emit(f"BIN {op} {a} {b}")
    for a, b in REM_PAIRS:
        emit(f"BIN rem {a} {b}")
        emit(f"BIN mod {a} {b}")
    for a, b in DIV_PAIRS:
        emit(f"BIN div {a} {b}")


def gen_arith_random(n):
    emit("# ---- random binary operations")
    for _ in range(n):
        a = rng.choice(VALUES) if rng.random() < 0.6 else rand_rat()
        b = rng.choice(VALUES) if rng.random() < 0.6 else rand_rat()
        op = rng.choice(BINOPS)
        if op in ("pow", "root", "and", "or", "xor"):
            # Keep exponents / bit operands calculator-sized.
            a = rng.choice(MODERATE)
            b = rng.choice(MODERATE)
        elif op in ("shl", "shr"):
            a = rng.choice(MODERATE)
            b = frac(rng.choice([rng.randint(-70, 70), F(rng.randint(-700, 700), 10)]))
        emit(f"BIN {op} {a} {b}")
    for a in VALUES:
        for op in ("add", "sub", "mul", "div"):
            b = rng.choice(VALUES)
            if op in ("add", "sub") and (a in HUGE[2:] or b in HUGE[2:]):
                # exact sums with 2^(31*5000) are 5000-digit numbers; one of
                # each is enough (see the random section above).
                b = HUGE[0] if a in HUGE[2:] else b
                a = HUGE[0] if a in HUGE[2:] else a
                b = HUGE[1] if b in HUGE[2:] else b
            emit(f"BIN {op} {a} {b}")


def gen_unary(inputs_by_op):
    emit("# ---- unary functions")
    for op, inputs in inputs_by_op:
        for a in inputs:
            emit(f"UN {op} {a}")


def gen_trig(trig_inputs, itrig_inputs):
    emit("# ---- trig")
    for a in trig_inputs:
        for op in TRIGOPS:
            for ang in ANGLES:
                emit(f"TRIG {op} {ang} {a}")
    for a in itrig_inputs:
        for op in ITRIGOPS:
            for ang in ANGLES:
                emit(f"TRIG {op} {ang} {a}")


def gen_cmp():
    emit("# ---- comparisons")
    pairs = [
        (frac(1), frac(1)), (frac(F(1, 2)), enc_num(1, 0, 2) + "/" + enc_num(1, 0, 4)), (frac(0), RAWS[10]),
        (frac(1), frac(F(10**130 + 1, 10**130))), (frac(1), frac(F(10**120 + 1, 10**120))),
        (frac(-1), frac(1)), (frac(2), frac(-3)), (RAWS[0], frac(F(-1, 3))), (frac(0), RAWS[11]),
        (HUGE[0], HUGE[2]), (HUGE[1], frac(0)), (frac(10**40), frac(10**40 + 1)),
    ]
    for _ in range(40):
        pairs.append((rng.choice(VALUES), rng.choice(VALUES)))
    for a, b in pairs:
        emit(f"CMP {a} {b}")
        emit(f"CMP {b} {a}")


def gen_str(values, radixes, precisions, fmts=FMTS):
    emit(f"# ---- to_string radixes={radixes} precisions={precisions}")
    for a in values:
        for radix in radixes:
            for prec in precisions:
                for fmt in fmts:
                    emit(f"STR {radix} {fmt} {prec} {a}")


def gen_u64():
    emit("# ---- to_u64")
    for x in [0, 1, 2**32 - 1, 2**32, 2**32 + 1, 2**63, 2**64 - 1, 2**64, 2**64 + 5, -1, F(1, 2), F(129, 10),
              F(-1, 2), 10**30, 123456789012345678, 2**31 - 1, 2**31, F(2**64 - 1, 1) + F(9, 10)]:
        emit(f"U64 {frac(x)}")
    for a in RAWS + [HUGE[1]]:
        emit(f"U64 {a}")
    for _ in range(20):
        emit(f"U64 {frac(rng.randrange(2**rng.randint(1, 70)))}")


def gen_from():
    emit("# ---- constructors")
    for v in [0, 1, -1, 2**31 - 1, -(2**31), 123456, -987654]:
        emit(f"FROMI32 {v}")
    for v in [0, 1, 2**31, 2**32 - 1, 3000000000]:
        emit(f"FROMU32 {v}")
    for v in [0, 1, 2**32 - 1, 2**32, 2**63, 2**64 - 1, 1234567890123456789]:
        emit(f"FROMU64 {v}")
    for n in [enc_num(1, 3, 5), enc_num(-1, -2, 7), enc_num(1, 0, 0), enc_num(-1, -1, 2**40), enc_num(1, 0, 10**20)]:
        emit(f"FROMNUM {n}")


def gen_const():
    emit("# ---- constants")
    for c in ["qword", "dword", "word", "byte", "exp", "ln_ten", "pi"]:
        emit(f"CONST {c}")


def q(s):
    assert "'" not in s and " " not in s and "\t" not in s
    return "'" + s + "'"


def gen_s2r(radix, precision, sep="."):
    emit(f"# ---- string_to_rat radix={radix} precision={precision}")
    if radix == 10:
        mants = ["", "0", "000", "0.0", ".5", "5.", "1.5", "-", "+", "123.456", "0.000001", "007",
                 "1234567890123456789012345678901234567890", "12.34.5", "1e5", "1e-5", "1e", "0e5",
                 "0.000e3", "+12", "-12", "9" * 40, "0." + "0" * 40 + "1", "3.14159265358979323846264338327950288",
                 "1A", "e", "1.0", "100", "0.1", "999999999999999999999999999999999"]
    elif radix == 16:
        mants = ["", "0", "ABC", "abc", "FFFFFFFF", "ffffffffffffffff", "1.8", "DEADBEEF", "1^5", "G", "e",
                 "E", "1e", "0.0", "10000000000000000"]
    elif radix == 8:
        mants = ["", "0", "777", "17777777777", "1777777777777777777777", "8", "1.4", "0.0", "12^3"]
    elif radix == 2:
        mants = ["", "0", "101", "102", "1" * 64, "1" + "0" * 64, "1.1", "0.0", "11^10"]
    else:
        mants = ["0", "1", "10", "zz", "ZZ"]
    if sep != ".":
        mants = [m.replace(".", sep) for m in mants] + ["1.5", "1" + sep + "5"]
    # Exponents are evaluated as radix^|exp| with no trimming (in C++ too), so
    # keep them calculator-sized: CalcInput allows at most 4 exponent digits,
    # and only in decimal mode.
    exps = ["", "5", "00", "0", "12", "-3", "1A"] + (["9999"] if radix == 10 else [])
    for m in mants:
        for mneg in (0, 1):
            emit(f"S2R {mneg} {q(m)} 0 {q('')} {radix} {precision}")
    for m in mants[:6]:
        for e in exps:
            for eneg in (0, 1):
                emit(f"S2R 0 {q(m)} {eneg} {q(e)} {radix} {precision}")


def gen_chains():
    emit("# ---- realistic chains via registers")
    emit("@two S2R 0 '2' 0 '' 10 32")
    emit("@r BIN root $two " + frac(2))
    emit("@s BIN mul $r $r")
    emit("BIN sub $s $two")
    emit("BIN sub $two $s")
    emit("STR 10 float 32 $s")
    emit("@p1 S2R 0 '0.1' 0 '' 10 32")
    emit("@p2 S2R 0 '0.2' 0 '' 10 32")
    emit("@p3 S2R 0 '0.3' 0 '' 10 32")
    emit("@sum BIN add $p1 $p2")
    emit("CMP $sum $p3")
    emit("BIN sub $sum $p3")
    emit("@third BIN div " + frac(1) + " " + frac(3))
    emit("@one BIN mul $third " + frac(3))
    emit("CMP $one " + frac(1))
    emit("STR 10 float 32 $one")
    emit("@pi CONST pi")
    for op in TRIGOPS:
        emit(f"TRIG {op} rad $pi")
    emit("@l UN log " + frac(10))
    emit("@el UN exp $l")
    emit("STR 10 float 32 $el")
    emit("@sn TRIG sin deg " + frac(30))
    emit("@as TRIG asin deg $sn")
    emit("STR 10 float 32 $as")
    emit("@sq BIN pow " + frac(F(9, 4)) + " " + frac(F(1, 2)))
    emit("BIN sub $sq " + frac(F(3, 2)))
    emit("@f UN fact " + frac(F(1, 2)))
    emit("BIN mul $f $f")
    emit("@big BIN pow " + frac(10) + " " + frac(400))
    emit("BIN add $big " + frac(1))
    emit("STR 10 float 32 $big")
    emit("STR 10 sci 32 $big")
    emit("@acc S2R 0 '1' 0 '' 10 32")
    for i in range(30):
        emit(f"@acc BIN mul $acc {frac(F(i + 2, i + 1))}")
    emit("STR 10 float 32 $acc")
    emit("@x S2R 0 '7' 0 '' 10 32")
    for i in range(10):
        emit("@x UN exp $x" if i % 2 == 0 else "@x UN log $x")
    emit("STR 10 float 32 $x")


def group_full(tag, fact_inputs):
    gen_const()
    gen_arith_core(tag)
    gen_unary([(op, EXPLOG_INPUTS) for op in ("exp", "log", "log10")]
              + [("fact", fact_inputs)]
              + [(op, HYP_INPUTS) for op in ("sinh", "cosh", "tanh", "asinh", "acosh", "atanh")]
              + [(op, VALUES) for op in ("neg", "frac", "int", "inv", "abs")])
    gen_trig(TRIG_INPUTS, ITRIG_INPUTS)


def group_light(tag):
    """A smaller set to run after other ChangeConstants calls."""
    gen_const()
    emit(f"# ---- light group ({tag})")
    for a in TRIG_INPUTS[::3]:
        for op in TRIGOPS:
            for ang in ANGLES:
                emit(f"TRIG {op} {ang} {a}")
    for a in ITRIG_INPUTS[::3]:
        for op in ITRIGOPS:
            emit(f"TRIG {op} {rng.choice(ANGLES)} {a}")
    for a in EXPLOG_INPUTS[::2]:
        emit(f"UN exp {a}")
        emit(f"UN log {a}")
    for a in HYP_INPUTS[::3]:
        emit(f"UN sinh {a}")
        emit(f"UN atanh {a}")
    for a, b in POW_PAIRS[::3]:
        emit(f"BIN pow {a} {b}")
    for a in FACT_INPUTS_FAST[::4]:
        emit(f"UN fact {a}")
    for a, b in SHIFT_PAIRS[::2]:
        emit(f"BIN shr {a} {b}")
    for a, b in REM_PAIRS[::2]:
        emit(f"BIN mod {a} {b}")
    for _ in range(40):
        emit(f"BIN {rng.choice(['add', 'sub', 'mul', 'div'])} {rng.choice(VALUES)} {rng.choice(VALUES)}")


STR_VALUES = (
    [frac(i) for i in INTS]
    + [frac(f) for f in FRACS]
    + [frac(d) for d in DECIMALS]
    + [frac(f) for f in IRR[::2]]
    + RAWS
    + HUGE
    + [rand_small() for _ in range(20)]
    + [rand_rat() for _ in range(10)]
)
INT_STR_VALUES = [frac(i) for i in INTS] + [frac(rng.randrange(2**64)) for _ in range(10)]


def main():
    # Initial state: ChangeConstants(10, 32) (done by the driver / Rust context init).
    group_full("10/32 initial", FACT_INPUTS)
    emit("# ---- big magnitudes (once)")
    for op, a in BIG_UNARY:
        emit(f"UN {op} {a}")
    gen_cmp()
    gen_u64()
    gen_from()
    gen_chains()
    gen_arith_random(150)
    gen_str(STR_VALUES, [10], [32, 8, 16, 64, 128])
    gen_str(STR_VALUES[::2], [16, 8, 2], [32, 64])
    gen_str(STR_VALUES[::7], [3, 36], [20])
    gen_s2r(10, 32)

    # Programmer mode: radix with maxIntDigits + 1 precision (QWORD).
    for radix, prec in [(16, 17), (8, 22)]:
        emit(f"CC {radix} {prec}")
        gen_const()
        gen_str(INT_STR_VALUES, [radix], [prec, 32, 64], ["float"])
        gen_s2r(radix, prec)
        gen_u64()
        for a, b in BIT_PAIRS + SHIFT_PAIRS[::2]:
            emit(f"BIN and {a} {b}")
            emit(f"BIN shl {a} {b}")

    # Binary QWORD: g_ratio * radix * precision = 30 * 2 * 65 > 2880, so this
    # takes the "compute the constants" path of ChangeConstants.
    emit("CC 2 65")
    group_light("2/65 computed constants")
    gen_str(INT_STR_VALUES, [2], [65, 64, 32], ["float"])
    gen_str(STR_VALUES[::5], [10], [32])
    gen_s2r(2, 65)
    gen_u64()

    # Back to decimal: cbitsofprecision is now 3900, so this re-reads ratconst.
    emit("CC 10 32")
    group_light("10/32 after 2/65")
    gen_str(STR_VALUES[::3], [10], [32], FMTS)

    # RationalTest's ChangeConstants(10, 128): compute path at precision 137.
    emit("CC 10 128")
    group_full("10/128 computed constants", FACT_INPUTS_FAST)
    gen_str(STR_VALUES[::4], [10], [32, 128])

    # Lower precision after a higher one: read path (table constants).
    emit("CC 10 64")
    group_light("10/64 read constants")
    emit("CC 10 32")
    group_light("10/32 again")

    # Other word widths in programmer mode.
    for radix, prec in [(16, 9), (8, 11), (2, 33), (16, 5), (8, 6), (2, 17), (2, 9), (16, 3), (8, 4)]:
        emit(f"CC {radix} {prec}")
        gen_const()
        gen_str(INT_STR_VALUES[::3], [radix], [prec], ["float"])
        for a, b in BIT_PAIRS[::2]:
            emit(f"BIN xor {a} {b}")

    # GetCurrentResultForRadix: ChangeConstants(m_radix, precision) then ToString.
    emit("CC 10 32")
    for radix in (16, 8, 2):
        emit("CC 10 64")
        gen_str(INT_STR_VALUES[::4], [radix], [64], ["float"])
        emit("CC 10 32")

    # Decimal separator.
    emit("SEP ','")
    gen_str(STR_VALUES[::6], [10], [32])
    gen_s2r(10, 32, ",")
    emit("SEP '.'")
    gen_s2r(10, 32)

    # Chains under 10/32 once more (state after all of the above).
    gen_chains()

    sys.stdout.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
