//! Integer steps read a typed decimal before rounding it to a double.

use graphing::{Graph, Viewport};

#[test]
fn literal_integer_steps_use_the_exact_side_of_the_boundary() {
    let cases = [
        ("ceil(1.00000000000000000001)", 2.0),
        ("floor(0.99999999999999999999)", 0.0),
        ("floor(-1.00000000000000000001)", -2.0),
        ("ceil(-0.99999999999999999999)", 0.0),
        ("round(0.49999999999999999999)", 0.0),
        ("round(-0.49999999999999999999)", 0.0),
        ("round(0.5)", 1.0),
        ("round(-0.5)", -1.0),
        ("ceil(1/3)", 1.0),
        ("floor(-1/3)", -1.0),
        ("sign(10^(-400))", 1.0),
        ("sign(-(10^(-400)))", -1.0),
        ("sign(0)", 0.0),
        ("floor(9007199254740993.1)-9007199254740993", 0.0),
        ("ceil(9007199254740992.1)-9007199254740993", 0.0),
        ("(-1)^floor(9007199254740993.1)", -1.0),
        ("(-1)^ceil(9007199254740992.1)", -1.0),
    ];
    let mut graph = Graph::new();
    graph.set_literal_digits(None);
    for (source, expected) in cases {
        let id = graph.add_equation(source);
        for x in [-3.0, 0.0, 3.0] {
            assert_eq!(graph.evaluate(id, x), Some(expected), "{source} at {x}");
        }
        graph.remove_equation(id);
    }

    // The digit setting acts before the step. At fourteen digits the
    // near-integer literal is exactly 1, whose ceiling is 1.
    graph.set_literal_digits(Some(14));
    let id = graph.add_equation("ceil(1.00000000000000000001)");
    assert_eq!(graph.evaluate(id, 0.0), Some(1.0));
}

#[test]
fn literal_steps_compose_and_plot_at_an_ordinary_scale() {
    let mut graph = Graph::new();
    graph.set_literal_digits(None);
    let vp = Viewport::default_for_size(760.0, 500.0);
    for source in [
        "atan(x)-atan(x)+5+ceil(1.00000000000000000001)",
        "5+ceil(1.00000000000000000001)",
        "7+floor(0.99999999999999999999)",
    ] {
        let id = graph.add_equation(source);
        assert_eq!(graph.evaluate(id, 0.0), Some(7.0), "{source}");
        let plot = graph.plot_equation(id, &vp).unwrap();
        assert!(!plot.has_missing_data, "{source}");
        assert!(plot.point_count() >= 2, "{source}");
        assert!(
            plot.curves
                .iter()
                .flatten()
                .all(|p| (p.y - 7.0).abs() < 1e-12),
            "{source}"
        );
        graph.remove_equation(id);
    }

    let id = graph.add_equation("x+ceil(1.00000000000000000001)");
    let plot = graph.plot_equation(id, &vp).unwrap();
    assert!(!plot.has_missing_data);
    assert!(plot.point_count() >= 2);
    assert!(
        plot.curves
            .iter()
            .flatten()
            .all(|p| (p.y - p.x - 2.0).abs() < 1e-12)
    );
}

#[test]
fn exact_integer_steps_keep_their_slider_dependency() {
    let mut graph = Graph::new();
    graph.set_literal_digits(None);
    let id = graph.add_equation("ceil(a+0.00000000000000000001)");
    for (a, expected) in [(1.0, 2.0), (2.0, 3.0), (-1.0, 0.0)] {
        graph.set_variable("a", a);
        assert_eq!(graph.evaluate(id, 0.0), Some(expected));
    }
}
