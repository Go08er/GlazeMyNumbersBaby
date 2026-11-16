//! Swirls' module (`apps/gmnb/themes/swirls/paint.wasm`, built from
//! `modules/field`) against the pass it replaces, `gmnb_media::field`, the
//! one `draw:field` and `gmnb.ramp` run: for the same snapshots, glide,
//! ramp, `scale`, `offset` and size, the same picture, byte for byte, after
//! what `picture:take` does to it (each colour clamped to its opacity). Run
//! under wasmtime as the app sets it up (relaxed SIMD off, NaNs canonical).

use gmnb_media::field::{self, Field, MAX_PIXELS, Ramp};

const PAINT: &[u8] = include_bytes!("../../../apps/gmnb/themes/swirls/paint.wasm");

struct Paint {
    store: wasmtime::Store<()>,
    memory: wasmtime::Memory,
    at: wasmtime::TypedFunc<(u32, u32), u32>,
    ramp: wasmtime::TypedFunc<(u32, u32, f32, f32, u32), u32>,
    made: wasmtime::TypedFunc<u32, u32>,
    size: wasmtime::TypedFunc<(f32, f32, f32), f64>,
    #[allow(clippy::type_complexity)]
    render: wasmtime::TypedFunc<(u32, u32, u32, u32, u32, f32, f32, f32, u32, u32), u32>,
}

impl Paint {
    fn new() -> Paint {
        let mut config = wasmtime::Config::new();
        config
            .wasm_simd(true)
            .wasm_relaxed_simd(false)
            .cranelift_nan_canonicalization(true);
        let engine = wasmtime::Engine::new(&config).unwrap();
        let module = wasmtime::Module::new(&engine, PAINT).unwrap();
        assert_eq!(
            module.imports().count(),
            0,
            "a theme's module imports nothing"
        );
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        Paint {
            at: instance.get_typed_func(&mut store, "at").unwrap(),
            ramp: instance.get_typed_func(&mut store, "ramp").unwrap(),
            made: instance.get_typed_func(&mut store, "made").unwrap(),
            size: instance.get_typed_func(&mut store, "size").unwrap(),
            render: instance.get_typed_func(&mut store, "render").unwrap(),
            memory,
            store,
        }
    }

    fn at(&mut self, slot: u32, part: u32) -> usize {
        self.at.call(&mut self.store, (slot, part)).unwrap() as usize
    }

    fn write(&mut self, at: usize, floats: &[f32]) {
        let bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.memory.write(&mut self.store, at, &bytes).unwrap();
    }

    /// The slot's ramp, as `gmnb.ramp` makes it of these colours.
    fn set_ramp(&mut self, slot: u32, colors: &[[f32; 4]], edge: [f32; 2], cycle: bool) -> u32 {
        let at = self.at(slot, 2);
        self.write(at, colors.as_flattened());
        self.ramp
            .call(
                &mut self.store,
                (slot, colors.len() as u32, edge[0], edge[1], cycle as u32),
            )
            .unwrap()
    }

    fn size(&mut self, w: f32, h: f32, quality: f32) -> (u32, u32) {
        let n = self.size.call(&mut self.store, (w, h, quality)).unwrap();
        let w = (n / 1_048_576.0).floor();
        (w as u32, (n - w * 1_048_576.0) as u32)
    }

    /// The slot's picture, as `picture:take` takes it.
    #[allow(clippy::too_many_arguments)]
    fn render(&mut self, slot: u32, f: &Case, width: u32, height: u32) -> Result<Vec<u8>, u32> {
        let (a, b) = (self.at(slot, 0), self.at(slot, 1));
        self.write(a, &f.data);
        if let Some(next) = &f.next {
            self.write(b, next);
        }
        let done = self
            .render
            .call(
                &mut self.store,
                (
                    slot,
                    f.cols as u32,
                    f.rows as u32,
                    f.channels as u32,
                    f.next.is_some() as u32,
                    f.mix,
                    f.scale,
                    f.offset,
                    width,
                    height,
                ),
            )
            .unwrap();
        if done != 1 {
            return Err(done);
        }
        let at = self.at(slot, 3);
        let mut out = vec![0; (width * height * 4) as usize];
        self.memory.read(&self.store, at, &mut out).unwrap();
        Ok(out)
    }
}

