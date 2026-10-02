// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `src/CalculatorUnitTests/RationalTest.cpp`.
//!
//! `TEST_CLASS_INITIALIZE(CommonSetup)` calls `ChangeConstants(10, 128)`; the
//! ratpack context is per-thread and every `#[test]` runs on its own thread,
//! so each test calls [`setup`] first.

use ratpack::rational_math::modulo;
use ratpack::{CALC_E_INDEFINITE, Number, NumberFormat, Rational, change_constants};

fn setup() {
    change_constants(10, 128);
}

fn r(i: i32) -> Rational {
    Rational::from(i)
}

fn pq(psign: i32, p: u32, q: u32) -> Rational {
    Rational::from_pq(Number::new(psign, 0, vec![p]), Number::new(1, 0, vec![q]))
}

/// `VERIFY_ARE_EQUAL(res, n)`: `operator==` with the implicit `Rational(int)`.
#[track_caller]
fn assert_rat_eq(res: &Rational, n: i32) {
    assert!(
        *res == r(n),
        "expected {n}, got {:?}",
        res.to_string_radix(10, NumberFormat::Float, 32)
    );
}

#[track_caller]
fn assert_str(res: &Rational, expected: &str) {
    assert_eq!(
        res.to_string_radix(10, NumberFormat::Float, 8).unwrap(),
        expected
    );
}

#[test]
fn test_modulo_operands_not_modified() {
    setup();
    // Verify results but also check that operands are not modified
    let rat25 = r(25);
    let ratminus25 = r(-25);
    let rat4 = r(4);
    let ratminus4 = r(-4);
    let mut res = modulo(&rat25, &rat4).unwrap();
    assert_rat_eq(&res, 1);
    assert_rat_eq(&rat25, 25);
    assert_rat_eq(&rat4, 4);
    res = modulo(&rat25, &ratminus4).unwrap();
    assert_rat_eq(&res, -3);
    assert_rat_eq(&rat25, 25);
    assert_rat_eq(&ratminus4, -4);
    res = modulo(&ratminus25, &ratminus4).unwrap();
    assert_rat_eq(&res, -1);
    assert_rat_eq(&ratminus25, -25);
    assert_rat_eq(&ratminus4, -4);
    res = modulo(&ratminus25, &rat4).unwrap();
    assert_rat_eq(&res, 3);
    assert_rat_eq(&ratminus25, -25);
    assert_rat_eq(&rat4, 4);
}

#[test]
fn test_modulo_integer() {
    setup();
    // Check with integers
    let mut res = modulo(&r(426), &r(56478)).unwrap();
    assert_rat_eq(&res, 426);
    res = modulo(&r(56478), &r(426)).unwrap();
    assert_rat_eq(&res, 246);
    res = modulo(&r(-643), &r(8756)).unwrap();
    assert_rat_eq(&res, 8113);
    res = modulo(&r(643), &r(-8756)).unwrap();
    assert_rat_eq(&res, -8113);
    res = modulo(&r(-643), &r(-8756)).unwrap();
    assert_rat_eq(&res, -643);
    res = modulo(&r(1000), &r(250)).unwrap();
    assert_rat_eq(&res, 0);
    res = modulo(&r(1000), &r(-250)).unwrap();
    assert_rat_eq(&res, 0);
}

#[test]
fn test_modulo_zero() {
    setup();
    // Test with Zero
    let mut res = modulo(&r(343654332), &r(0)).unwrap();
    assert_rat_eq(&res, 343654332);
    res = modulo(&r(0), &r(8756)).unwrap();
    assert_rat_eq(&res, 0);
    res = modulo(&r(0), &r(-242)).unwrap();
    assert_rat_eq(&res, 0);
    res = modulo(&r(0), &r(0)).unwrap();
    assert_rat_eq(&res, 0);
    res = modulo(&pq(1, 23242, 2), &pq(1, 0, 23)).unwrap();
    assert_rat_eq(&res, 11621);
}

#[test]
fn test_modulo_rational() {
    setup();
    // Test with rational numbers
    let mut res = modulo(&pq(1, 250, 100), &r(89)).unwrap();
    assert_str(&res, "2.5");
    res = modulo(&pq(1, 3330, 1332), &r(1)).unwrap();
    assert_str(&res, "0.5");
    res = modulo(&pq(1, 12250, 100), &r(10)).unwrap();
    assert_str(&res, "2.5");
    res = modulo(&pq(-1, 12250, 100), &r(10)).unwrap();
    assert_str(&res, "7.5");
    res = modulo(&pq(-1, 12250, 100), &r(-10)).unwrap();
    assert_str(&res, "-2.5");
    res = modulo(&pq(1, 12250, 100), &r(-10)).unwrap();
    assert_str(&res, "-7.5");
    res = modulo(&pq(1, 1000, 3), &r(1)).unwrap();
    assert_str(&res, "0.33333333");
    res = modulo(&pq(1, 1000, 3), &r(-10)).unwrap();
    assert_str(&res, "-6.6666667");
    res = modulo(&r(834345), &pq(1, 103, 100)).unwrap();
    assert_str(&res, "0.71");
    res = modulo(&r(834345), &pq(-1, 103, 100)).unwrap();
    assert_str(&res, "-0.32");
}

