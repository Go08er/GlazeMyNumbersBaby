//! A theme's WebAssembly modules (docs/theming.md, "Modules"), as a theme
//! reaches them: a module that fills a picture, end to end; every limit
//! hit on purpose; modules that loop, recurse, trap, grow and ask for what
//! they may not have (the 0.4 spike's hostile cases among them); a call's
//! deadline, which `pcall` doesn't dodge; the allowance of a module's
//! picture; and the theme's hooks held until its modules are ready. The
//! modules are written as text (`wat`), or byte by byte where text can't
//! say it. That nothing is made for a theme without modules is
//! `module_lazy.rs`'s, a process of its own.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gmnb_theme::draw::{Filter, Op};
use gmnb_theme::layer::{DisplayValue, Drawn, Failure, Frame, GlyphBox, Glyphs, LayerKind, Role};
use gmnb_theme::picture::{EARLIER_HELD, ImageId, TAKERS};
use gmnb_theme::theme::{Source, Theme};
use gmnb_theme::{Budget, Ctx, Limits, ThemeError};

/// The real frame deadline (8 ms), and room everywhere a test isn't about
/// time: the machine these run on is shared.
fn limits() -> Limits {
    let long = Duration::from_secs(20);
    Limits {
        load: long,
        style: long,
        preview: long,
        action: long,
        ..Limits::default()
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

struct Dir(std::path::PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let dir = std::env::temp_dir().join(format!("gmnb-modules-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Dir(dir)
}

impl Dir {
    fn put(&self, name: &str, bytes: &[u8]) -> &Dir {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
        self
    }

    fn wat(&self, name: &str, text: &str) -> &Dir {
        self.put(name, &wat::parse_str(text).unwrap())
    }

    /// The theme of `source`, loaded (its modules named, not compiled).
    fn load(&self, source: &str) -> Theme {
        std::fs::write(self.0.join("theme.luau"), source).unwrap();
        match Theme::load(Source::folder("t", &self.0), limits()) {
            Ok(theme) => theme,
            Err(e) => panic!("{e}"),
        }
    }

    /// A theme whose background layer's `draw` is `body` (with `draw` and
    /// `frame` in scope), after `top` at the file's top level: loaded, and
    /// its modules compiled (on a thread of their own) and started.
    fn background(&self, top: &str, body: &str) -> Theme {
        let mut theme = self.load(&format!(
            "{top}\nreturn {{ api = 1, name = 'T', layers = {{ background = {{ draw = function(draw, frame)\n{body}\nend }} }} }}"
        ));
        ready(&mut theme);
        theme
    }
}

/// Compiles what `theme` names on a thread of its own, as the app does,
/// and installs it.
fn ready(theme: &mut Theme) {
    if let Some(job) = theme.prepare_modules() {
        let (tx, rx) = std::sync::mpsc::channel();
        let thread = job
            .spawn(move |done| tx.send(done).unwrap())
            .unwrap_or_else(|_| panic!("no compile thread"));
        let done = rx.recv().unwrap();
        thread.join().unwrap();
        theme.install_modules(done);
    }
    assert!(theme.modules_ready());
}

fn run(theme: &Theme, frame: &Frame) -> Result<Drawn, Failure> {
    theme.draw(LayerKind::Background, &Ctx::default(), frame)
}

fn drawn(theme: &Theme) -> Drawn {
    match run(theme, &Frame::sized(400.0, 300.0)) {
        Ok(drawn) => drawn,
        Err(e) => panic!("{e:?}"),
    }
}

fn failed(theme: &Theme) -> Failure {
    match run(theme, &Frame::sized(400.0, 300.0)) {
        Ok(drawn) => panic!("drew {drawn:?}"),
        Err(e) => e,
    }
}

/// What the theme's author is told: of loading (what was left out), and
/// of its calls since the last look.
fn notes(theme: &Theme) -> Vec<String> {
    let mut all: Vec<String> = theme
        .notes
        .list
        .iter()
        .map(|n| format!("{}: {}", n.path, n.message))
        .collect();
    all.extend(theme.take_draw_notes().into_iter().map(|n| n.message));
    all
}

fn told(all: &[String], part: &str) {
    assert!(all.iter().any(|n| n.contains(part)), "{part}: {all:?}");
}

/// A clock the test holds, for the allowances.
fn clock(theme: &Theme, start: f64) -> Rc<Cell<f64>> {
    let now = Rc::new(Cell::new(start));
    let read = now.clone();
    theme.api().set_clock(Rc::new(move || read.get()));
    now
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

/// What the background's draw hook has failed, and whether it is off.
fn strikes(theme: &Theme) -> (u32, bool) {
    theme
        .hooks()
        .into_iter()
        .find(|(name, _, _)| name == "background.draw")
        .map(|(_, n, off)| (n, off))
        .unwrap()
}

/// `fill(w, h, out)`: the `w × h` picture at `out`, pixel (x, y) =
/// (7x, 13y, 3(x + y), 255) (each byte wrapping), and its size in bytes.
/// `bump()` adds one to the number at 0 and returns it; `spin(s)` never
/// returns; `nan()` the bits of 0 / 0; `twice(x)` 2x through SIMD,
/// `simd_nan()` the bits of 0 / 0 through SIMD; `echo32`, `echo64`,
/// `echof` give back what they are given; `pair` two numbers.
const PAINT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "fill") (param $w i32) (param $h i32) (param $out i32) (result i32)
    (local $x i32) (local $y i32) (local $p i32)
    (local.set $p (local.get $out))
    (block $done (loop $rows
      (br_if $done (i32.ge_u (local.get $y) (local.get $h)))
      (local.set $x (i32.const 0))
      (block $row (loop $cols
        (br_if $row (i32.ge_u (local.get $x) (local.get $w)))
        (i32.store8 (local.get $p) (i32.mul (local.get $x) (i32.const 7)))
        (i32.store8 offset=1 (local.get $p) (i32.mul (local.get $y) (i32.const 13)))
        (i32.store8 offset=2 (local.get $p)
          (i32.mul (i32.add (local.get $x) (local.get $y)) (i32.const 3)))
        (i32.store8 offset=3 (local.get $p) (i32.const 255))
        (local.set $p (i32.add (local.get $p) (i32.const 4)))
        (local.set $x (i32.add (local.get $x) (i32.const 1)))
        (br $cols)))
      (local.set $y (i32.add (local.get $y) (i32.const 1)))
      (br $rows)))
    (i32.mul (i32.mul (local.get $w) (local.get $h)) (i32.const 4)))
  (func (export "bump") (result i32)
    (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))
    (i32.load (i32.const 0)))
  (func (export "spin") (param $s i32) (result i32)
    (loop $l
      (local.set $s (i32.add (i32.mul (local.get $s) (i32.const 1103515245)) (i32.const 12345)))
      (br $l))
    (local.get $s))
  (func (export "nan") (result i32)
    (i32.reinterpret_f32 (f32.div (f32.const 0) (f32.const 0))))
  (func (export "twice") (param f32) (result f32)
    (f32x4.extract_lane 0 (f32x4.mul (f32x4.splat (local.get 0)) (f32x4.splat (f32.const 2)))))
  (func (export "simd_nan") (result i32)
    (i32.reinterpret_f32 (f32x4.extract_lane 1
      (f32x4.div (f32x4.splat (f32.const 0)) (f32x4.splat (f32.const 0))))))
  (func (export "echo32") (param i32) (result i32) (local.get 0))
  (func (export "echo64") (param i64) (result i64) (local.get 0))
  (func (export "echof") (param f64) (result f64) (local.get 0))
  (func (export "pair") (result i32 f64) (i32.const 3) (f64.const 0.5))
  (func (export "vector") (param v128) (result i32) (i32.const 0)))
"#;

/// The pixels `fill(w, h, _)` makes.
fn painted(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            out.extend([(x * 7) as u8, (y * 13) as u8, ((x + y) * 3) as u8, 255]);
        }
    }
    out
}

// ---- A module that fills a picture -------------------------------------------------