/// `picture:take`'s: each colour clamped to its pixel's opacity.
fn taken(mut pixels: Vec<u8>) -> Vec<u8> {
    for p in pixels.as_chunks_mut::<4>().0 {
        for c in 0..3 {
            p[c] = p[c].min(p[3]);
        }
    }
    pixels
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A field to colour: what `draw:field` is given.
#[derive(Clone)]
struct Case {
    cols: usize,
    rows: usize,
    channels: usize,
    data: Vec<f32>,
    next: Option<Vec<f32>>,
    mix: f32,
    scale: f32,
    offset: f32,
}

impl Case {
    fn random(rng: &mut Rng, cols: usize, rows: usize, channels: usize) -> Case {
        let n = cols * rows * channels;
        // Smooth, as a theme's are, with now and then a number that isn't
        // finite (which counts as 0).
        let (fx, fy, phase) = (
            rng.range(0.02, 0.4),
            rng.range(0.02, 0.4),
            rng.range(0.0, 6.0),
        );
        let wave = |rng: &mut Rng| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let p = i / channels;
                    let (x, y) = ((p % cols) as f32, (p / cols) as f32);
                    match rng.below(4000) {
                        0 => f32::NAN,
                        1 => f32::INFINITY,
                        _ => {
                            (x * fx + phase).sin() * 2.0
                                + (y * fy).cos() * 1.5
                                + rng.range(-0.1, 0.1)
                        }
                    }
                })
                .collect()
        };
        let data = wave(rng);
        let next = (rng.below(4) != 0).then(|| wave(rng));
        let mix = match rng.below(6) {
            0 => 0.0,
            1 => 1.0,
            2 => 1.7,
            _ => rng.unit(),
        };
        Case {
            cols,
            rows,
            channels,
            data,
            next,
            mix,
            scale: if rng.below(3) == 0 {
                1.25
            } else {
                rng.range(0.05, 3.0)
            },
            offset: if rng.below(2) == 0 {
                0.0
            } else {
                rng.range(-3.0, 3.0)
            },
        }
    }

    /// What `draw:field` makes of it.
    fn native(&self, ramp: &Ramp, width: u32, height: u32) -> Vec<u8> {
        let bytes = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let next = self.next.as_deref().map(bytes);
        let values = field::values(
            &bytes(&self.data),
            next.as_deref(),
            self.mix.clamp(0.0, 1.0),
        );
        let grid = Field {
            data: &values,
            cols: self.cols,
            rows: self.rows,
            channels: self.channels,
        };
        field::render(
            &grid,
            ramp,
            self.scale,
            self.offset,
            width as usize,
            height as usize,
        )
        .unwrap()
    }
}

fn random_ramp(rng: &mut Rng, alpha: bool) -> (Vec<[f32; 4]>, [f32; 2], bool) {
    let n = 2 + rng.below(15);
    let colors = (0..n)
        .map(|_| {
            let a = if alpha { rng.unit() } else { 1.0 };
            [rng.unit(), rng.unit(), rng.unit(), a]
        })
        .collect();
    let (e0, e1) = match rng.below(5) {
        0 => (0.5, 0.5),
        1 => (0.0, 1.0),
        2 => (0.08, 0.92),
        _ => {
            let (a, b) = (rng.unit(), rng.unit());
            (a.min(b), a.max(b))
        }
    };
    (colors, [e0, e1], rng.below(3) != 0)
}

