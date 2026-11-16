//! Swirls' paint stopped at its deadline (docs/theming.md, "Modules", rule
//! 5): it starts afresh, its memory as on load, and Swirls isn't told. It
//! finds out itself (its paint's `render` answers 0, `made` 0), writes all
//! it had written again, and the frame after is the very picture a Swirls
//! that was never stopped draws. A process of its own: what modules have
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

fn ctx(theme: &Theme) -> Ctx {
    Ctx {
        dark: true,
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

#[test]
fn swirls_writes_its_paint_again_after_a_timeout() {
    let undisturbed = Theme::load_ready(Source::built_in("swirls").unwrap(), limits(8)).unwrap();
    let mut theme = stopped_once(12);
    let c = ctx(&theme);
    // (Its colours worked out by `style`, on the style's time: a frame
    // then has only the paint to do.)
    theme.style(&c).unwrap();
    let before = module::stats();
    let mut stopped = 0;
    for i in 0..60u32 {
        let f = Frame {
            time: f64::from(i) / 30.0,
            dt: 1.0 / 30.0,
            ..Frame::sized(W, H)
        };
        let want = draw(&undisturbed, LayerKind::Background, &c, &f).unwrap();
        match draw(&theme, LayerKind::Background, &c, &f) {
            Ok(got) => assert_eq!(paint(&got), paint(&want), "frame {i}"),
            Err(Failure::Failed(ThemeError::Timeout(_))) => {
                stopped += 1;
                // The paint's call ran out of the frame's time, not Luau.
                assert_eq!(module::stats().timeouts, before.timeouts + 1, "frame {i}");
            }
            Err(e) => panic!("frame {i}: {e:?}"),
        }
        // Drawn again inside the same thirtieth: the same picture, the
        // paint not asked again.
        let again = Frame {
            time: f.time + 0.004,
            ..f.clone()
        };
        let want = draw(&undisturbed, LayerKind::Background, &c, &again).unwrap();
        if let Ok(got) = draw(&theme, LayerKind::Background, &c, &again) {
            assert_eq!(paint(&got), paint(&want), "frame {i}, again");
        }
    }
    assert_eq!(stopped, 1);
    let after = module::stats();
    assert_eq!(after.timeouts, before.timeouts + 1);
    // Started afresh once: one instance more than the undisturbed theme's
    // two (each one's, at load).
    assert_eq!(after.instances, before.instances + 1);
    // One strike, and the hook still on.
    let hook = theme
        .hooks()
        .into_iter()
        .find(|(name, _, _)| name == "background.draw")
        .unwrap();
    assert_eq!((hook.1, hook.2), (1, false));
    assert!(theme.take_draw_notes().is_empty());
}