#[test]
fn a_module_fills_a_picture_end_to_end() {
    let d = dir("fill");
    d.wat("paint.wasm", PAINT);
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        assert(paint == gmnb.module("paint.wasm"), "the same file is the same module")
        assert(paint.file == "paint.wasm")
        assert(not pcall(function() paint.file = "x" end), "read-only")
        local picture = paint:picture()
        assert(picture.width == 0 and picture.height == 0, "no size before its first take")
        assert(picture ~= paint:picture(), "each call makes another")
        local OUT = 1024
        "#,
        r#"
        assert(paint:call("fill", 16, 8, OUT) == 16 * 8 * 4)
        assert(picture:ready())
        assert(picture:take(paint, OUT, 16, 8) == true)
        assert(picture.width == 16 and picture.height == 8)
        draw:image(picture, 0, 0, frame.width, frame.height)
        draw:image(picture, 0, 0, 32, 16, { filter = "nearest" })
        "#,
    );
    let d0 = drawn(&theme);
    assert!(matches!(
        d0.list.ops[0],
        Op::Image {
            image: ImageId(0),
            filter: Filter::Linear,
            ..
        }
    ));
    assert!(matches!(
        d0.list.ops[1],
        Op::Image {
            filter: Filter::Nearest,
            ..
        }
    ));
    let canvas = theme.api().canvas(ImageId(0)).unwrap();
    assert_eq!((canvas.width, canvas.height, canvas.version), (16, 8, 1));
    assert_eq!(&canvas.pixels[..], &painted(16, 8)[..]);
    assert_eq!(fnv(&canvas.pixels), 0x5b09_2a39_212b_6805);
    // Charged to the image total at its size; the second picture, never
    // taken, at none.
    assert_eq!(theme.api().pictures().decoded_bytes(), 16 * 8 * 4);
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn the_same_pixels_on_every_machine() {
    let d = dir("same");
    d.wat("paint.wasm", PAINT);
    let theme = d.background(
        r#"local paint = gmnb.module("paint.wasm")"#,
        r#"
        -- NaNs are canonical, scalar and SIMD: the positive quiet NaN.
        assert(paint:call("nan") == 0x7fc00000, paint:call("nan"))
        assert(paint:call("simd_nan") == 0x7fc00000, paint:call("simd_nan"))
        assert(paint:call("twice", 1.25) == 2.5)
        -- Numbers in and out: an i32 takes -2^31 to 2^32 - 1 (the top half
        -- as its bits), an i64 whole numbers, a float any.
        assert(paint:call("echo32", -5) == -5)
        assert(paint:call("echo32", 4294967295) == -1)
        assert(paint:call("echo64", 2^40) == 2^40)
        assert(paint:call("echof", 0.1) == 0.1)
        local a, b = paint:call("pair")
        assert(a == 3 and b == 0.5)
        "#,
    );
    drawn(&theme);
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn memory_written_and_read_back() {
    let d = dir("memory");
    d.wat("paint.wasm", PAINT);
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        local buf = buffer.create(8)
        buffer.writeu32(buf, 0, 41)
        buffer.writeu32(buf, 4, 0xdeadbeef)
        "#,
        r#"
        paint:write(0, buf)
        assert(paint:call("bump") == 42)
        local back = paint:read(0, 8)
        assert(buffer.len(back) == 8)
        assert(buffer.readu32(back, 0) == 42 and buffer.readu32(back, 4) == 0xdeadbeef)
        "#,
    );
    drawn(&theme);
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

// ---- Wrong calls do nothing, with a note once per call site -----------------------

#[test]
fn wrong_calls_do_nothing_and_are_noted_once_per_call_site() {
    let d = dir("wrong");
    d.wat("paint.wasm", PAINT);
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        local picture = paint:picture()
        local small = buffer.create(16)
        local n = 0
        "#,
        r#"
        n += 1
        -- A function that isn't there, the wrong number or kind of
        -- arguments, a function a theme can't call: nothing comes back.
        assert(select('#', paint:call("nowhere")) == 0)
        assert(select('#', paint:call("fill", 1, 2)) == 0)
        assert(select('#', paint:call("fill", 1, 2, "x")) == 0)
        assert(select('#', paint:call("echo32", 0.5)) == 0)
        assert(select('#', paint:call("echo32", 2^32)) == 0)
        assert(select('#', paint:call("vector", 1)) == 0)
        assert(select('#', paint:call(7)) == 0)
        -- Called with a dot, not a colon.
        assert(select('#', paint.call("fill", 1, 1, 0)) == 0)
        -- Offsets and lengths outside its memory of 64 KiB.
        paint:write(65536 - 8, small)
        paint:write(-1, small)
        paint:write(0, "bytes")
        assert(paint:read(65530, 8) == nil)
        assert(paint:read(0, 2 ^ 21) == nil)
        assert(picture:take(paint, 65536 - 16, 4, 4) == false)
        assert(picture:take(paint, 0, 16385, 1) == false)
        assert(picture:take(paint, 0, 0, 4) == false)
        assert(picture:take(paint, 0, 600, 600) == false)
        assert(picture:take(paint, 0, 2^32 - 1, 2^32 - 1) == false)
        assert(picture:take({}, 0, 1, 1) == false)
        -- The offset changes each frame; the note doesn't.
        assert(picture:take(paint, 65536 + n, 1, 1) == false)
        "#,
    );
    drawn(&theme);
    let first = notes(&theme);
    for part in [
        "has no function \"nowhere\"",
        "fill takes (i32, i32, i32); it was given 2 arguments",
        "fill takes (i32, i32, i32); it was given 3 arguments",
        "echo32 takes (i32); it was given 1 arguments",
        "vector takes or returns something other than numbers",
        "the function to call is named by a string",
        "module:call was called on something that isn't a module",
        "16 bytes at 65528 don't fit its memory of 65536",
        "it takes an offset (a whole number) and a buffer",
        "8 bytes at 65530 aren't in its memory of 65536",
        "2097152 bytes; one read is at most 1048576",
        "4×4 at 65520 isn't in its memory of 65536",
        "16385×1; a module's picture is at most 262144 pixels, 1 to 16384 a side",
        "0×4; a module's picture is at most 262144 pixels",
        "600×600; a module's picture is at most 262144 pixels",
        "4294967295×4294967295; a module's picture is at most 262144 pixels",
        "picture:take takes a module",
        "1×1 at 65537 isn't in its memory",
    ] {
        assert!(first.iter().any(|n| n.contains(part)), "{part}: {first:?}");
    }
    // Each note names where the call was made.
    assert!(
        first.iter().all(|n| n.starts_with("t/theme.luau:")),
        "{first:?}"
    );
    // Drawn again, and with another offset: nothing more is said.
    drawn(&theme);
    drawn(&theme);
    assert!(notes(&theme).is_empty());
    // Nothing went wrong for the hook.
    assert_eq!(strikes(&theme), (0, false));
    assert!(theme.api().canvas(ImageId(0)).unwrap().pixels.is_empty());
}

// ---- Limits -------------------------------------------------------------------------

/// A module of `bytes` bytes of data (and one page of memory to hold it).
fn data_module(bytes: usize) -> String {
    let pages = bytes / 65536 + 1;
    format!(
        "(module (memory {pages}) (data (i32.const 0) \"{}\"))",
        "\\00".repeat(bytes)
    )
}

#[test]
fn at_most_four_modules_and_256_kib_of_them() {
    let d = dir("many");
    for name in ["a", "b", "c", "d", "e"] {
        d.wat(&format!("{name}.wasm"), PAINT);
    }
    let theme = d.background(
        r#"
        local all = {}
        for _, name in { "a", "b", "c", "d", "e" } do
            all[name] = gmnb.module(name .. ".wasm")
        end
        -- Not a .wasm file in the theme's folder.
        local bad = { gmnb.module("../paint.wasm"), gmnb.module("paint.bin"), gmnb.module("/abs.wasm") }
        "#,
        r#"
        assert(all.d:call("bump") == 1)
        -- The fifth is one too many: its calls do nothing.
        assert(select('#', all.e:call("bump")) == 0)
        assert(select('#', bad[1]:call("bump")) == 0)
        "#,
    );
    assert_eq!(theme.modules(), ["a.wasm", "b.wasm", "c.wasm", "d.wasm"]);
    drawn(&theme);
    let all = notes(&theme);
    for part in [
        "e.wasm: a theme may have 4 modules; left out",
        "\"../paint.wasm\" is not a .wasm file's path inside the theme's folder",
        "\"paint.bin\" is not a .wasm file's path",
        "\"/abs.wasm\" is not a .wasm file's path",
        "this module was left out",
    ] {
        told(&all, part);
    }

    // 256 KiB together, counted before anything is compiled: the module
    // that would go past them is left out, and those after it are read
    // if they fit.
    let d = dir("bytes");
    d.wat("one.wasm", &data_module(150 << 10))
        .wat("two.wasm", &data_module(150 << 10))
        .wat("three.wasm", PAINT);
    // 32 MiB: its size is enough to refuse it; it isn't read.
    let mut huge = wat::parse_str("(module)").unwrap();
    huge.resize(32 << 20, 0);
    d.put("huge.wasm", &huge);
    let mut theme = d.load(
        r#"
        local one, two, three, huge = gmnb.module("one.wasm"), gmnb.module("two.wasm"),
            gmnb.module("three.wasm"), gmnb.module("huge.wasm")
        return { api = 1, name = "T", layers = { background = { draw = function()
            assert(select('#', two:call("x")) == 0)
            assert(three:call("bump") == 1)
        end } } }
        "#,
    );
    let t = Instant::now();
    ready(&mut theme);
    assert!(t.elapsed() < Duration::from_secs(10));
    drawn(&theme);
    let all = notes(&theme);
    told(
        &all,
        "two.wasm: the theme's modules are over 256 KiB together; left out",
    );
    told(
        &all,
        "huge.wasm: the theme's modules are over 256 KiB together",
    );
    told(&all, "two.wasm was left out");
}