/// The colours of Swirls' paint: its own set (gold, cyan, near-black),
/// whole, and thinned as on a see-through window.
fn swirls_ramps() -> Vec<(Vec<[f32; 4]>, [f32; 2], bool)> {
    let hex = |h: u32, a: f32| {
        let c = |s: u32| ((h >> s) & 0xff) as f32 / 255.0;
        [c(16), c(8), c(0), a]
    };
    let mut out = Vec::new();
    for (bright, dark) in [(1.0, 1.0), (0.83, 0.62), (0.55, 0.0), (0.4, 0.0)] {
        out.push((
            vec![
                hex(0xffcf29, bright),
                hex(0x0ed6f0, bright),
                hex(0x010609, dark),
            ],
            [0.08, 0.92],
            true,
        ));
    }
    out
}

#[test]
fn the_modules_size_is_the_fields() {
    let mut paint = Paint::new();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut cases = vec![
        (760.0, 640.0, 0.5),
        (1600.0, 1000.0, 0.31623),
        (1920.0, 1080.0, 0.27778),
        (2400.0, 1000.0, 0.25820),
        (3840.0, 2160.0, 0.1),
        (96.0, 56.0, 1.0),
        (428.0, 68.0, 0.5),
        (1.0, 1.0, 0.1),
        (0.0, 5.0, 1.0),
        (1.0e7, 3.0, 1.0),
        (1.0e12, 1.0e12, 1.0),
        (2000.0, 2000.0, 1.0),
        (512.0, 512.0, 1.0),
        (37.5, 12.25, 0.05),
        (37.5, 12.25, 3.0),
    ];
    for _ in 0..2000 {
        let side = |rng: &mut Rng| match rng.below(4) {
            0 => rng.range(0.0, 20.0),
            1 => rng.range(0.0, 5000.0),
            2 => (rng.below(4000) as f32) + 0.5,
            _ => rng.range(0.0, 1.0e6),
        };
        let (w, h) = (side(&mut rng), side(&mut rng));
        cases.push((w, h, rng.range(0.0, 1.2)));
    }
    for (w, h, q) in cases {
        assert_eq!(
            paint.size(w, h, q),
            field::output_size(w, h, q.clamp(0.1, 1.0), MAX_PIXELS),
            "{w} x {h} at {q}"
        );
    }
}

#[test]
fn the_modules_pictures_are_the_fields_byte_for_byte() {
    let mut paint = Paint::new();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    // The sizes Swirls draws (the tile's 17 × 11 points and 96 × 56
    // pixels; its background at 760 × 640, 1600 × 1000 and 1920 × 1080),
    // the most a field makes in the squarest and widest shapes, and others.
    let mut sizes: Vec<(usize, usize, usize, u32, u32)> = vec![
        (17, 11, 2, 96, 56),
        (17, 11, 2, 17, 11),
        (65, 55, 2, 380, 320),
        (77, 49, 2, 506, 316),
        (82, 47, 2, 533, 300),
        (54, 10, 1, 214, 34),
        (128, 128, 2, 512, 512),
        (256, 64, 1, 16384, 16),
        (2, 2, 1, 1, 1),
        (256, 2, 2, 3, 700),
    ];
    for _ in 0..60 {
        let cols = 2 + rng.below(255);
        let rows = (2 + rng.below(255)).min(16_384 / cols).max(2);
        let width = 1 + rng.below(700) as u32;
        let height = (1 + rng.below(700) as u32).min(262_144 / width);
        sizes.push((cols, rows, 1 + rng.below(2), width, height));
    }
    let mut ramps = swirls_ramps();
    for i in 0..20 {
        ramps.push(random_ramp(&mut rng, i % 2 == 0));
    }
    let (mut pixels, mut clamped) = (0usize, 0usize);
    for (n, (cols, rows, channels, width, height)) in sizes.into_iter().enumerate() {
        let (colors, edge, cycle) = &ramps[n % ramps.len()];
        let native_ramp = Ramp::new(colors, *edge, *cycle).unwrap();
        let slot = (n % 8) as u32;
        assert_eq!(paint.set_ramp(slot, colors, *edge, *cycle), 1);
        let case = Case::random(&mut rng, cols, rows, channels);
        let made = paint.render(slot, &case, width, height).unwrap();
        let want = case.native(&native_ramp, width, height);
        // (How often the take's clamp changes anything: never, for a
        // ramp's colours.)
        clamped += made
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3].iter().any(|c| *c > p[3]))
            .count();
        let made = taken(made);
        let want = taken(want);
        let differ = made
            .as_chunks::<4>()
            .0
            .iter()
            .zip(want.as_chunks::<4>().0)
            .position(|(a, b)| a != b);
        assert!(
            differ.is_none(),
            "{cols} x {rows} x {channels} -> {width} x {height}, ramp {n}: first differs at pixel {:?}",
            differ
        );
        pixels += (width * height) as usize;
    }
    assert_eq!(clamped, 0);
    println!("{pixels} pixels, the same");
}

