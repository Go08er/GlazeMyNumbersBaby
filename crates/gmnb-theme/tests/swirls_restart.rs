//! Swirls' paint stopped at its deadline (docs/theming.md, "Modules", rule
//! 5): it starts afresh, its memory as on load, and Swirls isn't told. It
//! finds out itself (its paint's `render` answers 0, `made` 0), writes all
//! it had written again, and the frame after is the very picture a Swirls
//! that was never stopped draws; so it is when the frame after the stop
//! brings a new ramp too (the window's opacity changed), flowing or under
//! reduced motion. A process of its own, and one test: what modules have
//! done (`module::stats`) is the process's.

use gmnb_theme::layer::{Drawn, Failure, Frame, LayerKind};
use gmnb_theme::module;
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::{Ctx, Limits, ThemeError};

/// A frame's deadline of `frame` ms, and room for the rest.
fn limits(frame: u64) -> Limits {
    let long = std::time::Duration::from_secs(20);
    Limits {
        load: long,
        style: long,
        frame: std::time::Duration::from_millis(frame),
        ..Limits::default()
    }
}

/// A small window: Swirls' frames there take a small part of 1 ms.
const W: f32 = 120.0;
const H: f32 = 90.0;

/// Swirls, with its paint wrapped (the theme's own `gmnb` shadowed), and a
/// frame's deadline of 1 ms: its `stop_at`-th call of `render` first has
/// the paint colour the most pixels a picture may have, 512 × 512 from two
/// channels (some 2.3 ms a time), so that the deadline stops that call
/// (its timer may be a tick, 1 ms, late), not the theme's own code.
fn stopped_once(stop_at: u32) -> Theme {
    let from =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/gmnb/themes/swirls");
    let dir = std::env::temp_dir().join(format!("gmnb-swirls-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("credits")).unwrap();
    for file in ["paint.wasm", "credits/ColorMix.txt"] {
        std::fs::copy(from.join(file), dir.join(file)).unwrap();
    }
    let source = std::fs::read_to_string(from.join("theme.luau")).unwrap();
    let (first, rest) = source.split_once('\n').unwrap();
    assert_eq!(first, "--!strict");
    let shim = format!(
        r#"
local real = gmnb
local renders = 0
local gmnb = setmetatable({{
	module = function(file)
		local m = real.module(file)
		local proxy = {{ id = m.id, file = m.file }}
		function proxy.call(_, name, ...)
			if name == "render" then
				renders += 1
				if renders == {stop_at} then
					for _ = 1, 10 do
						m:call("render", 0, 2, 2, 2, 0, 0, 1, 0, 512, 512)
					end
				end
			end
			return m:call(name, ...)
		end
		function proxy.write(_, ...) return m:write(...) end
		function proxy.read(_, ...) return m:read(...) end
		function proxy.picture(_) return m:picture() end
		return proxy
	end,
}}, {{ __index = real }})
"#
    );
    std::fs::write(dir.join("theme.luau"), format!("{first}\n{shim}{rest}")).unwrap();
    let theme = Theme::load_ready(Source::folder("swirls", &dir), limits(1)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    theme
}

fn ctx(theme: &Theme, opacity: f32) -> Ctx {
    Ctx {
        dark: true,
        opacity,
        options: theme::complete_options(&theme.def.options, &Default::default()),
        ..Ctx::default()
    }
}

fn draw(theme: &Theme, kind: LayerKind, c: &Ctx, f: &Frame) -> Result<Drawn, Failure> {
    let now = f.time;
    theme.api().set_clock(std::rc::Rc::new(move || 100.0 + now));
    theme.draw(kind, c, f)
}

/// The paint a frame drew: its picture's size and bytes.
fn paint(d: &Drawn) -> (u32, u32, Vec<u8>) {
    let found: Vec<_> = d
        .list
        .ops
        .iter()
        .filter_map(|op| match op {
            gmnb_theme::draw::Op::Image {
                canvas: Some(c), ..
            } => Some((c.width, c.height, c.pixels.to_vec())),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1);
    found.into_iter().next().unwrap()
}

/// This thread's processor time, in milliseconds: what a hook's deadline
/// is of.
fn cpu_ms() -> f64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes the one `timespec` it is given.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) },
        0
    );
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

/// How the frame that runs over its deadline ended.
enum Stopped {
    /// In the paint's own call: it was started afresh.
    InThePaint,
    /// In the theme's code, after a call that returned (its timer may be a
    /// tick late: up to 4 ms on a 250 Hz kernel): this try tells nothing.
    InLuau,
}

/// What the frames after the stop are drawn with.
#[derive(Clone, Copy, Debug)]
enum Then {
    /// What the frame stopped was.
    Same,
    /// Another opacity, from the very draw after the stop: a new ramp for
    /// the paint as it finds out it was started afresh.
    Opacity,
}

/// One try: Swirls stopped once and one never stopped, the same frames
/// (`motion`: flowing, or under reduced motion), drawn with the same
/// context. The most processor time an ordinary frame of the stopped one
/// took comes back too.
fn one_try(motion: bool, then: Then) -> (Stopped, f64) {
    let mut undisturbed =
        Theme::load_ready(Source::built_in("swirls").unwrap(), limits(8)).unwrap();
    // Flowing, the paint makes a picture each thirtieth: the twelfth is
    // stopped. Still, it makes one when its ramp changes: the frame after
    // the first is at another opacity, and its picture is stopped.
    let mut theme = stopped_once(if motion { 12 } else { 2 });
    let first = ctx(&theme, 1.0);
    let stopped_at = if motion {
        first.clone()
    } else {
        ctx(&theme, 0.75)
    };
    let after = match then {
        Then::Same => stopped_at.clone(),
        Then::Opacity => ctx(&theme, 0.5),
    };
    // (Their colours worked out by `style`, on the style's time: a frame
    // then has only the paint to do. The same for both: a frame that
    // worked them out would be a long one, which Swirls' guard against a
    // slow machine would see.)
    for c in [&first, &stopped_at, &after] {
        theme.style(c).unwrap();
        undisturbed.style(c).unwrap();
    }
    let before = module::stats();
    let (mut stopped, mut longest) = (None, 0.0f64);
    for i in 0..60u32 {
        let f = Frame {
            time: f64::from(i) / 30.0,
            dt: 1.0 / 30.0,
            motion,
            ..Frame::sized(W, H)
        };
        let c = match stopped {
            Some(_) => &after,
            None if motion || i == 0 => &first,
            None => &stopped_at,
        };
        // The one never stopped draws what the other drew: not the frame
        // stopped, which took nothing (had it drawn that frame, it would
        // have spent its layer's allowance on one more picture, and been
        // refused one a few frames on).
        let started = cpu_ms();
        let got = draw(&theme, LayerKind::Background, c, &f);
        let took = cpu_ms() - started;
        match got {
            Ok(got) => {
                longest = longest.max(took);
                let want = draw(&undisturbed, LayerKind::Background, c, &f).unwrap();
                assert_eq!(paint(&got), paint(&want), "frame {i}");
            }
            Err(Failure::Failed(ThemeError::Timeout(_))) => {
                assert!(stopped.is_none(), "frame {i}: a second frame ran over");
                if module::stats().timeouts == before.timeouts {
                    return (Stopped::InLuau, longest);
                }
                assert_eq!(module::stats().timeouts, before.timeouts + 1, "frame {i}");
                stopped = Some(i);
            }
            Err(e) => panic!("frame {i}: {e:?}"),
        }
        // Drawn again inside the same thirtieth: the same picture (after
        // the stop, with what follows it).
        let again = Frame {
            time: f.time + 0.004,
            ..f.clone()
        };
        let c = if stopped.is_some() { &after } else { c };
        let started = cpu_ms();
        let got = draw(&theme, LayerKind::Background, c, &again);
        longest = longest.max(cpu_ms() - started);
        let want = draw(&undisturbed, LayerKind::Background, c, &again).unwrap();
        assert_eq!(paint(&got.unwrap()), paint(&want), "frame {i}, again");
    }
    assert!(stopped.is_some(), "the paint was never stopped");
    let after = module::stats();
    assert_eq!(after.timeouts, before.timeouts + 1);
    // Started afresh once: one instance more than the two themes' own.
    assert_eq!(after.instances, before.instances + 1);
    // One strike, and the hook still on.
    let hook = theme
        .hooks()
        .into_iter()
        .find(|(name, _, _)| name == "background.draw")
        .unwrap();
    assert_eq!((hook.1, hook.2), (1, false));
    assert!(theme.take_draw_notes().is_empty());
    (Stopped::InThePaint, longest)
}

#[test]
fn swirls_writes_its_paint_again_after_a_timeout() {
    // One after another (what modules have done is the process's).
    for (motion, then) in [
        (true, Then::Same),
        (true, Then::Opacity),
        (false, Then::Same),
        (false, Then::Opacity),
    ] {
        stopped_in_the_paint(motion, then);
    }
}

/// A try whose stop landed in the theme's own code (the paint's call had
/// returned before its timer fired) is made again, on new themes.
fn stopped_in_the_paint(motion: bool, then: Then) {
    for attempt in 1..=12 {
        let (how, longest) = one_try(motion, then);
        println!(
            "motion {motion}, then {then:?}, try {attempt}: an ordinary frame took {longest:.2} ms \
             at most of the 1 ms"
        );
        if let Stopped::InThePaint = how {
            return;
        }
    }
    panic!("motion {motion}, then {then:?}: in twelve tries the deadline never stopped the paint");
}