#[test]
fn what_a_module_may_not_be_is_refused_before_it_is_compiled() {
    let d = dir("refused");
    let paint = wat::parse_str(PAINT).unwrap();
    let mut garbage = vec![0u8; 4096];
    let mut s = 0x1234_5678u32;
    for b in &mut garbage {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        *b = s as u8;
    }
    let mut bad_magic = paint.clone();
    bad_magic[0] = b'X';
    let mut header_then_garbage = paint[..8].to_vec();
    header_then_garbage.extend_from_slice(&garbage);
    d.put("random.wasm", &garbage)
        .put("magic.wasm", &bad_magic)
        .put("header.wasm", &header_then_garbage)
        .put("half.wasm", &paint[..paint.len() / 2])
        .wat(
            "import.wasm",
            r#"(module (import "env" "f" (func)) (func (export "g") call 0))"#,
        )
        .wat("start.wasm", "(module (func $s (loop (br 0))) (start $s))")
        .wat("pages.wasm", "(module (memory 65536))")
        .wat("over.wasm", "(module (memory 1025))")
        .put("text.wasm", b"(module)")
        .wat("table.wasm", "(module (table 100000 funcref))")
        .put("component.wasm", &[0, b'a', b's', b'm', 0x0d, 0, 1, 0]);
    let files = [
        (
            "random.wasm",
            "is not a WebAssembly module (by its first bytes)",
        ),
        (
            "magic.wasm",
            "is not a WebAssembly module (by its first bytes)",
        ),
        ("header.wasm", "is not a well-formed"),
        ("half.wasm", "is not a well-formed"),
        (
            "import.wasm",
            "imports env.f: a theme's module may import nothing",
        ),
        ("start.wasm", "has a start function"),
        (
            "pages.wasm",
            "declares 4096 MiB of memory; a module's memory is at most 64 MiB",
        ),
        ("over.wasm", "declares 64 MiB of memory"),
        (
            "text.wasm",
            "is not a WebAssembly module (by its first bytes)",
        ),
        ("table.wasm", "couldn't be started"),
        ("component.wasm", "is a component"),
    ];
    // Four at a time: a theme may have four.
    for group in files.chunks(4) {
        let names: Vec<String> = group.iter().map(|(f, _)| format!("{f:?}")).collect();
        let mut theme = d.load(&format!(
            r#"
            local all = {{}}
            for _, f in {{ {} }} do all[f] = gmnb.module(f) end
            return {{ api = 1, name = "T", layers = {{ background = {{ draw = function()
                for f, m in all do assert(select('#', m:call("g")) == 0, f) end
            end }} }} }}
            "#,
            names.join(", ")
        ));
        ready(&mut theme);
        drawn(&theme);
        let all = notes(&theme);
        for (file, why) in group {
            told(&all, &format!("{file}: "));
            assert!(
                theme
                    .notes
                    .list
                    .iter()
                    .any(|n| n.path == *file && n.message.contains(why)),
                "{file}: {why}: {:?}",
                theme.notes.list
            );
        }
        assert_eq!(strikes(&theme), (0, false));
    }
    // 1024 pages is the cap itself: that one is fine.
    let d = dir("cap");
    d.wat(
        "cap.wasm",
        r#"(module (memory (export "memory") 1024) (func (export "size") (result i32) memory.size))"#,
    );
    let theme = d.background(
        r#"local cap = gmnb.module("cap.wasm")"#,
        r#"assert(cap:call("size") == 1024)"#,
    );
    drawn(&theme);
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn memory_and_tables_grow_no_further_than_their_caps() {
    let d = dir("grow");
    d.wat(
        "grow.wasm",
        r#"
        (module
          (memory (export "memory") 1)
          (table 1 funcref)
          (func (export "grow") (result i32)
            (loop $more
              (br_if $more (i32.ne (memory.grow (i32.const 1)) (i32.const -1))))
            (memory.size))
          (func (export "leap") (result i32) (memory.grow (i32.const 65535)))
          (func (export "tables") (result i32)
            (table.grow (ref.null func) (i32.const 10000000))))
        "#,
    );
    let theme = d.background(
        r#"local m = gmnb.module("grow.wasm")"#,
        r#"
        -- 4 GiB at once: refused, -1.
        assert(m:call("leap") == -1)
        -- A page at a time, refused at 64 MiB (1024 pages).
        assert(m:call("grow") == 1024, m:call("grow"))
        assert(m:call("tables") == -1)
        -- Its memory is as big as it grew.
        assert(buffer.len(m:read(64 * 1024 * 1024 - 4, 4)) == 4)
        "#,
    );
    drawn(&theme);
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

// ---- Time --------------------------------------------------------------------------

const LOOPS: &str = r#"local m = gmnb.module("paint.wasm")"#;

#[test]
fn an_endless_loop_in_a_frame_is_a_timeout_and_three_switch_the_hook_off() {
    let d = dir("spin");
    d.wat("paint.wasm", PAINT);
    let theme = d.background(LOOPS, r#"m:call("spin", 7)"#);
    for strike in 1..=3 {
        let t = Instant::now();
        assert_eq!(
            failed(&theme),
            Failure::Failed(ThemeError::Timeout(ms(8))),
            "strike {strike}"
        );
        assert!(t.elapsed() < Duration::from_secs(5));
        assert_eq!(strikes(&theme), (strike, strike == 3));
    }
    assert_eq!(failed(&theme), Failure::Off);
}

/// A thread whose mask blocks the timer's signal (masks are inherited)
/// still has its module stopped at the hook's deadline.
#[test]
fn a_call_is_stopped_on_a_thread_that_blocks_the_timers_signal() {
    let d = dir("blocked");
    d.wat("paint.wasm", PAINT);
    std::fs::write(
        d.0.join("theme.luau"),
        format!(
            "{LOOPS}\nreturn {{ api = 1, name = 'T', layers = {{ background = {{ draw = function() m:call('spin', 7) end }} }} }}"
        ),
    )
    .unwrap();
    let path = d.0.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // SAFETY: blocks one signal in this thread's own mask.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGRTMIN() + 3);
            libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        }
        let mut theme = Theme::load(Source::folder("t", &path), limits()).unwrap();
        ready(&mut theme);
        let _ = tx.send(run(&theme, &Frame::sized(10.0, 10.0)).err());
    });
    // A call nothing stops would never come back.
    let got = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the call was stopped");
    assert_eq!(got, Some(Failure::Failed(ThemeError::Timeout(ms(8)))));
}

#[test]
fn a_module_call_runs_inside_whatever_deadline_its_hook_has() {
    let d = dir("load");
    d.wat("paint.wasm", PAINT);
    std::fs::write(
        d.0.join("theme.luau"),
        format!(
            "{LOOPS}\nreturn {{ api = 1, name = 'T', layers = {{ background = {{ draw = function() m:call('spin', 7) end }} }} }}"
        ),
    )
    .unwrap();
    // The real limits: a load's 250 ms.
    let mut theme = Theme::load(Source::folder("t", &d.0), Limits::default()).unwrap();
    ready(&mut theme);
    // A hook given a load's 250 ms: the module is stopped at 250 ms, not
    // at a frame's 8.
    let hook = theme
        .layer(LayerKind::Background)
        .and_then(|l| l.draw.clone())
        .unwrap();
    let t = Instant::now();
    assert_eq!(
        theme.engine().call(&hook, Budget::Load, ()),
        Err(ThemeError::Timeout(ms(250)))
    );
    assert!(t.elapsed() >= ms(250));
    assert!(t.elapsed() < Duration::from_secs(5));
    // A span shared with other calls: what is left of it.
    let span = theme.engine().span(Budget::Custom(ms(40)));
    assert_eq!(
        theme.engine().call_in(&hook, &span, ()),
        Err(ThemeError::Timeout(ms(40)))
    );
    // Over before the call: it fails without running.
    assert_eq!(
        theme.engine().call_in(&hook, &span, ()),
        Err(ThemeError::Timeout(ms(40)))
    );
}

#[test]
fn pcall_doesnt_dodge_the_deadline() {
    let d = dir("pcall");
    d.wat("paint.wasm", PAINT);
    for body in [
        // Caught, then the hook loops in Luau.
        r#"local ok = pcall(m.call, m, "spin", 7) assert(not ok) while true do end"#,
        // Caught, and the hook returns as if nothing happened.
        r#"pcall(m.call, m, "spin", 7) return 1"#,
        // A hundred times: each after the first fails at once.
        r#"for _ = 1, 100 do pcall(m.call, m, "spin", 7) end return 1"#,
        r#"for _ = 1, 100 do pcall(function() m:call("spin", 7) end) end"#,
    ] {
        let theme = d.background(LOOPS, body);
        let span = gmnb_theme::Span::new(Duration::from_secs(60));
        assert_eq!(
            failed(&theme),
            Failure::Failed(ThemeError::Timeout(ms(8))),
            "{body}"
        );
        // One deadline's worth, not a hundred: one strike, not switched
        // off for running four times over.
        assert!(span.used() < ms(32), "{body}: {:?}", span.used());
        assert_eq!(strikes(&theme), (1, false), "{body}");
    }
}

