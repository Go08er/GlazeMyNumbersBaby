//! Tables a theme makes up and hands to a draw call: what they hold is
//! read only up to what the call takes (a ramp's 16 colours, a gradient's
//! 16 stops), for a native loop over them is not stopped by the deadline
//! (round 2, R2-E-2: a forged ramp of 150 000 colours held a frame for
//! 650 ms against its 8 ms).

use std::time::{Duration, Instant};

use gmnb_theme::layer::{Frame, LayerKind};
use gmnb_theme::theme::{Source, Theme};
use gmnb_theme::{Ctx, Limits};

fn load(name: &str, top: &str, body: &str) -> Theme {
    let dir = std::env::temp_dir().join(format!("gmnb-hostile-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("theme.luau"),
        format!(
            "{top}\nreturn {{ api = 1, name = 'T', layers = {{ background = {{ draw = function(draw, frame)\n{body}\nend }} }} }}"
        ),
    )
    .unwrap();
    let t =
        Theme::load(Source::folder("t", &dir), Limits::default()).unwrap_or_else(|e| panic!("{e}"));
    let _ = std::fs::remove_dir_all(&dir);
    t
}

/// How long a frame of the theme took, and what it said.
fn frame(theme: &Theme) -> (Duration, String) {
    let t = Instant::now();
    let r = theme.draw(
        LayerKind::Background,
        &Ctx::default(),
        &Frame::sized(400.0, 300.0),
    );
    let took = t.elapsed();
    (
        took,
        match r {
            Ok(d) => format!("drawn {}", d.list.ops.len()),
            Err(e) => format!("{e:?}"),
        },
    )
}

/// A ramp the app let go of (nine were made; one is held no more) is made
/// again from the table the theme hands in: a table the theme made up, of
/// a ramp's number and a hundred thousand colours, is not a ramp, and is
/// not read to say so.
#[test]
fn a_forged_ramp_table_is_not_read_to_the_end() {
    let theme = load(
        "ramp",
        r##"
        for i = 1, 10 do gmnb.ramp({ colors = { gmnb.rgb(i / 12, 0, 0), "#0000ff" } }) end
        local one = { r = 0, g = 0, b = 0, a = 1 }
        local many = {}
        for i = 1, 150000 do many[i] = one end
        local forged = { id = 1, colors = many, edge = { 0, 1 }, cycle = false }
        local buf = buffer.create(2 * 2 * 4)
        "##,
        r##"
        draw:field(0, 0, 40, 40, { data = buf, cols = 2, rows = 2, channels = 1, ramp = forged })
        "##,
    );
    let (took, said) = frame(&theme);
    assert!(said.contains("not a ramp of this theme"), "{said}");
    assert!(took < Duration::from_millis(100), "{took:?}");
}

#[test]
fn a_gradient_with_a_hundred_thousand_stops_is_refused_unread() {
    let theme = load(
        "stops",
        r##"
        local stops = {}
        for i = 1, 150000 do stops[i] = 0.5 end
        local g = { kind = "linear", colors = { { r = 0, g = 0, b = 0, a = 1 }, { r = 1, g = 1, b = 1, a = 1 } }, stops = stops }
        "##,
        r##"
        draw:rect(0, 0, 10, 10, g)
        "##,
    );
    let (took, said) = frame(&theme);
    assert!(said.contains("stops"), "{said}");
    assert!(took < Duration::from_millis(100), "{took:?}");
}