#[test]
fn every_colour_of_swirls_ramps_is_the_tables() {
    // A one-channel field that sweeps the whole ramp, a pixel an entry:
    // every entry of the table, through the module and natively.
    let mut paint = Paint::new();
    for (colors, edge, cycle) in swirls_ramps() {
        let bands = if cycle {
            colors.len()
        } else {
            colors.len() - 1
        };
        let entries = (bands * 256) as u32;
        // Position (k + 0.5) / 256 at pixel k: the values at the field's two
        // ends, interpolated across the picture's width.
        let span = entries as f32 / 256.0;
        let case = Case {
            cols: 2,
            rows: 2,
            channels: 1,
            data: vec![0.0, span, 0.0, span],
            next: None,
            mix: 0.0,
            scale: 1.0,
            offset: 0.0,
        };
        let native_ramp = Ramp::new(&colors, edge, cycle).unwrap();
        assert_eq!(paint.set_ramp(0, &colors, edge, cycle), 1);
        let made = paint.render(0, &case, entries, 1).unwrap();
        assert_eq!(made, case.native(&native_ramp, entries, 1));
        let table: Vec<[u8; 4]> = (0..entries)
            .map(|k| native_ramp.at((k as f32 + 0.5) / 256.0))
            .collect();
        assert_eq!(made.as_chunks::<4>().0, &table[..]);
        assert!(
            made.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[..3].iter().all(|c| *c <= p[3]))
        );
    }
}