#[test]
fn after_a_timeout_the_module_starts_afresh_and_after_a_trap_it_doesnt() {
    let d = dir("fresh");
    d.wat("paint.wasm", PAINT).wat(
        "trap.wasm",
        r#"(module (memory (export "memory") 1)
             (func (export "bump") (result i32)
               (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))
               (i32.load (i32.const 0)))
             (func (export "boom") unreachable))"#,
    );
    let theme = d.background(
        r#"
        local m = gmnb.module("paint.wasm")
        local t = gmnb.module("trap.wasm")
        local n = 0
        "#,
        r#"
        n += 1
        if n == 3 then m:call("spin", 7) end
        if n == 6 then t:call("boom") end
        return m:call("bump") * 1000 + t:call("bump")
        "#,
    );
    let mut got = Vec::new();
    for _ in 0..8 {
        got.push(run(&theme, &Frame::sized(10.0, 10.0)).map(|d| d.again));
    }
    let n = |a: f64| Ok(Some(a));
    assert_eq!(
        got,
        [
            n(1001.0),
            n(2002.0),
            // Stopped: the module of the loop starts afresh (its count
            // back at 0); the other keeps its own.
            Err(Failure::Failed(ThemeError::Timeout(ms(8)))),
            n(1003.0),
            n(2004.0),
            // A trap is an error in the hook, and the module keeps its
            // memory.
            got[5].clone(),
            n(3005.0),
            n(4006.0),
        ]
    );
    match &got[5] {
        Err(Failure::Failed(ThemeError::Runtime { message, .. })) => {
            assert!(
                message.contains(
                    "module:call: trap.wasm: boom: wasm `unreachable` instruction executed"
                ),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(strikes(&theme), (2, false));
}

#[test]
fn recursion_and_traps_are_errors_in_the_hook() {
    let d = dir("traps");
    d.wat(
        "bad.wasm",
        r#"
        (module
          (memory (export "memory") 1)
          (func $r (export "recurse") (param i32) (result i32)
            (call $r (i32.add (local.get 0) (i32.const 1))))
          (func (export "divide") (param i32) (result i32) (i32.div_s (i32.const 1) (local.get 0)))
          (func (export "past") (result i32) (i32.load (i32.const 0x7ffffff0)))
          (func (export "unreachable") unreachable)
          (func (export "convert") (param f32) (result i32) (i32.trunc_f32_s (local.get 0)))
          (func (export "fine") (result i32) (i32.const 5)))
        "#,
    );
    for (call, says) in [
        (r#""recurse", 0"#, "call stack exhausted"),
        (r#""divide", 0"#, "integer divide by zero"),
        (r#""past""#, "out of bounds memory access"),
        (
            r#""unreachable""#,
            "wasm `unreachable` instruction executed",
        ),
        (r#""convert", 1e30"#, "integer overflow"),
    ] {
        let theme = d.background(
            r#"local bad = gmnb.module("bad.wasm")"#,
            &format!(
                r#"
                local ok, e = pcall(bad.call, bad, {call})
                assert(not ok and string.find(tostring(e), "{says}", 1, true), tostring(e))
                assert(bad:call("fine") == 5)
                bad:call({call})
                "#
            ),
        );
        let t = Instant::now();
        match failed(&theme) {
            Failure::Failed(ThemeError::Runtime { message, .. }) => {
                assert!(message.contains(says), "{message}");
            }
            other => panic!("{call}: {other:?}"),
        }
        assert!(t.elapsed() < Duration::from_secs(5));
        assert_eq!(strikes(&theme), (1, false));
    }
}

// ---- Pictures -------------------------------------------------------------------------

#[test]
fn a_module_picture_keeps_the_allowance_of_a_picture_a_theme_fills() {
    let d = dir("allowance");
    d.wat("paint.wasm", PAINT);
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        local picture = paint:picture()
        local n = 0
        "#,
        r#"
        -- Something else each time it is taken.
        n += 1
        paint:call("fill", 4, 2 + n % 2, 0)
        local taken = picture:take(paint, 0, 4, 2 + n % 2)
        -- The same bytes again: taken, and nothing spent.
        if taken then
            for _ = 1, 3 do assert(picture:take(paint, 0, 4, 2 + n % 2)) end
        end
        draw:image(picture, 0, 0, frame.width, frame.height)
        return if taken then 1 else 2
        "#,
    );
    let now = clock(&theme, 100.0);
    assert_eq!(drawn(&theme).again, Some(1.0));
    let second = drawn(&theme);
    assert_eq!((second.again, second.retry), (Some(1.0), None));
    // Thirty a second, two in hand: a third at the same moment is
    // refused, the old picture stays, and the layer is owed a frame when
    // the picture can be changed again.
    let third = drawn(&theme);
    assert_eq!(third.again, Some(2.0));
    assert!(close(third.retry, 1.0 / 30.0), "{:?}", third.retry);
    let canvas = theme.api().canvas(ImageId(0)).unwrap();
    assert_eq!((canvas.width, canvas.height, canvas.version), (4, 2, 2));
    now.set(100.0 + 1.1 / 30.0);
    assert_eq!(drawn(&theme).again, Some(1.0));
    // A steady thirty a second is never refused.
    for i in 1..=60 {
        now.set(200.0 + f64::from(i) / 30.0);
        assert_eq!(drawn(&theme).again, Some(1.0), "frame {i}");
    }
}

#[test]
fn ready_says_whether_a_take_would_be_taken_and_owes_the_redraw() {
    let d = dir("ready");
    d.wat("paint.wasm", PAINT);
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        local picture = paint:picture()
        local n = 0
        "#,
        r#"
        -- The contract's way: no work when it wouldn't be taken.
        if picture:ready() then
            n += 1
            paint:call("fill", 2, n, 0)
            assert(picture:take(paint, 0, 2, n))
        end
        draw:image(picture, 0, 0, frame.width, frame.height)
        return n
        "#,
    );
    let now = clock(&theme, 50.0);
    assert_eq!(drawn(&theme).again, Some(1.0));
    assert_eq!(drawn(&theme).again, Some(2.0));
    // Not ready: nothing taken, and still the layer is owed its frame.
    let skipped = drawn(&theme);
    assert_eq!(skipped.again, Some(2.0));
    assert!(close(skipped.retry, 1.0 / 30.0), "{:?}", skipped.retry);
    now.set(50.0 + 1.1 / 30.0);
    assert_eq!(drawn(&theme).again, Some(3.0));
    let canvas = theme.api().canvas(ImageId(0)).unwrap();
    assert_eq!((canvas.width, canvas.height), (2, 3));
}

#[test]
fn a_module_picture_is_any_shape_of_as_many_pixels_as_a_field() {
    // A field's 262 144 pixels, in whatever shape the window gives it:
    // Swirls' at 1920×1080 is 533×300, Aurora's light as wide as the
    // window and a few rows tall.
    let d = dir("picshape");
    d.wat(
        "paint.wasm",
        &PAINT.replace(
            "(memory (export \"memory\") 1)",
            "(memory (export \"memory\") 17)",
        ),
    );
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        local wide, square, swirls = paint:picture(), paint:picture(), paint:picture()
        "#,
        r#"
        paint:call("fill", 16384, 16, 0)
        assert(wide:take(paint, 0, 16384, 16))
        assert(wide.width == 16384 and wide.height == 16)
        paint:call("fill", 512, 512, 0)
        assert(square:take(paint, 0, 512, 512))
        paint:call("fill", 533, 300, 0)
        assert(swirls:take(paint, 0, 533, 300))
        -- One row more, or one column: over.
        assert(not swirls:take(paint, 0, 16384, 17))
        assert(not swirls:take(paint, 0, 16385, 16))
        assert(swirls.width == 533 and swirls.height == 300)
        "#,
    );
    drawn(&theme);
    assert_eq!(
        theme.api().pictures().decoded_bytes(),
        (16384 * 16 + 512 * 512 + 533 * 300) * 4
    );
    let all = notes(&theme);
    told(
        &all,
        "16384×17; a module's picture is at most 262144 pixels",
    );
    told(
        &all,
        "16385×16; a module's picture is at most 262144 pixels",
    );
    assert_eq!(all.len(), 2, "{all:?}");
}

#[test]
fn module_pictures_are_capped_in_number_size_and_the_image_total() {
    let d = dir("piccaps");
    // A PNG's header: 31.6 MiB of the 32 MiB total, counted.
    let png = |w: u32, h: u32| {
        let mut out = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        out.extend(w.to_be_bytes());
        out.extend(h.to_be_bytes());
        out.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        out
    };
    d.wat(
        "paint.wasm",
        &PAINT.replace(
            "(memory (export \"memory\") 1)",
            "(memory (export \"memory\") 32)",
        ),
    )
    .put("big1.png", &png(2048, 2048))
    .put("big2.png", &png(2048, 2000));
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        assert(gmnb.image("big1.png").width == 2048 and gmnb.image("big2.png").width == 2048)
        local pictures = {}
        for i = 1, 5 do pictures[i] = paint:picture() end
        local stage = 0
        "#,
        r#"
        stage += 1
        local p = pictures[1]
        if stage == 1 then
            -- 256 × 256 fit in the 400 KiB left.
            paint:call("fill", 256, 256, 0)
            assert(p:take(paint, 0, 256, 256))
            -- 512 × 512 don't: its size is charged as it is now.
            paint:call("fill", 512, 512, 0)
            assert(not p:take(paint, 0, 512, 512))
            assert(p.width == 256)
        elseif stage == 2 then
            -- Made smaller, it frees what it held: then 300 × 300 fit.
            paint:call("fill", 16, 16, 0)
            assert(p:take(paint, 0, 16, 16))
            assert(pictures[2]:take(paint, 0, 300, 300))
        end
        -- The fifth is one too many: no size, takes nothing, never ready.
        local fifth = pictures[5]
        assert(fifth.width == 0 and not fifth:take(paint, 0, 1, 1) and not fifth:ready())
        "#,
    );
    let now = clock(&theme, 10.0);
    drawn(&theme);
    now.set(20.0);
    drawn(&theme);
    let total = 2048 * 2048 * 4 + 2048 * 2000 * 4 + 16 * 16 * 4 + 300 * 300 * 4;
    assert_eq!(theme.api().pictures().decoded_bytes(), total);
    let all = notes(&theme);
    told(&all, "a theme's modules may fill 4 pictures; left out");
    told(
        &all,
        "picture:take: 512×512: the theme's pictures would be over 32 MiB decoded",
    );
}

/// `fill(n, count, out)`: `count` pixels at `out`, each (n, n, n, 255);
/// and 16 bytes at 0 whose colours are above their opacity in places:
/// (200, 100, 50, 128), (255, 255, 255, 0), (10, 20, 30, 255) and
/// (130, 129, 128, 128).
const GREYS: &str = r#"
(module
  (memory (export "memory") 8)
  (data (i32.const 0) "\c8\64\32\80\ff\ff\ff\00\0a\14\1e\ff\82\81\80\80")
  (func (export "fill") (param $n i32) (param $count i32) (param $out i32)
    (local $end i32)
    (local.set $end (i32.add (local.get $out) (i32.mul (local.get $count) (i32.const 4))))
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $out) (local.get $end)))
      (i32.store8 (local.get $out) (local.get $n))
      (i32.store8 offset=1 (local.get $out) (local.get $n))
      (i32.store8 offset=2 (local.get $out) (local.get $n))
      (i32.store8 offset=3 (local.get $out) (i32.const 255))
      (local.set $out (i32.add (local.get $out) (i32.const 4)))
      (br $l)))))