#[test]
fn test_remainder_operands_not_modified() {
    setup();
    // Verify results but also check that operands are not modified
    let rat25 = r(25);
    let ratminus25 = r(-25);
    let rat4 = r(4);
    let ratminus4 = r(-4);
    let mut res = rat25.rem(&rat4).unwrap();
    assert_rat_eq(&res, 1);
    assert_rat_eq(&rat25, 25);
    assert_rat_eq(&rat4, 4);
    res = rat25.rem(&ratminus4).unwrap();
    assert_rat_eq(&res, 1);
    assert_rat_eq(&rat25, 25);
    assert_rat_eq(&ratminus4, -4);
    res = ratminus25.rem(&ratminus4).unwrap();
    assert_rat_eq(&res, -1);
    assert_rat_eq(&ratminus25, -25);
    assert_rat_eq(&ratminus4, -4);
    res = ratminus25.rem(&rat4).unwrap();
    assert_rat_eq(&res, -1);
    assert_rat_eq(&ratminus25, -25);
    assert_rat_eq(&rat4, 4);
}

#[test]
fn test_remainder_integer() {
    setup();
    // Check with integers
    let mut res = r(426).rem(&r(56478)).unwrap();
    assert_rat_eq(&res, 426);
    res = r(56478).rem(&r(426)).unwrap();
    assert_rat_eq(&res, 246);
    res = r(-643).rem(&r(8756)).unwrap();
    assert_rat_eq(&res, -643);
    res = r(643).rem(&r(-8756)).unwrap();
    assert_rat_eq(&res, 643);
    res = r(-643).rem(&r(-8756)).unwrap();
    assert_rat_eq(&res, -643);
    res = r(-124).rem(&r(-124)).unwrap();
    assert_rat_eq(&res, 0);
    res = r(24).rem(&r(24)).unwrap();
    assert_rat_eq(&res, 0);
}

#[test]
fn test_remainder_zero() {
    setup();
    // Test with Zero
    let mut res = r(0).rem(&r(3654)).unwrap();
    assert_rat_eq(&res, 0);
    res = r(0).rem(&r(-242)).unwrap();
    assert_rat_eq(&res, 0);
    for number in [343654332, 0, -23423] {
        assert_eq!(r(number).rem(&r(0)).unwrap_err(), CALC_E_INDEFINITE);

        let lhs = Rational::from_pq(Number::new(1, number, vec![0]), Number::new(1, 0, vec![2]));
        let rhs = pq(1, 0, 23);
        assert_eq!(lhs.rem(&rhs).unwrap_err(), CALC_E_INDEFINITE);
    }
}

#[test]
fn test_remainder_rational() {
    setup();
    // Test with rational numbers
    let mut res = pq(1, 250, 100).rem(&r(89)).unwrap();
    assert_str(&res, "2.5");
    res = pq(1, 3330, 1332).rem(&r(1)).unwrap();
    assert_str(&res, "0.5");
    res = pq(1, 12250, 100).rem(&r(10)).unwrap();
    assert_str(&res, "2.5");
    res = pq(-1, 12250, 100).rem(&r(10)).unwrap();
    assert_str(&res, "-2.5");
    res = pq(-1, 12250, 100).rem(&r(-10)).unwrap();
    assert_str(&res, "-2.5");
    res = pq(1, 12250, 100).rem(&r(-10)).unwrap();
    assert_str(&res, "2.5");
    res = pq(1, 1000, 3).rem(&r(1)).unwrap();
    assert_str(&res, "0.33333333");
    res = pq(1, 1000, 3).rem(&r(-10)).unwrap();
    assert_str(&res, "3.3333333");
    res = pq(-1, 1000, 3).rem(&r(-10)).unwrap();
    assert_str(&res, "-3.3333333");
    res = r(834345).rem(&pq(1, 103, 100)).unwrap();
    assert_str(&res, "0.71");
    res = r(834345).rem(&pq(-1, 103, 100)).unwrap();
    assert_str(&res, "0.71");
    res = r(-834345).rem(&pq(1, 103, 100)).unwrap();
    assert_str(&res, "-0.71");
}
