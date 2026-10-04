# Generates the expected values in review11_primitives.rs (see its header).
import mpmath as mp
from fractions import Fraction
import math
import sys

mp.mp.dps = 80


def dbl(v):
    """mpf -> nearest double (correctly rounded), ±inf on overflow, NaN for None."""
    if v is None:
        return float('nan')
    try:
        f = float(v)
    except OverflowError:
        f = math.inf if v > 0 else -math.inf
    return f


def rs(f):
    if math.isnan(f):
        return "f64::NAN"
    if math.isinf(f):
        return "f64::INFINITY" if f > 0 else "f64::NEG_INFINITY"
    r = repr(f)
    if 'e' not in r and '.' not in r:
        r += '.0'
    return r


units = {'R': mp.mpf(1), 'D': mp.pi / 180, 'G': mp.pi / 200}


def F(name, u, x):
    X = mp.mpf(x)
    k = units[u] if u else None
    if name in ('sin', 'cos', 'tan', 'sec', 'csc', 'cot'):
        if u == 'R':
            # Enough working digits to reduce even 1e300 by 2π exactly.
            with mp.workdps(max(80, int(abs(float(x)) and math.log10(abs(float(x)) + 1)) + 100)):
                v = {'sin': mp.sin, 'cos': mp.cos, 'tan': mp.tan, 'sec': mp.sec,
                     'csc': mp.csc, 'cot': mp.cot}[name](X) if not (
                         name in ('csc', 'cot') and X == 0) else None
                return None if v is None else +v
        if u != 'R' and x < 0:
            # Odd (sin, tan, csc, cot) or even (cos, sec): |x| reduced exactly
            # beats x mod a turn, which is a turn less a tiny amount.
            v = F(name, u, -x)
            if v is None:
                return None
            return v if name in ('cos', 'sec') else -v
        if u != 'R':
            turn = 360 if u == 'D' else 400
            # Exactly x mod a turn (a dyadic rational), then exact quarters.
            r = Fraction(x) % turn
            X = mp.mpf(r.numerator) / r.denominator
            q = r / (Fraction(turn) / 4)
            if q.denominator == 1:
                qi = int(q) % 4
                s, c = [(0, 1), (1, 0), (0, -1), (-1, 0)][qi]
                vals = {
                    'sin': s, 'cos': c,
                    'tan': None if c == 0 else mp.mpf(s) / c,
                    'sec': None if c == 0 else mp.mpf(1) / c,
                    'csc': None if s == 0 else mp.mpf(1) / s,
                    'cot': None if s == 0 else mp.mpf(c) / s,
                }
                v = vals[name]
                return None if v is None else mp.mpf(v)
        if name in ('csc', 'cot') and X == 0:
            return None
        a = X * k
        return {'sin': mp.sin, 'cos': mp.cos, 'tan': mp.tan, 'sec': mp.sec,
                'csc': mp.csc, 'cot': mp.cot}[name](a)
    if name in ('asin', 'acos', 'atan', 'asec', 'acsc', 'acot'):
        if name == 'asin':
            if abs(X) > 1:
                return None
            r = mp.asin(X)
        elif name == 'acos':
            if abs(X) > 1:
                return None
            r = mp.acos(X)
        elif name == 'atan':
            r = mp.atan(X)
        elif name == 'asec':
            if abs(X) < 1:
                return None
            r = mp.acos(1 / X)
        elif name == 'acsc':
            if abs(X) < 1:
                return None
            r = mp.asin(1 / X)
        else:
            if X == 0:
                r = mp.pi / 2
            elif X > 0:
                r = mp.atan(1 / X)
            else:
                r = mp.pi + mp.atan(1 / X)
        return r / k
    if name == 'sinh':
        return mp.sinh(X)
    if name == 'cosh':
        return mp.cosh(X)
    if name == 'tanh':
        return mp.tanh(X)
    if name == 'sech':
        return mp.sech(X)
    if name == 'csch':
        return None if X == 0 else mp.csch(X)
    if name == 'coth':
        return None if X == 0 else mp.coth(X)
    if name == 'asinh':
        return mp.asinh(X)
    if name == 'acosh':
        return None if X < 1 else mp.acosh(X)
    if name == 'atanh':
        return None if abs(X) >= 1 else mp.atanh(X)
    if name == 'asech':
        return None if not (0 < X <= 1) else mp.acosh(1 / X)
    if name == 'acsch':
        return None if X == 0 else mp.asinh(1 / X)
    if name == 'acoth':
        return None if abs(X) <= 1 else mp.atanh(1 / X)
    if name == 'ln':
        return None if X <= 0 else mp.log(X)
    if name == 'log10':
        return None if X <= 0 else mp.log10(X)
    if name == 'gamma':
        if X <= 0 and X == mp.floor(X):
            return None
        return mp.gamma(X)
    if name.startswith('pow_'):
        _, p, q = name.split('_')
        p, q = int(p.replace('m', '-')), int(q)
        if X == 0:
            return None if p < 0 else mp.mpf(0)
        if X < 0:
            if q % 2 == 0:
                return None
            m = mp.power(-X, mp.mpf(p) / q)
            return m if p % 2 == 0 else -m
        return mp.power(X, mp.mpf(p) / q)
    raise Exception(name)