"#;

#[test]
fn a_module_picture_is_premultiplied_its_colours_clamped_as_it_is_taken() {
    let d = dir("premultiplied");
    d.wat("paint.wasm", GREYS);
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        local picture = paint:picture()
        local pixels = gmnb.pixels(2, 2)
        "#,
        r#"
        assert(picture:take(paint, 0, 2, 2))
        -- The same bytes again (clamped as before): nothing changes.
        assert(picture:take(paint, 0, 2, 2))
        draw:image(picture, 0, 0, 2, 2)
        -- A picture the theme fills from a buffer stays as written.
        assert(pixels:set(paint:read(0, 16)))
        draw:image(pixels, 2, 0, 2, 2)
        "#,
    );
    let d0 = drawn(&theme);
    let clamped = [
        128, 100, 50, 128, 0, 0, 0, 0, 10, 20, 30, 255, 128, 128, 128, 128,
    ];
    consistent(&theme);
    let canvas = theme.api().canvas(ImageId(0)).unwrap();
    assert!(canvas.premultiplied);
    assert_eq!(&canvas.pixels[..], &clamped);
    assert_eq!(canvas.version, 1);
    // What the draw holds is the picture as taken.
    let Op::Image {
        canvas: Some(held), ..
    } = &d0.list.ops[0]
    else {
        panic!("{:?}", d0.list.ops[0]);
    };
    assert_eq!(**held, canvas);
    let straight = theme.api().canvas(ImageId(1)).unwrap();
    assert!(!straight.premultiplied);
    assert_eq!(
        &straight.pixels[..],
        &[
            200, 100, 50, 128, 255, 255, 255, 0, 10, 20, 30, 255, 130, 129, 128, 128
        ]
    );
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

/// A theme whose option's tiles each take its one module picture three
/// times, something else each time, and draw it; and whose background
/// takes it once a frame and says whether it was taken (1) or not (2).
const TILES: &str = r#"
local paint = gmnb.module("paint.wasm")
local picture = paint:picture()
local n = 0
local SIDE = SIDE_HERE
local function fill(v)
    paint:call("fill", v, SIDE * SIDE, 1024)
    return picture:take(paint, 1024, SIDE, SIDE)
end
return {
    api = 1,
    name = "Tiles",
    options = { {
        id = "o",
        type = "choice",
        label = "O",
        choices = { { id = "a", label = "A" }, { id = "b", label = "B" } },
        preview = function(draw, frame, choice)
            for i = 1, TAKES_HERE do
                assert(picture:ready(), "a tile is never refused")
                n += 1
                fill(n % 256)
            end
            draw:image(picture, 0, 0, frame.width, frame.height)
        end,
    } },
    layers = { background = { draw = function(draw, frame)
        n += 1
        local taken = fill(n % 256)
        draw:image(picture, 0, 0, frame.width, frame.height)
        return if taken then 1 else 2
    end } },
}
"#;

fn tiles(d: &Dir, side: u32, takes: u32, limits: Limits) -> Theme {
    d.wat("paint.wasm", GREYS);
    let source = TILES
        .replace("SIDE_HERE", &side.to_string())
        .replace("TAKES_HERE", &takes.to_string());
    std::fs::write(d.0.join("theme.luau"), source).unwrap();
    let mut theme =
        Theme::load(Source::folder("t", &d.0), limits).unwrap_or_else(|e| panic!("{e}"));
    ready(&mut theme);
    theme
}

/// The image total the theme's pictures are counted as is what they hold,
/// worked out afresh (each module picture's versions, each once).
fn consistent(theme: &Theme) {
    let pictures = theme.api().pictures();
    assert_eq!(pictures.decoded_bytes(), pictures.decoded_from_scratch());
}

/// The version of a module picture a list's first op drew, and its size.
fn drew(list: &gmnb_theme::draw::DisplayList) -> (u64, u32, u32) {
    match &list.ops[0] {
        Op::Image {
            canvas: Some(c), ..
        } => (c.version, c.width, c.height),
        op => panic!("{op:?}"),
    }
}

/// The version of a module picture a list's first op drew, and its
/// first byte.
fn held(list: &gmnb_theme::draw::DisplayList) -> (u64, u8) {
    match &list.ops[0] {
        Op::Image {
            canvas: Some(c), ..
        } => (c.version, c.pixels[0]),
        op => panic!("{op:?}"),
    }
}