#[test]
fn what_the_module_refuses_and_a_fresh_one_has_no_ramp() {
    let mut paint = Paint::new();
    let case = Case {
        cols: 2,
        rows: 2,
        channels: 1,
        data: vec![0.0; 4],
        next: None,
        mix: 0.0,
        scale: 1.0,
        offset: 0.0,
    };
    // Fresh: no slot has a ramp (how a theme finds the module started
    // afresh after a timeout).
    let made = |paint: &mut Paint, slot: u32| -> u32 {
        let f = paint.made.clone();
        f.call(&mut paint.store, slot).unwrap()
    };
    for slot in 0..8 {
        assert_eq!(paint.render(slot, &case, 4, 4), Err(0));
        assert_eq!(made(&mut paint, slot), 0);
    }
    let gold = [1.0, 0.8, 0.2, 1.0];
    // A ramp of one colour, of seventeen, edges the wrong way round: none.
    assert_eq!(paint.set_ramp(0, &[gold], [0.0, 1.0], true), 0);
    assert_eq!(paint.set_ramp(0, &[gold; 17], [0.0, 1.0], true), 0);
    assert_eq!(paint.set_ramp(0, &[gold; 2], [0.7, 0.2], true), 0);
    assert_eq!(paint.render(0, &case, 4, 4), Err(0));
    // Colours and edges outside 0 to 1 are held to it, as gmnb.ramp did.
    let wild = [[2.0, -1.0, 0.5, 1.0], [0.0, 0.0, 0.0, 7.0]];
    assert_eq!(paint.set_ramp(1, &wild, [-0.5, 1.5], false), 1);
    assert_eq!(made(&mut paint, 1), 2);
    assert_eq!(made(&mut paint, 9), 0);
    let held = Ramp::new(
        &[[1.0, 0.0, 0.5, 1.0], [0.0, 0.0, 0.0, 1.0]],
        [0.0, 1.0],
        false,
    )
    .unwrap();
    let swept = Case {
        data: vec![0.0, 1.0, 0.0, 1.0],
        ..case
    };
    assert_eq!(
        paint.render(1, &swept, 300, 2).unwrap(),
        swept.native(&held, 300, 2)
    );
    // Fields and sizes the pass doesn't take: 2, and nothing made.
    let wide = Case {
        cols: 257,
        data: vec![0.0; 514],
        ..swept.clone()
    };
    assert_eq!(paint.render(1, &wide, 4, 4), Err(2));
    assert_eq!(paint.render(1, &swept, 16_385, 1), Err(2));
    assert_eq!(paint.render(1, &swept, 512, 513), Err(2));
    assert_eq!(paint.render(1, &swept, 0, 4), Err(2));
    assert_eq!(paint.render(8, &swept, 4, 4), Err(2));
    assert_eq!(paint.at(8, 0), 0);
    assert_eq!(paint.at(0, 4), 0);
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

/// What a pass costs at Swirls' sizes, the module (as the app runs it:
/// epoch interruption on, a deadline far off) against the native pass:
/// the median of 50 after 5. Run with `--release -- --ignored
/// --nocapture`.
#[test]
#[ignore = "a measurement: cargo test --release -p gmnb-theme --test swirls_paint -- --ignored --nocapture"]
fn what_a_pass_costs() {
    let mut config = wasmtime::Config::new();
    config
        .wasm_simd(true)
        .wasm_relaxed_simd(false)
        .cranelift_nan_canonicalization(true)
        .epoch_interruption(true);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let module = wasmtime::Module::new(&engine, PAINT).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    store.set_epoch_deadline(u64::MAX / 2);
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let mut paint = Paint {
        memory: instance.get_memory(&mut store, "memory").unwrap(),
        at: instance.get_typed_func(&mut store, "at").unwrap(),
        ramp: instance.get_typed_func(&mut store, "ramp").unwrap(),
        made: instance.get_typed_func(&mut store, "made").unwrap(),
        size: instance.get_typed_func(&mut store, "size").unwrap(),
        render: instance.get_typed_func(&mut store, "render").unwrap(),
        store,
    };
    let (colors, edge, cycle) = swirls_ramps().remove(0);
    let ramp = Ramp::new(&colors, edge, cycle).unwrap();
    assert_eq!(paint.set_ramp(0, &colors, edge, cycle), 1);
    let mut rng = Rng(7);
    let median = |mut t: Vec<f64>| {
        t.sort_by(f64::total_cmp);
        t[t.len() / 2]
    };
    for (window, cols, rows, w, h) in [
        ("760x640", 65, 55, 380, 320),
        ("1600x1000", 77, 49, 506, 316),
        ("1920x1080", 82, 47, 533, 300),
    ] {
        let case = Case {
            next: Some(Case::random(&mut rng, cols, rows, 2).data),
            mix: 0.4,
            ..Case::random(&mut rng, cols, rows, 2)
        };
        let (mut module, mut native, mut write) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..55 {
            // The inputs written, as a theme writes them when they change.
            let t = cpu_ms();
            let (a, b) = (paint.at(0, 0), paint.at(0, 1));
            paint.write(a, &case.data);
            paint.write(b, case.next.as_ref().unwrap());
            let written = cpu_ms() - t;
            let t = cpu_ms();
            let done = paint
                .render
                .call(
                    &mut paint.store,
                    (0, cols as u32, rows as u32, 2, 1, case.mix, 1.25, 0.0, w, h),
                )
                .unwrap();
            let took = cpu_ms() - t;
            assert_eq!(done, 1);
            let t = cpu_ms();
            std::hint::black_box(case.native(&ramp, w, h));
            let natively = cpu_ms() - t;
            if i >= 5 {
                module.push(took);
                native.push(natively);
                write.push(written);
            }
        }
        println!(
            "{window} ({w} x {h}): module {:.3} ms a pass, native {:.3} ms; writing both snapshots {:.3} ms",
            median(module),
            median(native),
            median(write)
        );
    }
}