tiny = [5e-324, 2.0 ** -1074 * 3, 1e-310, 2.0 ** -1022, 1e-300, 1e-200, 1e-20, 1e-8, 1e-3]
near1 = [1.0, 1 + 2 ** -52, 1 - 2 ** -53, 1 + 1e-10, 1 - 1e-10, 1 + 1e-15, 1 - 1e-15, 1.5, 2.0, 0.5]
big = [3.0, 10.0, 100.0, 700.0, 709.0, 711.0, 740.0, 745.0, 1e3, 1e8, 1e15, 1e17, 1e100, 1e300]
trigx = [1e-300, 5e-324, 1e-20, 1e-8, 0.5, 1.0, math.pi / 2, math.pi, 89.99999999999999, 90.0,
         90.00000000000001, 180.0, 179.99999999999997, 100.0, 200.0, 360.0, 400.0, 1e15, 1e17,
         1e300, 12345.678]

rows = []


def add(name, u, xs, neg=True):
    for x in xs:
        for s in ([1, -1] if neg else [1]):
            xv = s * x
            rows.append((name, u or '-', xv, dbl(F(name, u, xv))))


for u in 'RDG':
    for n in ('sin', 'cos', 'tan', 'sec', 'csc', 'cot'):
        add(n, u, trigx)
    for n in ('asin', 'acos', 'atan'):
        add(n, u, tiny + [0.5, 1 - 1e-15, 1 - 2 ** -53, 1.0, 0.9999999999, 2.0, 1e15, 1e300])
    for n in ('asec', 'acsc', 'acot'):
        add(n, u, near1 + big + tiny[:3])
for n in ('sinh', 'cosh', 'tanh', 'sech', 'csch', 'coth', 'asinh', 'acosh', 'asech', 'acsch',
          'acoth', 'atanh'):
    add(n, None, tiny + near1 + big)
for n in ('ln', 'log10'):
    add(n, None, tiny + near1 + big, neg=False)
for (p, q) in [(1, 3), (2, 3), (-1, 3), (4, 3), (1, 5), (3, 7), (5, 2), (-3, 2), (1000, 3), (7, 1000)]:
    add(f'pow_{p}_{q}'.replace('-', 'm'), None, [x for x in tiny + near1 + big if x < 1e200 or p * 1.0 / q < 1.5])
add('gamma', None, [0.5, 1.5, 2.5, 10.0, 170.5, -0.5, -1.5, -2.9999999999, -3.0000000001,
                    -10.000000001, -0.000000001, 1e-8, 171.5], neg=False)

uname = {'R': 'Radians', 'D': 'Degrees', 'G': 'Grads', '-': 'Radians'}
out = [f'    ("{n}", {uname[u]}, {rs(x)}, {rs(v)}),' for (n, u, x, v) in rows]
open(sys.argv[1], 'w').write("\n".join(out) + "\n")
print(len(out))