#[test]
fn a_draw_shows_a_picture_as_it_was_and_a_tile_has_no_allowance() {
    let d = dir("tiles");
    let theme = tiles(&d, 2, 3, limits());
    let _now = clock(&theme, 100.0);
    let option = theme.def.options[0].clone();
    // Each tile takes the picture three times at one moment, every take
    // taken (a frame of a layer has two in hand), and draws the last.
    let a = theme.preview(&option, "a", &Ctx::default()).unwrap();
    let b = theme.preview(&option, "b", &Ctx::default()).unwrap();
    consistent(&theme);
    // Each tile holds the picture as it drew it, though the picture has
    // changed since: the tiles in Settings stand as they were drawn.
    assert_eq!(held(&a), (3, 3));
    assert_eq!(held(&b), (6, 6));
    assert_eq!(theme.api().canvas(ImageId(0)).unwrap().version, 6);
    // The earlier version a tile still shows is counted (the versions
    // taken over within a hook, which nothing drew, are not).
    assert_eq!(theme.api().pictures().earlier_bytes(), (16, 1));
    // The tiles spent nothing of the allowance: at the same moment a
    // frame has its two in hand, and is refused the third.
    let frames: Vec<_> = (0..3).map(|_| drawn(&theme)).collect();
    let again: Vec<_> = frames.iter().map(|f| f.again).collect();
    assert_eq!(again, [Some(1.0), Some(1.0), Some(2.0)]);
    // A frame refused shows what stands, which the frame before took.
    assert_eq!(held(&frames[2].list), held(&frames[1].list));
    consistent(&theme);
    drop(a);
    drop(frames);
    // What the picture holds, each version once, is charged to the image
    // total: the frame's last take (which a layer that never took it
    // draws) and the tiles' last; no earlier one is drawn any more.
    assert_eq!(theme.api().pictures().decoded_bytes(), 2 * 16);
    assert_eq!(theme.api().pictures().earlier_bytes(), (0, 0));
    assert_eq!(held(&b), (6, 6));
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn earlier_versions_still_shown_count_against_the_image_total() {
    let d = dir("earlier");
    // A picture of 256 KiB, and 1 MiB for all of them.
    let mut lim = limits();
    lim.draw.image_bytes = 1 << 20;
    let theme = tiles(&d, 256, 1, lim);
    let option = theme.def.options[0].clone();
    let mut kept = Vec::new();
    for k in 1..=4u64 {
        kept.push(theme.preview(&option, "a", &Ctx::default()).unwrap());
        // Four fit: the one the tile took, and the three earlier ones the
        // tiles before hold (each let go as the next tile was begun).
        assert_eq!(drew(&kept[k as usize - 1]), (k, 256, 256), "tile {k}");
    }
    // The fifth would hold 1.25 MiB: refused, and the tile draws what a
    // layer that never took the picture draws (none has: no size).
    let fifth = theme.preview(&option, "a", &Ctx::default()).unwrap();
    assert_eq!(drew(&fifth), (0, 0, 0));
    told(
        &notes(&theme),
        "picture:take: 256×256: the theme's pictures, with the earlier ones still shown, would \
         be over 1 MiB decoded",
    );
    consistent(&theme);
    // Let go, they are counted no more.
    kept.clear();
    let taken = theme.preview(&option, "a", &Ctx::default()).unwrap();
    assert_eq!(drew(&taken), (5, 256, 256));
    consistent(&theme);
}

#[test]
fn at_most_so_many_earlier_versions_are_held() {
    let d = dir("held");
    let theme = tiles(&d, 1, 1, limits());
    let option = theme.def.options[0].clone();
    // Every tile kept: the version each took is an earlier one once the
    // next is begun (and counted as one to come till then).
    let mut kept = Vec::new();
    for k in 0..EARLIER_HELD {
        kept.push(theme.preview(&option, "a", &Ctx::default()).unwrap());
        assert!(
            theme.api().pictures().earlier_bytes().1 < EARLIER_HELD,
            "tile {k}"
        );
    }
    let last = EARLIER_HELD as u64;
    assert_eq!(held(&kept[EARLIER_HELD - 1]).0, last);
    assert_eq!(
        theme.api().pictures().earlier_bytes(),
        (4 * (last - 1), EARLIER_HELD - 1)
    );
    // The next tile: the last one's version is the EARLIER_HELD-th earlier
    // one, and this one's take is refused. It draws what a layer that
    // never took the picture draws (none has: no size).
    let refused = theme.preview(&option, "a", &Ctx::default()).unwrap();
    consistent(&theme);
    assert_eq!(
        theme.api().pictures().earlier_bytes(),
        (4 * last, EARLIER_HELD)
    );
    assert_eq!(drew(&refused), (0, 0, 0));
    told(
        &notes(&theme),
        &format!("{EARLIER_HELD} earlier versions of the theme's pictures are still shown"),
    );
    // Let go, they are counted no more: the next tile's is taken.
    drop(kept);
    drop(refused);
    let taken = theme.preview(&option, "a", &Ctx::default()).unwrap();
    assert_eq!(held(&taken).0, last + 1);
    assert_eq!(theme.api().pictures().earlier_bytes(), (0, 0));
    consistent(&theme);
}

/// A theme whose readouts each paint the one module picture the grey of
/// their own number (`fill`), and draw it: once a frame, skipping the
/// work when it wouldn't be taken (`ALWAYS`: taking all the same). Its
/// background draws it, taking nothing; its tile takes the number its
/// choice names, or ("none") draws it taking nothing. What a frame
/// returns: how many takes have been refused.
const PER_LAYER: &str = r#"
local paint = gmnb.module("paint.wasm")
local picture = paint:picture()
local refused = 0
local ALWAYS = ALWAYS_HERE
local function show(draw, n)
    if ALWAYS or picture:ready() then
        paint:call("fill", n % 256, 4, 1024)
        if not picture:take(paint, 1024, 2, 2) then refused += 1 end
    else
        refused += 1
    end
    draw:image(picture, 0, 0, 2, 2)
end
return {
    api = 1,
    name = "Per layer",
    options = { {
        id = "o",
        type = "choice",
        label = "O",
        choices = { { id = "77", label = "A" }, { id = "88", label = "B" }, { id = "none", label = "C" } },
        preview = function(draw, frame, choice)
            if choice == "none" then
                draw:image(picture, 0, 0, 2, 2)
            else
                show(draw, tonumber(choice))
            end
        end,
    } },
    layers = {
        display = { draw = function(draw, frame)
            show(draw, tonumber(frame.value.text))
            return refused
        end },
        background = { draw = function(draw, frame)
            draw:image(picture, 0, 0, 2, 2)
            return refused
        end },
    },
}
"#;

fn per_layer(d: &Dir, always: bool) -> Theme {
    d.wat("paint.wasm", GREYS);
    let source = PER_LAYER.replace("ALWAYS_HERE", if always { "true" } else { "false" });
    let mut theme = d.load(&source);
    ready(&mut theme);
    theme
}

/// Readout `readout`'s frame at `time`, showing `n`.
fn readout(theme: &Theme, readout: u64, n: u32, time: f64) -> Drawn {
    use std::sync::atomic::{AtomicU64, Ordering};
    static VERSION: AtomicU64 = AtomicU64::new(1);
    let glyphs = Glyphs {
        glyphs: vec![GlyphBox {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 20.0,
        }],
        size: 20.0,
        rect: [0.0, 0.0, 10.0, 20.0],
    };
    let text = n.to_string();
    let frame = Frame {
        time,
        value: Some(DisplayValue {
            readout,
            version: VERSION.fetch_add(1, Ordering::Relaxed),
            text: &text,
            glyphs: &glyphs,
            band: [0.0, 0.0, 100.0, 30.0],
            error: false,
            align_end: true,
            role: Role::Calculator,
            change: None,
            previous: None,
        }),
        ..Frame::sized(100.0, 30.0)
    };
    match theme.draw(LayerKind::Display, &Ctx::default(), &frame) {
        Ok(drawn) => drawn,
        Err(e) => panic!("{e:?}"),
    }
}

/// The grey a list's module picture shows (its first byte), and the
/// version.
fn grey(list: &gmnb_theme::draw::DisplayList) -> (u8, u64) {
    match &list.ops[0] {
        Op::Image {
            canvas: Some(c), ..
        } => (c.pixels[0], c.version),
        op => panic!("{op:?}"),
    }
}

#[test]
fn layers_taking_one_module_picture_each_have_their_own() {
    let d = dir("perlayer");
    let theme = per_layer(&d, false);
    let now = clock(&theme, 10.0);
    // Two readouts, one picture, each its own number, thirty frames a
    // second for three seconds, each frame something else: no take of
    // either is refused, and each draws its own.
    for i in 0..90u32 {
        now.set(10.0 + f64::from(i) / 30.0);
        let a = readout(&theme, 1, 2 * i, f64::from(i) / 30.0);
        let b = readout(&theme, 2, 2 * i + 101, f64::from(i) / 30.0);
        assert_eq!((a.again, b.again), (Some(0.0), Some(0.0)), "frame {i}");
        assert_eq!(grey(&a.list).0, (2 * i % 256) as u8, "frame {i}");
        assert_eq!(grey(&b.list).0, ((2 * i + 101) % 256) as u8, "frame {i}");
    }
    consistent(&theme);
    // A layer that has never taken it draws the version taken last.
    let bg = drawn(&theme);
    assert_eq!(grey(&bg.list).0, ((2 * 89 + 101) % 256) as u8);
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn a_layer_refused_draws_its_own_last_version() {
    let d = dir("refused");
    let theme = per_layer(&d, false);
    let _now = clock(&theme, 10.0);
    // At one moment: readout 1 takes twice (its two in hand), readout 2
    // takes something else, and readout 1, refused the third, still shows
    // the second of its own, not readout 2's.
    let first = readout(&theme, 1, 11, 0.0);
    let second = readout(&theme, 1, 12, 0.0);
    let other = readout(&theme, 2, 99, 0.0);
    let third = readout(&theme, 1, 13, 0.0);
    assert_eq!(grey(&first.list).0, 11);
    assert_eq!(grey(&second.list).0, 12);
    assert_eq!(grey(&other.list).0, 99);
    assert_eq!(third.again, Some(1.0), "the third is refused");
    assert_eq!(grey(&third.list), grey(&second.list));
    consistent(&theme);
    // And it is owed a frame for when it can be taken.
    assert!(close(third.retry, 1.0 / 30.0), "{:?}", third.retry);
    // Readout 2 had its own allowance: its two in hand, one spent.
    let again = readout(&theme, 2, 98, 0.0);
    assert_eq!(grey(&again.list).0, 98);
}

#[test]
fn a_tile_changes_nothing_a_window_layer_draws_or_spends() {
    let d = dir("tilefree");
    let theme = per_layer(&d, true);
    let _now = clock(&theme, 10.0);
    let option = theme.def.options[0].clone();
    // Readout 1 takes 40, and then 41: its two in hand spent.
    readout(&theme, 1, 40, 0.0);
    let before = readout(&theme, 1, 41, 0.0);
    // A tile takes 77, and the other 88.
    let a = theme.preview(&option, "77", &Ctx::default()).unwrap();
    let b = theme.preview(&option, "88", &Ctx::default()).unwrap();
    assert_eq!((grey(&a).0, grey(&b).0), (77, 88));
    // The readout drawn again with its own bytes: taken, free (it has no
    // allowance left to spend), and the same version it drew.
    let after = readout(&theme, 1, 41, 0.0);
    assert_eq!(
        after.again,
        Some(0.0),
        "taken, though its allowance is spent"
    );
    assert_eq!(grey(&after.list), grey(&before.list));
    consistent(&theme);
    // A layer that has never taken it draws the readout's, not a tile's.
    assert_eq!(grey(&drawn(&theme).list).0, 41);
    // The tiles still show theirs.
    assert_eq!((grey(&a).0, grey(&b).0), (77, 88));
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn a_tiles_take_lasts_for_that_tile_only() {
    let d = dir("tileonly");
    let theme = per_layer(&d, true);
    let _now = clock(&theme, 10.0);
    let option = theme.def.options[0].clone();
    let p = |theme: &Theme| {
        consistent(theme);
        let pictures = theme.api().pictures();
        (pictures.decoded_bytes(), pictures.earlier_bytes())
    };
    // Tile 1 takes 77 and draws it; tile 2 draws it taking nothing: what a
    // layer that never took the picture draws (none has: no size), not
    // tile 1's.
    let one = theme.preview(&option, "77", &Ctx::default()).unwrap();
    assert_eq!(p(&theme), (16, (0, 0)));
    let two = theme.preview(&option, "none", &Ctx::default()).unwrap();
    assert_eq!((grey(&one).0, drew(&two)), (77, (0, 0, 0)));
    // Tile 1's version, which its draw still holds, is an earlier one.
    assert_eq!(p(&theme), (0, (16, 1)));
    // A readout takes 40; tile 3 takes 88, and tile 4 draws taking
    // nothing: the readout's.
    let readout_40 = readout(&theme, 1, 40, 0.0);
    let three = theme.preview(&option, "88", &Ctx::default()).unwrap();
    let four = theme.preview(&option, "none", &Ctx::default()).unwrap();
    assert_eq!((grey(&three).0, grey(&four).0), (88, 40));
    // Earlier now: tile 1's and tile 3's, and the empty one tile 2 drew.
    assert_eq!(p(&theme), (16, (32, 3)));
    // Each tile still shows what it drew; let go, nothing earlier is
    // counted, and the picture holds the readout's alone.
    assert_eq!((grey(&one).0, grey(&readout_40.list).0), (77, 40));
    drop((one, two, three, four));
    assert_eq!(p(&theme), (16, (0, 0)));
    assert_eq!(grey(&drawn(&theme).list).0, 40);
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn past_so_many_layers_the_one_used_longest_ago_is_let_go() {
    let d = dir("evicted");
    let theme = per_layer(&d, false);
    let _now = clock(&theme, 10.0);
    let p = |theme: &Theme| {
        consistent(theme);
        let pictures = theme.api().pictures();
        (pictures.decoded_bytes(), pictures.earlier_bytes())
    };
    // Readout 1 takes twice at one moment (its two in hand spent): its
    // first version, still drawn, is an earlier one.
    let mut kept = vec![readout(&theme, 1, 1, 0.0), readout(&theme, 1, 2, 0.0)];
    assert_eq!(p(&theme), (16, (16, 1)));
    // Readouts 2 to TAKERS take one each, every draw kept: the picture
    // holds TAKERS versions, its fallback the last.
    for r in 2..=TAKERS as u64 {
        kept.push(readout(&theme, r, 100 + r as u32, 0.0));
    }
    assert_eq!(p(&theme), (16 * TAKERS as u64, (16, 1)));
    // One more: readout 1, used longest ago, is let go; its version, still
    // drawn, is an earlier one now.
    let more = TAKERS as u64 + 1;
    let last = readout(&theme, more, 200, 0.0);
    assert_eq!((last.again, grey(&last.list).0), (Some(0.0), 200));
    assert_eq!(p(&theme), (16 * TAKERS as u64, (32, 2)));
    // Readout 1 takes again at the same moment, its allowance whole (it
    // had spent both), and readout 2 goes in its turn.
    let back = readout(&theme, 1, 3, 0.0);
    assert_eq!((back.again, grey(&back.list).0), (Some(0.0), 3));
    assert_eq!(p(&theme), (16 * TAKERS as u64, (48, 3)));
    // The draws let go, nothing earlier is counted, and every layer kept
    // still draws its own.
    kept.clear();
    assert_eq!(p(&theme), (16 * TAKERS as u64, (0, 0)));
    let three = readout(&theme, 3, 103, 0.0);
    assert_eq!((three.again, grey(&three.list).0), (Some(0.0), 103));
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}

#[test]
fn a_new_layer_taking_what_it_shows_is_refused_past_so_many_earlier_versions() {
    let d = dir("pinfull");
    let theme = per_layer(&d, false);
    let _now = clock(&theme, 10.0);
    // TAKERS readouts and EARLIER_HELD more, each something else, every
    // draw kept: past TAKERS each lets the one used longest ago go, its
    // version an earlier one.
    let all = TAKERS + EARLIER_HELD;
    let kept: Vec<_> = (1..=all as u64)
        .map(|r| readout(&theme, r, (r % 200) as u32 + 1, 0.0))
        .collect();
    assert!(kept.iter().all(|d| d.again == Some(0.0)));
    consistent(&theme);
    assert_eq!(theme.api().pictures().earlier_bytes().1, EARLIER_HELD);
    // One more readout, taking the very bytes the picture's fallback is
    // (the last readout's): it would hold that version as its own, letting
    // another go whose version is still drawn: refused, a note and a frame
    // owed; it draws the fallback all the same.
    let same = (all as u64 % 200) as u32 + 1;
    let more = readout(&theme, all as u64 + 1, same, 0.0);
    assert_eq!(more.again, Some(1.0));
    assert!(close(more.retry, 1.0 / 30.0), "{:?}", more.retry);
    assert_eq!(grey(&more.list).0, same as u8);
    assert_eq!(theme.api().pictures().earlier_bytes().1, EARLIER_HELD);
    consistent(&theme);
    told(
        &notes(&theme),
        &format!("{EARLIER_HELD} earlier versions of the theme's pictures are still shown"),
    );
    // Let go, it is taken.
    drop(kept);
    let again = readout(&theme, all as u64 + 1, same, 0.0);
    assert_eq!(again.again, Some(1.0), "no take refused since");
    assert_eq!(theme.api().pictures().earlier_bytes(), (0, 0));
    consistent(&theme);
}

#[test]
fn a_pixels_set_called_on_a_module_picture_changes_nothing() {
    let d = dir("setmodule");
    d.wat("paint.wasm", GREYS);
    // The background takes the module's picture each frame, something else
    // each time; and calls a pixels picture's `set` on it (`set` reads the
    // picture off the table it is called on), with as many bytes as it
    // holds, which a module's picture never takes. (The pixels picture of
    // one pixel: 4 bytes of the image total.)
    let theme = d.background(
        r#"
        local paint = gmnb.module("paint.wasm")
        local picture = paint:picture()
        local pixels = gmnb.pixels(1, 1)
        local n = 0
        "#,
        r#"
        n += 1
        paint:call("fill", n, 4, 1024)
        assert(picture:take(paint, 1024, 2, 2))
        local set = pixels.set(picture, buffer.create(16))
        draw:image(picture, 0, 0, 2, 2)
        return if set then 1 else 0
        "#,
    );
    let now = clock(&theme, 10.0);
    let first = drawn(&theme);
    // Refused, and nothing owed: no frame would take it.
    assert_eq!((first.again, first.retry), (Some(0.0), None));
    assert_eq!(grey(&first.list).0, 1);
    consistent(&theme);
    assert_eq!(theme.api().pictures().decoded_bytes(), 4 + 16);
    // The layer's next take: the picture as the layer took it, counted
    // once.
    now.set(11.0);
    let second = drawn(&theme);
    assert_eq!((second.again, second.retry), (Some(0.0), None));
    assert_eq!(grey(&second.list).0, 2);
    consistent(&theme);
    assert_eq!(theme.api().pictures().decoded_bytes(), 4 + 16);
    // Said once, though called each frame.
    let all = notes(&theme);
    let said = "gmnb.pixels: set was called on a module's picture, which changes only by \
                picture:take";
    assert_eq!(
        all.iter().filter(|n| n.contains(said)).count(),
        1,
        "{all:?}"
    );
    assert_eq!(all.len(), 1, "{all:?}");
}

#[test]
fn a_pixels_picture_refused_for_earlier_versions_is_a_note_and_a_frame_owed() {
    let d = dir("pixelsfull");
    // 1 MiB in all; a picture of 256 KiB.
    let mut lim = limits();
    lim.draw.image_bytes = 1 << 20;
    std::fs::write(
        d.0.join("theme.luau"),
        r#"
        local canvas = gmnb.pixels(256, 256)
        local buf = buffer.create(256 * 256 * 4)
        local n = 0
        return { api = 1, name = "T", layers = { background = { draw = function(draw, frame)
            n += 1
            buffer.fill(buf, 0, n % 256)
            local taken = canvas:set(buf)
            draw:image(canvas, 0, 0, 256, 256)
            return if taken then 1 else 2
        end } } }
        "#,
    )
    .unwrap();
    let theme = Theme::load(Source::folder("t", &d.0), lim).unwrap_or_else(|e| panic!("{e}"));
    let now = clock(&theme, 10.0);
    // Each frame kept: the picture standing and the earlier versions they
    // hold come to 1 MiB at the fourth; the fifth set is refused.
    let mut kept = Vec::new();
    for i in 0..4 {
        now.set(10.0 + f64::from(i));
        let d = drawn(&theme);
        assert_eq!(d.again, Some(1.0), "frame {i}");
        kept.push(d);
    }
    now.set(20.0);
    let refused = drawn(&theme);
    consistent(&theme);
    // Not an error, nor a strike: a note, and the layer owed a frame.
    assert_eq!(refused.again, Some(2.0));
    assert!(close(refused.retry, 1.0 / 30.0), "{:?}", refused.retry);
    assert_eq!(strikes(&theme), (0, false));
    told(
        &notes(&theme),
        "with the earlier ones still shown, would be over 1 MiB decoded",
    );
    // The frames let go, it is taken.
    kept.clear();
    drop(refused);
    now.set(21.0);
    assert_eq!(drawn(&theme).again, Some(1.0));
}

// ---- When modules are ready -----------------------------------------------------------

#[test]
fn a_hook_never_runs_before_its_modules_are_ready() {
    let d = dir("held");
    d.wat("paint.wasm", PAINT);
    let mut theme = d.load(
        r#"
        local paint = gmnb.module("paint.wasm")
        return {
            api = 1, name = "T",
            style = function() print("style ran") return {} end,
            layers = { background = { draw = function()
                print("draw ran")
                return paint:call("bump")
            end } },
        }
        "#,
    );
    assert!(!theme.modules_ready());
    assert_eq!(theme.modules(), ["paint.wasm"]);
    // Held: no hook runs, and none is struck for it.
    for _ in 0..5 {
        assert_eq!(
            failed(&theme),
            Failure::Failed(ThemeError::Unsupported(
                "the theme's modules are still being compiled".into()
            ))
        );
        assert!(theme.style(&Ctx::default()).is_err());
    }
    assert_eq!(strikes(&theme), (0, false));
    assert!(theme.take_log().is_empty());
    let job = theme.prepare_modules().expect("a module to compile");
    assert!(job.bytes() > 0);
    // Still held while it compiles, prepared again or not.
    assert!(theme.prepare_modules().is_none());
    assert!(!theme.modules_ready());
    assert!(matches!(failed(&theme), Failure::Failed(_)));
    let done = job.run();
    theme.install_modules(done);
    assert!(theme.modules_ready());
    assert_eq!(drawn(&theme).again, Some(1.0));
    assert!(theme.style(&Ctx::default()).is_ok());
    assert_eq!(theme.take_log(), ["draw ran", "style ran"]);
    assert_eq!(strikes(&theme), (0, false));
}

#[test]
fn modules_are_named_at_the_top_and_used_only_in_hooks() {
    let d = dir("top");
    d.wat("paint.wasm", PAINT);
    // Used at the top of the file: an error, the theme doesn't load.
    for top in [
        r#"gmnb.module("paint.wasm"):call("bump")"#,
        r#"local m = gmnb.module("paint.wasm") m:write(0, buffer.create(1))"#,
        r#"local m = gmnb.module("paint.wasm") m:read(0, 1)"#,
        r#"local m = gmnb.module("paint.wasm") m:picture():take(m, 0, 1, 1)"#,
    ] {
        std::fs::write(
            d.0.join("theme.luau"),
            format!("{top}\nreturn {{ api = 1, name = 'T' }}"),
        )
        .unwrap();
        let e = Theme::load(Source::folder("t", &d.0), limits())
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("paint.wasm can't be used at the top of the file: only in a hook"),
            "{top}: {e}"
        );
    }
    // Named first in a hook: never compiled, its calls do nothing.
    let theme = d.background(
        "",
        r#"
        local late = gmnb.module("paint.wasm")
        assert(select('#', late:call("bump")) == 0)
        "#,
    );
    assert!(theme.modules().is_empty());
    drawn(&theme);
    told(
        &notes(&theme),
        "paint.wasm was first named in a hook; a theme names its modules at the top of its file",
    );
}

#[test]
fn a_module_compiled_once_isnt_compiled_again() {
    let d = dir("cache");
    // Bytes no other test has.
    d.wat(
        "own.wasm",
        &PAINT.replace("(i32.const 12345)", "(i32.const 54321)"),
    );
    let source = r#"
        local m = gmnb.module("own.wasm")
        return { api = 1, name = "T", layers = { background = { draw = function()
            return m:call("bump")
        end } } }
    "#;
    let mut first = d.load(source);
    ready(&mut first);
    // Previewed, then applied (or reloaded): the same bytes are ready at
    // once, from memory, with a fresh instance of their own.
    let mut again = d.load(source);
    assert!(again.prepare_modules().is_none());
    assert!(again.modules_ready());
    assert_eq!(drawn(&first).again, Some(1.0));
    assert_eq!(drawn(&again).again, Some(1.0));
    assert_eq!(drawn(&first).again, Some(2.0));
}

// ---- Hostile modules on the compile thread ------------------------------------------------

/// A module of one function `() -> ()` whose body is `body` (no locals),
/// written byte by byte: shapes text can't easily say.
fn one_function(body: &[u8]) -> Vec<u8> {
    fn leb(mut n: usize, out: &mut Vec<u8>) {
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }
    fn section(id: u8, content: &[u8], out: &mut Vec<u8>) {
        out.push(id);
        leb(content.len(), out);
        out.extend_from_slice(content);
    }
    let mut out = b"\0asm\x01\0\0\0".to_vec();
    section(1, &[1, 0x60, 0, 0], &mut out);
    section(3, &[1, 0], &mut out);
    section(7, &[1, 1, b'f', 0, 0], &mut out);
    let mut func = vec![0];
    func.extend_from_slice(body);
    func.push(0x0b);
    let mut code = vec![1];
    leb(func.len(), &mut code);
    code.extend_from_slice(&func);
    section(10, &code, &mut out);
    out
}

#[test]
fn hostile_shapes_compile_or_are_refused_on_the_thread_without_harm() {
    let d = dir("shapes");
    // 60 000 blocks inside one another (as many as the cap holds).
    let mut nested = Vec::new();
    for _ in 0..60_000 {
        nested.extend([0x02, 0x40]);
    }
    nested.extend(std::iter::repeat_n(0x0b, 60_000));
    // One function as big as the cap allows: `i32.const 1; drop` over and
    // over.
    let mut long = Vec::new();
    while long.len() < (250 << 10) {
        long.extend([0x41, 0x01, 0x1a]);
    }
    // Nesting that never closes.
    let open: Vec<u8> = std::iter::repeat_n([0x02, 0x40], 80_000)
        .flatten()
        .collect();
    d.put("nested.wasm", &one_function(&nested))
        .put("long.wasm", &one_function(&long))
        .put("open.wasm", &one_function(&open));
    for file in ["nested.wasm", "long.wasm", "open.wasm"] {
        let mut theme = d.load(&format!(
            r#"
            local m = gmnb.module("{file}")
            return {{ api = 1, name = "T", layers = {{ background = {{ draw = function()
                m:call("f")
            end }} }} }}
            "#
        ));
        let t = Instant::now();
        ready(&mut theme);
        let took = t.elapsed();
        assert!(took < Duration::from_secs(30), "{file}: {took:?}");
        // Whatever came of it, the theme runs: a call does what the
        // module does, or nothing, with a note.
        let _ = run(&theme, &Frame::sized(10.0, 10.0));
        eprintln!("{file}: {took:?}, {:?}", notes(&theme));
    }
}

// ---- The test theme that uses the API -----------------------------------------------------

/// `tests/themes/modules`' module: `tint(r, g, b)`, `fill(w, h, out)` in
/// that colour, `bump()`.
const TINTED: &str = r#"
(module
  (memory (export "memory") 17)
  (global $r (mut i32) (i32.const 255))
  (global $g (mut i32) (i32.const 255))
  (global $b (mut i32) (i32.const 255))
  (func (export "tint") (param i32 i32 i32)
    (global.set $r (local.get 0)) (global.set $g (local.get 1)) (global.set $b (local.get 2)))
  (func (export "fill") (param $w i32) (param $h i32) (param $out i32) (result i32)
    (local $p i32) (local $end i32)
    (local.set $p (local.get $out))
    (local.set $end
      (i32.add (local.get $out) (i32.mul (i32.mul (local.get $w) (local.get $h)) (i32.const 4))))
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $p) (local.get $end)))
      (i32.store8 (local.get $p) (global.get $r))
      (i32.store8 offset=1 (local.get $p) (global.get $g))
      (i32.store8 offset=2 (local.get $p) (global.get $b))
      (i32.store8 offset=3 (local.get $p) (i32.const 255))
      (local.set $p (i32.add (local.get $p) (i32.const 4)))
      (br $l)))
    (i32.sub (local.get $end) (local.get $out)))
  (func (export "bump") (result i32)
    (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))
    (i32.load (i32.const 0))))
"#;

#[test]
fn the_test_theme_that_uses_the_api_draws_and_previews() {
    let d = dir("theme");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/themes/modules/theme.luau");
    d.wat("paint.wasm", TINTED);
    let mut theme = d.load(&std::fs::read_to_string(source).unwrap());
    ready(&mut theme);
    let frame = Frame::sized(760.0, 640.0);
    let drawn = run(&theme, &frame).unwrap();
    assert!(matches!(
        drawn.list.ops[0],
        Op::Image {
            image: ImageId(0),
            ..
        }
    ));
    let canvas = theme.api().canvas(ImageId(0)).unwrap();
    assert_eq!((canvas.width, canvas.height), (95, 80));
    assert!(canvas.pixels.chunks(4).all(|p| p == [255, 255, 255, 255]));
    let option = theme.def.options[0].clone();
    for (choice, rgb) in [("warm", [255, 128, 0]), ("cool", [0, 128, 255])] {
        let list = theme.preview(&option, choice, &Ctx::default()).unwrap();
        assert!(matches!(
            list.ops[0],
            Op::Image {
                image: ImageId(1),
                ..
            }
        ));
        let tile = theme.api().canvas(ImageId(1)).unwrap();
        assert_eq!((tile.width, tile.height), (16, 16));
        assert_eq!(&tile.pixels[..4], &[rgb[0], rgb[1], rgb[2], 255]);
    }
    assert!(notes(&theme).is_empty(), "{:?}", notes(&theme));
}
