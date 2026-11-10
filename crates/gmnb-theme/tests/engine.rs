//! The engine against hostile scripts: every limit, every escape tried, and
//! the conversion's caps. Deadlines are short and the ceilings on how long
//! a stopped script may take generous, so these hold on a loaded machine
//! and in debug builds; the cost of a frame call is measured by the
//! ignored `frame_calls_are_cheap` (run it with `--release -- --ignored`).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use gmnb_theme::mlua::{self, Value};
use gmnb_theme::{Budget, Engine, FunctionHandle, Limits, ThemeError, ThemeValue};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Short deadlines, so tests that wait for one stay fast.
fn quick() -> Limits {
    Limits {
        load: ms(100),
        style: ms(50),
        frame: ms(30),
        ..Limits::default()
    }
}

fn engine() -> Engine {
    Engine::new(quick()).unwrap()
}

/// Deadlines that real work (filling the heap, building big tables) can't
/// reach on a loaded machine, for tests that expect another outcome.
fn roomy() -> Engine {
    let long = Duration::from_secs(5);
    Engine::new(Limits {
        load: long,
        style: long,
        frame: long,
        ..Limits::default()
    })
    .unwrap()
}

fn load(e: &Engine, src: &str) -> Result<ThemeValue, ThemeError> {
    e.load("=test", src)
}

/// The function a chunk returns.
fn hook(e: &Engine, src: &str) -> FunctionHandle {
    match load(e, src) {
        Ok(ThemeValue::Function(f)) => f,
        other => panic!("not a function: {other:?}"),
    }
}

fn call(e: &Engine, f: &FunctionHandle) -> Result<ThemeValue, ThemeError> {
    e.call(f, Budget::Frame, ())
}

/// A stopped script must come back long before a hang would be noticed.
fn assert_prompt(start: Instant) {
    let t = start.elapsed();
    assert!(t < Duration::from_secs(5), "took {t:?}");
}

fn assert_usable(e: &Engine) {
    assert_eq!(load(e, "return 1 + 1"), Ok(ThemeValue::Int(2)));
    let f = hook(
        e,
        "return function(n) local t = table.create(1000, n) return #t end",
    );
    assert_eq!(e.call(&f, Budget::Style, 7), Ok(ThemeValue::Int(1000)));
}

fn map(entries: &[(&str, ThemeValue)]) -> ThemeValue {
    ThemeValue::Map(
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}

// ---- Deadlines ---------------------------------------------------------

#[test]
fn infinite_loop_at_load() {
    let e = engine();
    let t = Instant::now();
    assert_eq!(
        load(&e, "while true do end"),
        Err(ThemeError::Timeout(ms(100)))
    );
    assert!(t.elapsed() >= ms(100));
    assert_prompt(t);
    assert_usable(&e);
}

#[test]
fn infinite_loop_in_a_call() {
    let e = engine();
    let spin = "return function() while true do end end";
    let t = Instant::now();
    assert_eq!(call(&e, &hook(&e, spin)), Err(ThemeError::Timeout(ms(30))));
    assert_prompt(t);
    // The style budget is its own.
    let t = Instant::now();
    assert_eq!(
        e.call(&hook(&e, spin), Budget::Style, ()),
        Err(ThemeError::Timeout(ms(50)))
    );
    assert!(t.elapsed() >= ms(50));
    assert_usable(&e);
}

#[test]
fn a_loop_that_catches_the_timeout_is_still_stopped() {
    let e = engine();
    for src in [
        "return function() while true do pcall(function() while true do end end) end end",
        // The handler loops too.
        "return function() xpcall(function() while true do end end, function() while true do end end) end",
        // Nested catchers, each retrying.
        "return function() local function g() while true do pcall(g) end end g() end",
        // Errors caught, the loop's own condition never checked.
        "return function() repeat local ok = pcall(error, 'x') until false end",
    ] {
        let f = hook(&e, src);
        let t = Instant::now();
        assert_eq!(call(&e, &f), Err(ThemeError::Timeout(ms(30))), "{src}");
        assert_prompt(t);
    }
    assert_usable(&e);
}

#[test]
fn a_call_that_swallows_the_timeout_and_returns_still_times_out() {
    let e = engine();
    let f = hook(
        &e,
        "return function() pcall(function() while true do end end) return 'survived' end",
    );
    assert_eq!(call(&e, &f), Err(ThemeError::Timeout(ms(30))));
}

#[test]
fn coroutines_dont_escape_the_deadline() {
    let e = engine();
    for src in [
        "return function() coroutine.wrap(function() while true do end end)() end",
        // A coroutine that catches the error and yields back, resumed forever.
        "return function()
            local co = coroutine.wrap(function()
                while true do pcall(function() while true do end end) coroutine.yield() end
            end)
            while true do pcall(co) end
        end",
        // Nested coroutines.
        "return function()
            coroutine.wrap(function() coroutine.wrap(function() while true do end end)() end)()
        end",
    ] {
        let f = hook(&e, src);
        let t = Instant::now();
        assert_eq!(call(&e, &f), Err(ThemeError::Timeout(ms(30))), "{src}");
        assert_prompt(t);
    }
    // A coroutine made in one call and resumed in the next gets that call's
    // deadline.
    let f = hook(
        &e,
        "local co = coroutine.wrap(function() coroutine.yield(1) while true do end end)
         return function() return co() end",
    );
    assert_eq!(call(&e, &f), Ok(ThemeValue::Int(1)));
    assert_eq!(call(&e, &f), Err(ThemeError::Timeout(ms(30))));
    assert_usable(&e);
}

#[test]
fn a_sort_comparator_that_loops() {
    let e = engine();
    let f = hook(
        &e,
        "return function() table.sort({3, 2, 1}, function(a, b) while true do end end) end",
    );
    let t = Instant::now();
    assert_eq!(call(&e, &f), Err(ThemeError::Timeout(ms(30))));
    assert_prompt(t);
    // And a pattern that backtracks for ever (Luau's matcher checks the
    // interrupt).
    let f = hook(
        &e,
        r#"return function() return string.find(string.rep("a", 60), string.rep("a*", 60) .. "b") end"#,
    );
    assert_eq!(call(&e, &f), Err(ThemeError::Timeout(ms(30))));
}

#[test]
fn one_library_call_past_the_deadline_is_a_timeout() {
    // Sorting numbers runs no script code, so the interrupt can't stop it;
    // returning late is a timeout all the same, and returning more than
    // four times late switches the hook off at once.
    let e = roomy();
    let f = hook(
        &e,
        "local t = table.create(300000, 0) for i = 1, #t do t[i] = (i * 7919) % 300007 end
         return function() table.sort(t) table.sort(t, function(a, b) return a > b end) end",
    );
    assert_eq!(
        e.call(&f, Budget::Custom(ms(1)), ()),
        Err(ThemeError::Timeout(ms(1)))
    );
    assert_eq!(f.strikes(), 1);
    assert!(f.is_disabled());
    assert_eq!(
        e.call(&f, Budget::Custom(ms(1)), ()),
        Err(ThemeError::Disabled(1))
    );
    assert!(!e.is_dead());
}

#[test]
fn a_limit_is_of_processor_time_not_of_the_clock_on_the_wall() {
    // A call that loses the processor (here: the host sleeps inside it,
    // as a busy machine would make it) hasn't used its limit: it isn't a
    // timeout, whether it is interrupted after the wait or returns right
    // after it.
    let nap = |lua: &gmnb_theme::mlua::Lua| {
        let f = lua.create_function(|_, ms: u64| {
            std::thread::sleep(Duration::from_millis(ms));
            Ok(())
        })?;
        lua.globals().raw_set("nap", f)
    };
    let e = Engine::with_globals(Limits::default(), nap).unwrap();
    let returns = hook(&e, "return function() nap(120) return 1 end");
    let t = Instant::now();
    assert_eq!(
        e.call(&returns, Budget::Custom(ms(30)), ()),
        Ok(ThemeValue::Int(1))
    );
    assert!(t.elapsed() >= ms(120));
    let goes_on = hook(
        &e,
        "return function() nap(120) local n = 0 for i = 1, 1000 do n += i end return n end",
    );
    assert_eq!(
        e.call(&goes_on, Budget::Custom(ms(30)), ()),
        Ok(ThemeValue::Int(500500))
    );
    assert_eq!((returns.strikes(), goes_on.strikes()), (0, 0));
    // What it runs itself still counts, after any wait.
    let spins = hook(&e, "return function() nap(60) while true do end end");
    assert_eq!(
        e.call(&spins, Budget::Custom(ms(30)), ()),
        Err(ThemeError::Timeout(ms(30)))
    );
    // A span is not over for having waited.
    let span = e.span(Budget::Custom(ms(30)));
    std::thread::sleep(ms(60));
    assert!(!span.is_over());
    assert_eq!(e.call_in(&returns, &span, ()), Ok(ThemeValue::Int(1)));
}

#[test]
fn the_contracts_budgets() {
    let d = Limits::default();
    assert_eq!(
        [
            d.load, d.style, d.frame, d.preview, d.visible, d.action, d.place
        ],
        [250, 50, 8, 10, 10, 10, 2].map(ms)
    );
    assert_eq!(
        (d.memory, d.strikes, d.string_bytes, d.source_bytes),
        (8 << 20, 3, 4096, 512 << 10)
    );
    let e = Engine::new(Limits::default()).unwrap();
    let spin = "return function() while true do end end";
    for budget in [Budget::Preview, Budget::Visible, Budget::Action] {
        assert_eq!(
            e.call(&hook(&e, spin), budget, ()),
            Err(ThemeError::Timeout(ms(10)))
        );
    }
    // A span shares one limit between calls: all of a keypad frame's
    // `place` calls. (Spans here are long where calls must succeed: 2 ms
    // is no promise on a loaded machine.)
    let place = hook(&e, "return function(i) return i end");
    let span = e.span(Budget::Custom(Duration::from_secs(5)));
    for i in 1..=3 {
        assert_eq!(e.call_in(&place, &span, i), Ok(ThemeValue::Int(i)));
    }
    // The slow call stops when the span ends, which began before it.
    let slow = hook(&e, spin);
    let t = Instant::now();
    let span = e.span(Budget::Custom(ms(40)));
    assert_eq!(e.call_in(&place, &span, 0), Ok(ThemeValue::Int(0)));
    assert_eq!(
        e.call_in(&slow, &span, ()),
        Err(ThemeError::Timeout(ms(40)))
    );
    assert!(t.elapsed() >= ms(40));
    assert!(span.is_over());
    // Once it's over, a call fails without running, and isn't the hook's
    // fault.
    assert_eq!(
        e.call_in(&place, &span, 4),
        Err(ThemeError::Timeout(ms(40)))
    );
    assert_eq!(place.strikes(), 0);
    assert_eq!(
        e.call_in(&place, &e.span(Budget::Custom(Duration::from_secs(5))), 5),
        Ok(ThemeValue::Int(5))
    );
}

// ---- Recursion and the C stack -------------------------------------------

#[test]
fn recursion_bombs_fail_cleanly() {
    for src in [
        "local function f() return f() + 1 end return f()",
        // Through metamethods, pcall, coroutines, gsub and tostring: each
        // level a C call.
        "local t = setmetatable({}, {__index = function(t, k) return t[k] end}) return t.x",
        "local t = setmetatable({}, {__tostring = function(s) return tostring(s) end}) return tostring(t)",
        "local function nest() return coroutine.wrap(nest)() end return nest()",
        "local function g(s) return (string.gsub(s, '.', g)) end return g('ab')",
        "local t = setmetatable({}, {__tostring = function(s) print(s) return 'x' end}) print(t)",
        "local function f() return select(2, pcall(f)) end return f()",
        "local function f() local ok, e = xpcall(f, f) error(e) end f()",
    ] {
        // Its own engine for each: the heap cap is fatal.
        let e = engine();
        let t = Instant::now();
        match load(&e, src) {
            Err(ThemeError::Memory(_)) => assert!(e.is_dead(), "{src}"),
            // xpcall's handler retrying each failure makes a tree of calls,
            // which the deadline cuts.
            Err(ThemeError::Runtime { .. } | ThemeError::Timeout(_)) => assert_usable(&e),
            // pcall recursion unwinds into its own catchers.
            Ok(_) if src.contains("pcall") => assert_usable(&e),
            other => panic!("{src}: {other:?}"),
        }
        assert_prompt(t);
    }
}

#[test]
fn deeply_nested_source_is_a_syntax_error() {
    let e = engine();
    for (open, close) in [
        ("{", "}"),
        ("(", ")"),
        ("not ", ""),
        ("function() return ", " end"),
    ] {
        let src = format!("return {}1{}", open.repeat(20_000), close.repeat(20_000));
        assert!(
            matches!(load(&e, &src), Err(ThemeError::Syntax(_))),
            "{open}"
        );
    }
    assert_usable(&e);
}

// ---- Memory -------------------------------------------------------------

#[test]
fn string_rep_memory_bomb() {
    for src in [
        "return #string.rep('x', 64 * 1024 * 1024)",
        "local s = 'x' while true do s = s .. s end",
    ] {
        let e = roomy();
        assert_eq!(load(&e, src), Err(ThemeError::Memory(8 << 20)), "{src}");
        assert!(e.is_dead());
    }
}

#[test]
fn table_growth_memory_bomb() {
    let e = roomy();
    let f = hook(
        &e,
        "return function() local t = {} for i = 1, 1e9 do t[i] = i end end",
    );
    let g = hook(&e, "return function() return 1 end");
    let err = e.call(&f, Budget::Load, ());
    assert_eq!(err, Err(ThemeError::Memory(8 << 20)));
    // Fatal: the engine is stopped, the host unloads the theme, and
    // everything fails the same way without running.
    assert!(err.unwrap_err().is_fatal());
    assert!(e.is_dead());
    assert_eq!(call(&e, &g), Err(ThemeError::Memory(8 << 20)));
    assert_eq!(g.strikes(), 0);
    assert_eq!(load(&e, "return 1"), Err(ThemeError::Memory(8 << 20)));
    assert!(matches!(e.host_table(0, 1), Err(ThemeError::Memory(_))));
}

#[test]
fn a_caught_memory_error_stays_under_the_cap() {
    // A script may catch the error, and the host never hears of it; the
    // cap held all the same.
    let e = roomy();
    assert_eq!(
        load(
            &e,
            "local ok = pcall(string.rep, 'x', 64 * 1024 * 1024) return ok"
        ),
        Ok(ThemeValue::Bool(false))
    );
    assert!(!e.is_dead());
}

#[test]
fn host_work_with_the_heap_full_fails_cleanly() {
    let e = roomy();
    // Fill the heap, keep it full, and hand back more than fits.
    let r = load(
        &e,
        "hold = {}
         pcall(function() while true do hold[#hold + 1] = string.rep('x', 4000) .. #hold end end)
         local fs = {}
         for i = 1, 200 do fs[i] = function() return i end end
         return { fs = fs, show = function(x) print(x, string.rep('y', 100)) end }",
    );
    let show = match r {
        Ok(v) => v.get("show").and_then(ThemeValue::as_function).cloned(),
        Err(ThemeError::Memory(_)) => None,
        Err(other) => panic!("{other:?}"),
    };
    match e.host_table(300, 8) {
        Ok(_) | Err(ThemeError::Memory(_)) => {}
        Err(other) => panic!("{other:?}"),
    }
    if let Some(show) = show {
        match e.call(&show, Budget::Style, "hello") {
            Ok(_) | Err(ThemeError::Memory(_)) => {}
            Err(other) => panic!("{other:?}"),
        }
    }
    match e.convert(Value::Nil) {
        Ok(ThemeValue::Nil) => assert!(!e.is_dead()),
        Err(ThemeError::Memory(_)) => assert!(e.is_dead()),
        other => panic!("{other:?}"),
    }
}

// ---- The sandbox ----------------------------------------------------------

#[test]
fn require_and_loadstring_are_gone() {
    let e = engine();
    for name in [
        "require",
        "loadstring",
        "debug",
        "io",
        "getfenv",
        "setfenv",
        "newproxy",
        "load",
        "dofile",
    ] {
        assert_eq!(
            load(&e, &format!("return type({name})")),
            Ok(ThemeValue::String("nil".into())),
            "{name}"
        );
    }
    assert!(matches!(
        load(&e, "return require('./evil')"),
        Err(ThemeError::Runtime { .. })
    ));
    assert!(matches!(
        load(&e, "return loadstring('return 1')()"),
        Err(ThemeError::Runtime { .. })
    ));
}

#[test]
fn the_globals_left() {
    let e = engine();
    let names = load(
        &e,
        "local out = {}
         for k in pairs(_G) do out[#out + 1] = k end
         table.sort(out)
         return out",
    )
    .unwrap();
    let ThemeValue::Array(names) = names else {
        panic!("{names:?}")
    };
    let names: Vec<&str> = names.iter().filter_map(ThemeValue::as_str).collect();
    assert_eq!(
        names,
        [
            "_G",
            "_VERSION",
            "assert",
            "bit32",
            "buffer",
            "collectgarbage",
            "coroutine",
            "error",
            "gcinfo",
            "getmetatable",
            "ipairs",
            "math",
            "next",
            "os",
            "pairs",
            "pcall",
            "print",
            "rawequal",
            "rawget",
            "rawlen",
            "rawset",
            "select",
            "setmetatable",
            "string",
            "table",
            "tonumber",
            "tostring",
            "type",
            "typeof",
            "unpack",
            "utf8",
            "vector",
            "xpcall",
        ]
    );
    // What the small libraries hold.
    assert_eq!(
        load(
            &e,
            "local out = {}
             for _, lib in {'os'} do
                 for k in pairs(_G[lib]) do out[#out + 1] = lib .. '.' .. k end
             end
             table.sort(out)
             return table.concat(out, ' ')"
        ),
        Ok(ThemeValue::String(
            "os.clock os.date os.difftime os.time".into()
        ))
    );
    // collectgarbage only counts.
    assert!(matches!(
        load(&e, "return collectgarbage('count')"),
        Ok(ThemeValue::Number(_))
    ));
    for opt in ["collect", "stop", "step", "restart"] {
        assert!(
            matches!(
                load(&e, &format!("collectgarbage('{opt}')")),
                Err(ThemeError::Runtime { .. })
            ),
            "{opt}"
        );
    }
}

#[test]
fn libraries_and_host_globals_are_read_only() {
    let e = Engine::with_globals(quick(), |lua| {
        let api = lua.create_table()?;
        let colors = lua.create_table()?;
        colors.set("accent", "#c9b8ff")?;
        api.set("colors", colors)?;
        api.set_metatable(Some(lua.create_table()?))?;
        lua.globals().set("api", api)
    })
    .unwrap();
    for src in [
        "string.rep = nil",
        "rawset(string, 'evil', 1)",
        "table.insert(math, 1)",
        "_G.print = nil",
        "rawset(_G, 'x', 1)",
        "setmetatable(_G, {})",
        "getmetatable('').__index = {}",
        "getmetatable('').__index.upper = nil",
        "api.colors.accent = 'red'",
        "getmetatable(api).__index = {}",
        "setmetatable(api, nil)",
        "table.clear(api.colors)",
    ] {
        assert!(
            matches!(load(&e, src), Err(ThemeError::Runtime { .. })),
            "{src}"
        );
    }
    // A theme's own globals are its own, and shadowing a library only
    // affects the theme.
    assert_eq!(
        load(&e, "x = 41 string = nil return x + 1"),
        Ok(ThemeValue::Int(42))
    );
    assert_eq!(
        load(&e, "return api.colors.accent"),
        Ok(ThemeValue::String("#c9b8ff".into()))
    );
}

#[test]
fn bytecode_is_refused() {
    let e = engine();
    let bytecode = mlua::chunk::Compiler::new()
        .compile("return 'from bytecode'")
        .unwrap();
    // Luau's bytecode version, then its type-info version.
    assert_eq!(bytecode[..2], [9, 3]);
    assert_eq!(
        e.load("=bc", &bytecode),
        Err(ThemeError::Unsupported(
            "precompiled Luau bytecode; themes are loaded from source".into()
        ))
    );
    // A compile error's "bytecode" (a 0 then the message), and junk.
    for junk in [b"\0oops".as_slice(), &[3, 1, 2, 3], &[6]] {
        assert!(
            matches!(
                e.load("=bc", junk),
                Err(ThemeError::Unsupported(_) | ThemeError::Syntax(_))
            ),
            "{junk:?}"
        );
    }
    assert_usable(&e);
}

#[test]
fn source_size_is_capped() {
    let e = engine();
    let src = format!("return '{}'", "x".repeat(1 << 20));
    assert!(matches!(load(&e, &src), Err(ThemeError::Limit(_))));
}

// ---- Errors -------------------------------------------------------------

#[test]
fn huge_and_odd_error_values() {
    let e = engine();
    let Err(ThemeError::Runtime { message, traceback }) =
        load(&e, "error(string.rep('x', 512 * 1024))")
    else {
        panic!()
    };
    assert!(message.len() < 1100, "{}", message.len());
    assert!(message.ends_with(" more bytes)"), "{message}");
    assert!(traceback.len() < 2100);
    // Bigger, and the traceback mlua adds doesn't fit in the heap: still
    // an error (and, if the heap ran out, a fatal one).
    assert!(matches!(
        load(&engine(), "error(string.rep('x', 4 * 1024 * 1024))"),
        Err(ThemeError::Memory(_) | ThemeError::Runtime { .. })
    ));
    for (src, has) in [
        ("error({code = 1})", "table: "),
        ("error(nil)", "nil"),
        ("error(42)", "42"),
        (
            "error(setmetatable({}, {__tostring = function() return 'custom' end}))",
            "custom",
        ),
        (
            "error(setmetatable({}, {__tostring = function() error('again') end}))",
            "",
        ),
        (
            "error(setmetatable({}, {__tostring = function() return {} end}))",
            "",
        ),
        (
            "error(setmetatable({}, {__tostring = function() return string.rep('y', 1e6) end}))",
            "y",
        ),
        ("error('\\255\\254 not utf-8')", "not utf-8"),
    ] {
        match load(&e, src) {
            Err(ThemeError::Runtime { message, .. }) => {
                assert!(message.contains(has), "{src}: {message}");
                assert!(message.len() < 1100, "{src}");
            }
            other => panic!("{src}: {other:?}"),
        }
    }
    // A __tostring on the error that loops is stopped like anything else.
    assert_eq!(
        load(
            &e,
            "error(setmetatable({}, {__tostring = function() while true do end end}))"
        ),
        Err(ThemeError::Timeout(ms(100)))
    );
    // A traceback names the line.
    let Err(ThemeError::Runtime { message, traceback }) =
        e.load("@theme.luau", "local x = 1\nerror('boom')")
    else {
        panic!()
    };
    assert_eq!(message, "theme.luau:2: boom");
    assert!(traceback.starts_with("stack traceback:"), "{traceback}");
    assert!(matches!(
        load(&e, "return 1 +"),
        Err(ThemeError::Syntax(m)) if m.contains("Expected")
    ));
}

#[test]
fn a_hook_is_disabled_after_three_failures() {
    let e = engine();
    let v = load(
        &e,
        "local n = 0
         return {
             bad = function() error('no') end,
             slow = function() while true do end end,
             good = function() n += 1 return n end,
         }",
    )
    .unwrap();
    let f = |k| v.get(k).and_then(ThemeValue::as_function).unwrap().clone();
    let (bad, slow, good) = (f("bad"), f("slow"), f("good"));
    for i in 1..=3 {
        assert!(!bad.is_disabled());
        assert!(matches!(call(&e, &bad), Err(ThemeError::Runtime { .. })));
        assert_eq!(bad.strikes(), i);
    }
    assert!(bad.is_disabled());
    assert_eq!(call(&e, &bad), Err(ThemeError::Disabled(3)));
    assert_eq!(bad.strikes(), 3);
    // Strikes are per hook, and any failure counts. (A timeout normally
    // takes three too, or one if a stall made the call run four times its
    // limit.)
    assert_eq!(call(&e, &good), Ok(ThemeValue::Int(1)));
    let mut timeouts = 0;
    while !slow.is_disabled() {
        assert_eq!(call(&e, &slow), Err(ThemeError::Timeout(ms(30))));
        timeouts += 1;
        assert!(timeouts <= 3);
    }
    assert_eq!(call(&e, &slow), Err(ThemeError::Disabled(timeouts)));
    assert_eq!(call(&e, &good), Ok(ThemeValue::Int(2)));
    assert!(!good.is_disabled());
}

#[test]
fn handles_belong_to_their_engine() {
    let a = engine();
    let b = engine();
    let f = hook(&a, "return function() return 1 end");
    assert!(matches!(call(&b, &f), Err(ThemeError::Unsupported(_))));
    assert_eq!(call(&a, &f), Ok(ThemeValue::Int(1)));
    let t = b.host_table(0, 1).unwrap();
    assert!(matches!(
        a.fill(&t, |_| Ok(())),
        Err(ThemeError::Unsupported(_))
    ));
    // A handle outliving its engine is harmless.
    drop(a);
    drop(f);
}

// ---- print --------------------------------------------------------------

#[test]
fn print_writes_to_a_capped_log() {
    let e = engine();
    load(
        &e,
        "print('hello', 1, 2.5, nil, true, setmetatable({}, {__tostring = function() return 'obj' end}))
         print('\\255bad')",
    )
    .unwrap();
    let log = e.take_log();
    assert_eq!(log.lines, ["hello\t1\t2.5\tnil\ttrue\tobj", "\u{fffd}bad"]);
    assert_eq!(log.dropped, 0);
    assert!(e.take_log().lines.is_empty());

    // The newest lines are kept.
    load(&e, "for i = 1, 1000 do print(i) end").unwrap();
    let log = e.take_log();
    assert_eq!(log.lines.len(), 200);
    assert_eq!(log.lines[199], "1000");
    assert_eq!(log.dropped, 800);

    // A line longer than the log is cut; the log stays within its bytes.
    load(
        &e,
        "print(string.rep('z', 100000)) for i = 1, 100 do print(string.rep('w', 1000)) end",
    )
    .unwrap();
    let log = e.take_log();
    assert!(log.lines.iter().map(String::len).sum::<usize>() <= 16 << 10);
    assert!(log.dropped > 0);

    // A __tostring that errors makes print fail.
    assert!(matches!(
        load(
            &e,
            "print(setmetatable({}, {__tostring = function() error('no') end}))"
        ),
        Err(ThemeError::Runtime { .. })
    ));
}

// ---- Conversion -----------------------------------------------------------

#[test]
fn tables_become_arrays_and_maps() {
    let e = roomy();
    assert_eq!(
        load(
            &e,
            "return { name = 'Aurora', sizes = {1, 2.5, 3}, nested = { deep = { [\"odd key\"] = true } }, empty = {} }"
        ),
        Ok(map(&[
            ("empty", ThemeValue::Map(BTreeMap::new())),
            ("name", ThemeValue::String("Aurora".into())),
            (
                "nested",
                map(&[("deep", map(&[("odd key", ThemeValue::Bool(true))]))])
            ),
            (
                "sizes",
                ThemeValue::Array(vec![
                    ThemeValue::Int(1),
                    ThemeValue::Number(2.5),
                    ThemeValue::Int(3)
                ])
            ),
        ]))
    );
    // Arrays filled in any order.
    assert_eq!(
        load(&e, "local t = {} t[3] = 'c' t[1] = 'a' t[2] = 'b' return t"),
        Ok(ThemeValue::Array(
            ["a", "b", "c"]
                .map(|s| ThemeValue::String(s.into()))
                .to_vec()
        ))
    );
    for (src, what) in [
        ("return {1, 2, x = 3}", "both array entries and string keys"),
        ("return {[1] = 1, [3] = 3}", "aren't 1 to n"),
        ("return {[0] = 'zero'}", "aren't 1 to n"),
        ("return {[-1] = 1, [1] = 2}", "aren't 1 to n"),
        ("return {[1.5] = 1}", "not a whole one"),
        ("return {[true] = 1}", "key of type boolean"),
        ("return {[{}] = 1}", "key of type table"),
        ("return {a = {b = {[2] = 1}}}", "at value.a.b"),
        ("return {list = {1, 2, {x = 0/0}}}", "at value.list[3].x"),
        ("return vector.create(1, 2, 3)", "vector"),
        ("return buffer.create(4)", "buffer"),
        ("return coroutine.create(print)", "thread"),
        (
            "return {['not a name'] = {0/0}}",
            "at value[\"not a name\"][1]",
        ),
    ] {
        match load(&e, src) {
            Err(ThemeError::Unsupported(m)) => assert!(m.contains(what), "{src}: {m}"),
            other => panic!("{src}: {other:?}"),
        }
    }
}

#[test]
fn cyclic_tables() {
    let e = roomy();
    for src in [
        "local t = {} t.me = t return t",
        "local a, b = {}, {} a.b = b b.a = a return a",
        "local t = {} t[1] = {t} return t",
    ] {
        match load(&e, src) {
            Err(ThemeError::Unsupported(m)) => assert!(m.contains("cycle"), "{m}"),
            other => panic!("{src}: {other:?}"),
        }
    }
    // The same table twice, not inside itself, is no cycle.
    assert_eq!(
        load(&e, "local s = {1} return {a = s, b = s}"),
        Ok(map(&[
            ("a", ThemeValue::Array(vec![ThemeValue::Int(1)])),
            ("b", ThemeValue::Array(vec![ThemeValue::Int(1)])),
        ]))
    );
}

#[test]
fn deep_and_wide_trees_are_capped() {
    let e = roomy();
    let nest = |n: usize| format!("local t = 1 for i = 1, {n} do t = {{t}} end return t");
    // 32 tables deep is the cap.
    assert!(load(&e, &nest(32)).is_ok());
    assert!(matches!(load(&e, &nest(33)), Err(ThemeError::Limit(m)) if m.contains("deep")));
    assert!(matches!(
        load(&e, &nest(100_000)),
        Err(ThemeError::Limit(_))
    ));
    // No cycle, but two paths to everything: 2^30 values.
    let t = Instant::now();
    assert!(matches!(
        load(&e, "local t = {} for i = 1, 30 do t = {t, t} end return t"),
        Err(ThemeError::Limit(m)) if m.contains("values")
    ));
    assert_prompt(t);
    assert!(matches!(
        load(&e, "return table.create(30000, 1)"),
        Err(ThemeError::Limit(m)) if m.contains("values")
    ));
    // Strings: each, and all together.
    let limit = |src: &str, has: &str| match load(&e, src) {
        Err(ThemeError::Limit(m)) if m.contains(has) => {}
        other => panic!("{src}: {other:?}"),
    };
    assert_eq!(
        load(&e, "return string.rep('s', 4096)"),
        Ok(ThemeValue::String("s".repeat(4096)))
    );
    limit("return string.rep('s', 4097)", "a string of 4097 bytes");
    limit(
        "return {[string.rep('k', 5000)] = 1}",
        "a string of 5000 bytes",
    );
    limit(
        "local t = {} for i = 1, 300 do t[i] = string.rep('s', 4000) .. i end return t",
        "in all",
    );
    match load(&e, "return {['\\255'] = 1}") {
        Err(ThemeError::Unsupported(m)) if m.contains("UTF-8") => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn numbers() {
    let e = roomy();
    for (src, want) in [
        ("return 3", ThemeValue::Int(3)),
        ("return -7", ThemeValue::Int(-7)),
        ("return 0.5", ThemeValue::Number(0.5)),
        ("return 2^53", ThemeValue::Int(1 << 53)),
        ("return 2^62", ThemeValue::Int(1 << 62)),
        ("return 2^63", ThemeValue::Number(9.223372036854776e18)),
        ("return 1e308", ThemeValue::Number(1e308)),
        ("return -1e308", ThemeValue::Number(-1e308)),
        ("return 5e-324", ThemeValue::Number(5e-324)),
    ] {
        assert_eq!(load(&e, src), Ok(want), "{src}");
    }
    match load(&e, "return -0.0") {
        Ok(ThemeValue::Number(z)) => assert!(z == 0.0 && z.is_sign_negative()),
        other => panic!("{other:?}"),
    }
    for src in [
        "return 0/0",
        "return math.huge",
        "return -math.huge",
        "return 1e308 * 10",
    ] {
        assert!(
            matches!(load(&e, src), Err(ThemeError::Unsupported(m)) if m.contains("finite")),
            "{src}"
        );
    }
}

#[test]
fn metamethods_dont_run_during_conversion() {
    let e = engine();
    let spin = "function() while true do end end";
    let t = Instant::now();
    let v = load(
        &e,
        &format!(
            "local mt = {{ __index = {spin}, __newindex = {spin}, __len = {spin}, __pairs = {spin},
                          __tostring = {spin}, __eq = {spin}, __lt = {spin}, __call = {spin},
                          __iter = {spin}, __metatable = 'locked' }}
             local inner = table.freeze(setmetatable({{1, 2}}, {{ __index = {spin} }}))
             return setmetatable({{ a = 1, list = inner }}, mt)"
        ),
    );
    assert_prompt(t);
    assert_eq!(
        v,
        Ok(map(&[
            ("a", ThemeValue::Int(1)),
            (
                "list",
                ThemeValue::Array(vec![ThemeValue::Int(1), ThemeValue::Int(2)])
            ),
        ]))
    );
    // A table key with a __tostring isn't stringified.
    assert!(matches!(
        load(
            &e,
            &format!("return {{ [setmetatable({{}}, {{__tostring = {spin}}})] = 1 }}")
        ),
        Err(ThemeError::Unsupported(_))
    ));
}

#[test]
fn functions_become_handles() {
    let e = engine();
    let v = e
        .load(
            "@theme.luau",
            "local function frame(t) return t * 2 end\nreturn { frame = frame, again = frame, other = function() end }",
        )
        .unwrap();
    let frame = v.get("frame").and_then(ThemeValue::as_function).unwrap();
    assert_eq!(frame.label(), "frame (theme.luau:1)");
    // One function, one handle.
    assert_eq!(v.get("again"), v.get("frame"));
    assert_ne!(v.get("other"), v.get("frame"));
    assert_eq!(e.call(frame, Budget::Frame, 21), Ok(ThemeValue::Int(42)));
    assert_eq!(
        v.to_string(),
        "{\n  again = <function frame (theme.luau:1)>,\n  frame = <function frame (theme.luau:1)>,\n  \
         other = <function other (theme.luau:2)>,\n}"
    );
}

// ---- Host tables ----------------------------------------------------------

#[test]
fn host_tables_are_read_only_to_scripts() {
    let e = engine();
    let ctx = e.host_table(0, 4).unwrap();
    e.fill(&ctx, |t| t.raw_set("time", 1.5)).unwrap();
    for src in [
        "return function(ctx) ctx.time = 0 end",
        "return function(ctx) rawset(ctx, 'x', 1) end",
        "return function(ctx) setmetatable(ctx, {}) end",
        "return function(ctx) table.clear(ctx) end",
    ] {
        let f = hook(&e, src);
        assert!(
            matches!(
                e.call(&f, Budget::Frame, ctx.table()),
                Err(ThemeError::Runtime { .. })
            ),
            "{src}"
        );
    }
    let f = hook(&e, "return function(ctx) return ctx.time end");
    assert_eq!(
        e.call(&f, Budget::Frame, ctx.table()),
        Ok(ThemeValue::Number(1.5))
    );
    e.fill(&ctx, |t| t.raw_set("time", 2)).unwrap();
    assert_eq!(
        e.call(&f, Budget::Frame, ctx.table()),
        Ok(ThemeValue::Int(2))
    );
    assert!(ctx.table().is_readonly());
}

// ---- The host's allowance --------------------------------------------------

/// Fills the theme's heap to its cap with tables of about 1 KiB, lets go
/// of the last `spare` of them (the collection after the load frees them)
/// and keeps the rest. The chunk returns a function that allocates 16 KiB
/// more.
const NEARLY_FULL: &str = "
    local spare = ...
    hold = table.create(12000)
    local n = 0
    pcall(function()
        while true do
            hold[n + 1] = table.create(60, n)
            n += 1
        end
    end)
    for i = n - spare + 1, n do
        hold[i] = nil
    end
    return function() local t = table.create(1000, 0) return #t end";

fn nearly_full(e: &Engine, spare: u32) -> FunctionHandle {
    let src = NEARLY_FULL.replace("...", &spare.to_string());
    hook(e, &src)
}

/// A host table of `n` numbers: 16 bytes each, in an array that doubles.
fn host_numbers(e: &Engine, n: i64) -> Result<gmnb_theme::HostTable, ThemeError> {
    let t = e.host_table(0, 4)?;
    e.fill(&t, |t| {
        for i in 1..=n {
            t.raw_seti(i as usize, i as f64)?;
        }
        Ok(())
    })?;
    Ok(t)
}

/// A refill used to lift the heap cap and leave the heap above it, so the
/// theme's next allocation, however small, was a (fatal) memory error.
#[test]
fn a_host_refill_never_costs_the_theme_its_room() {
    let e = roomy();
    let more = nearly_full(&e, 256);
    let room = e.limits().memory - e.used_memory();
    assert!(room < 512 << 10, "{room} bytes left");
    // Far more than the theme has left.
    let table = host_numbers(&e, 20_000).unwrap();
    let held = e.host_memory();
    // 20 000 numbers of 16 bytes, in an array that doubled to 32 768.
    assert!(
        (20_000 * 16..=40_000 * 16).contains(&held),
        "the host holds {held} bytes"
    );
    assert!(e.used_memory() > e.limits().memory);
    assert_eq!(e.call(&more, Budget::Style, ()), Ok(ThemeValue::Int(1000)));
    assert!(!e.is_dead());
    // A refill in place takes no more.
    for round in 0..50 {
        e.fill(&table, |t| t.raw_seti(1 + round, 0.5)).unwrap();
    }
    assert_eq!(e.host_memory(), held);
    let none = hook(&e, "return function(t) return t[1] end");
    assert_eq!(
        e.call(&none, Budget::Style, table.table()),
        Ok(ThemeValue::Number(0.5))
    );
}

/// The allowance is the host's: with or without host tables, the theme's
/// own code gets `Limits::memory` and no more.
#[test]
fn the_hosts_allowance_is_not_the_themes() {
    const FILL: &str = "
        hold = table.create(12000)
        local n = 0
        local ok = pcall(function()
            while true do
                n += 1
                hold[n] = table.create(60, n)
            end
        end)
        return ok";
    for host in [0, 20_000] {
        let e = roomy();
        let _table = host_numbers(&e, host).unwrap();
        // The theme caught its own memory error: the cap held.
        assert_eq!(load(&e, FILL), Ok(ThemeValue::Bool(false)), "{host}");
        let own = e.used_memory() - e.host_memory();
        let cap = e.limits().memory;
        assert!(own <= cap, "{host}: the theme holds {own} bytes");
        // (Luau takes its memory a page at a time.)
        assert!(
            own > cap - (256 << 10),
            "{host}: the theme holds {own} bytes"
        );
    }
}

#[test]
fn a_host_overrun_is_a_limit_and_not_fatal() {
    let long = Duration::from_secs(5);
    let e = Engine::new(Limits {
        host_memory: 64 << 10,
        load: long,
        style: long,
        ..Limits::default()
    })
    .unwrap();
    let more = nearly_full(&e, 256);
    match host_numbers(&e, 20_000) {
        Err(ThemeError::Limit(m)) => assert!(m.contains("the allowance is 65536"), "{m}"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    assert!(!e.is_dead());
    // Within the allowance, the theme still has its room.
    let _small = host_numbers(&e, 100).unwrap();
    assert!(e.host_memory() <= 64 << 10);
    assert_eq!(e.call(&more, Budget::Style, ()), Ok(ThemeValue::Int(1000)));
}

/// 1000 frame calls, each refilling a 300-entry context in place and
/// running a hook that reads all of it: the mean refill, the mean call and
/// the worst of the two together.
fn frame_calls(e: &Engine) -> (Duration, Duration, Duration) {
    const KEYS: usize = 300;
    let ctx = e.host_table(0, 4).unwrap();
    let keys = e.host_table(KEYS, 0).unwrap();
    let mut rows = Vec::new();
    for _ in 0..KEYS {
        let row = e.host_table(0, 4).unwrap();
        e.fill(&keys, |t| t.raw_push(row.table())).unwrap();
        rows.push(row);
    }
    e.fill(&ctx, |t| t.raw_set("keys", keys.table())).unwrap();
    let frame = hook(
        e,
        "return function(ctx)
             local sum = 0
             for i, k in ctx.keys do
                 sum += k.x * k.w + k.y * k.h + math.sin(ctx.t + i)
             end
             return sum
         end",
    );
    let (mut filling, mut calling, mut worst) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    for n in 0..1000 {
        let t0 = Instant::now();
        let time = n as f64 / 60.0;
        e.fill(&ctx, |t| t.raw_set("t", time)).unwrap();
        for (i, row) in rows.iter().enumerate() {
            e.fill(row, |t| {
                t.raw_set("x", (i % 20) as f64 * 24.0)?;
                t.raw_set("y", (i / 20) as f64 * 24.0 + time)?;
                t.raw_set("w", 20.0)?;
                t.raw_set("h", 20.0)
            })
            .unwrap();
        }
        let t1 = Instant::now();
        let v = e.call(&frame, Budget::Frame, ctx.table()).unwrap();
        assert!(v.as_f64().is_some());
        let t2 = Instant::now();
        filling += t1 - t0;
        calling += t2 - t1;
        worst = worst.max(t2 - t0);
    }
    (filling / 1000, calling / 1000, worst)
}

#[test]
fn frame_calls_work() {
    let e = Engine::new(Limits {
        frame: Duration::from_secs(1),
        ..Limits::default()
    })
    .unwrap();
    frame_calls(&e);
}

#[test]
#[ignore = "a measurement: run with --release -- --ignored --nocapture"]
fn frame_calls_are_cheap() {
    let e = Engine::new(Limits::default()).unwrap();
    let (fill, call, worst) = frame_calls(&e);
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    println!(
        "1000 frames, a 300-entry context (1201 stores) refilled then a hook reading it all: \
         mean refill {:.1} µs + call {:.1} µs, worst frame {:.1} µs; heap {} KiB",
        us(fill),
        us(call),
        us(worst),
        e.used_memory() / 1024
    );
    assert!(
        fill + call < Duration::from_millis(1) / 4,
        "mean {:?}",
        fill + call
    );
}

// ---- Library calls the interrupt can't see into --------------------------
//
// Luau checks the interrupt only in its own instructions (calls, returns,
// loops) and in the pattern matcher's steps. A library function's own loop,
// and a library function calling another library function, are out of its
// reach: each case below ran for seconds to minutes on the main thread at
// its full size (one call of each, under any budget). Sized here to run
// ~0.1–0.6 s unfixed, where the call came back late (a timeout, the hook off
// at once); fixed, it is refused at once, or stopped at its deadline.

/// A hook doing `body`, after `setup` (run at load).
fn library_hook(e: &Engine, setup: &str, body: &str) -> FunctionHandle {
    hook(e, &format!("{setup}\nreturn function() {body} end"))
}

#[test]
fn searches_the_interrupt_cant_stop_are_refused() {
    // Plain searches compare the pattern at every place (string bytes ×
    // pattern bytes); a `[set]` repeated by `*`, `+` or `-` is matched
    // against each character with no interrupt between (string bytes × set
    // bytes); `table.move` copies one element at a time however many it is
    // told, present or not.
    let plain = "local s = string.rep('a', 200000) local p = string.rep('a', 100000) .. 'b'";
    let set = "local s = string.rep('a', 20000) local p = '^[' .. string.rep('b', 20000) .. 'a]*c'";
    // The same set after a `%b` whose closing byte is `%`.
    let balanced = "local s = 'x%' .. string.rep('a', 20000)
                    local p = '^%bx%[' .. string.rep('b', 20000) .. 'a]*c'";
    for (setup, body, what) in [
        (plain, "return (string.find(s, p, 1, true))", "string.find"),
        (plain, "return (s:find(p))", "string.find"),
        (plain, "return #s:split(p)", "string.split"),
        (set, "return (s:find(p))", "string.find"),
        (set, "return (string.match(s, p))", "string.match"),
        (set, "return (s:gsub(p, ''))", "string.gsub"),
        (set, "for _ in s:gmatch(p) do end", "string.gmatch"),
        (balanced, "return (s:find(p))", "string.find"),
        ("", "table.move({}, 1, 10000000, 2, {})", "table.move"),
    ] {
        let e = engine();
        let f = library_hook(&e, setup, body);
        let t = Instant::now();
        match e.call(&f, Budget::Custom(ms(10)), ()) {
            Err(ThemeError::Runtime { message, .. }) => {
                assert!(message.contains(what), "{body}: {message}");
                assert!(message.contains("too much"), "{body}: {message}");
            }
            other => panic!("{body}: {other:?} after {:?}", t.elapsed()),
        }
        assert_prompt(t);
        // An error like any other: one strike, the hook still on.
        assert_eq!(f.strikes(), 1, "{body}");
        assert!(!f.is_disabled(), "{body}");
        assert_usable(&e);
    }
}

#[test]
fn library_loops_that_call_back_keep_the_deadline() {
    // A library function that calls a function it is given (a comparator,
    // `__eq`, `__lt`, `__tostring`) calls a library function as directly,
    // with no interrupt between: n × m work in one call.
    let n = "local T = table.create(4000, 1) local t = table.create(4000, T)";
    let lt = "local A = setmetatable(table.create(4000, 1), {__lt = table.find})
              local t = table.create(4000, A)";
    let eq = "local mt = {__eq = table.find}
              local v = setmetatable(table.create(10000, 1), mt)
              local X = table.create(10000, setmetatable({}, mt))";
    let ts = "local T = setmetatable(table.create(5000, ''), {__tostring = table.concat})
              local args = table.create(7000, T) local f = string.rep('%*', 7000)";
    for (setup, body) in [
        (n, "table.sort(t, table.find)"),
        (lt, "table.sort(t)"),
        (eq, "return table.find(X, v)"),
        (ts, "return #string.format(f, table.unpack(args))"),
        (ts, "print(table.unpack(args))"),
    ] {
        let e = engine();
        let f = library_hook(&e, setup, body);
        let t = Instant::now();
        assert_eq!(
            e.call(&f, Budget::Custom(ms(10)), ()),
            Err(ThemeError::Timeout(ms(10))),
            "{body}"
        );
        assert_prompt(t);
        // Stopped at its deadline, not long after it.
        assert!(
            !f.is_disabled(),
            "{body}: came back after {:?}",
            t.elapsed()
        );
        assert_usable(&e);
    }
}

#[test]
fn the_bounded_library_works_as_before() {
    // Each line: what one call returned, all of it.
    let e = engine();
    let v = load(
        &e,
        r##"
        local out = {}
        local function add(...)
            local parts = {}
            for i = 1, select("#", ...) do parts[i] = tostring((select(i, ...))) end
            out[#out + 1] = table.concat(parts, " ")
        end
        add(string.find("hello world", "o w"))
        add(string.find("a.b", ".", 1, true))
        add(("hello"):find("l+"))
        add(string.find("hello", "l", -2))
        add(string.find("x", "y"))
        add(string.match("key = value", "^(%w+)%s*=%s*(%w+)$"))
        add(("abc"):match("b", 3))
        local words = {}
        for w in ("one two  three"):gmatch("%a+") do words[#words + 1] = w end
        add(table.concat(words, ","))
        add(("a-b-c"):gsub("-", "+"))
        add(("abc"):gsub("%w", { a = "1" }))
        add(("abc"):gsub(".", string.upper, 2))
        add(#("a,b,,c"):split(","), #string.split("a--b", "--"), #("abc"):split(""))
        local named = setmetatable({}, { __tostring = function() return "named" end })
        add(string.format("%d|%5.1f|%s|%*|%*|%%*", 7, 2.25, "s", named, 3))
        local t = { 3, 1, 2 }
        table.sort(t)
        add(table.concat(t))
        table.sort(t, function(a, b) return a > b end)
        add(table.concat(t))
        local mt = { __lt = function(a, b) return a.k < b.k end, __eq = function(a, b) return a.k == b.k end }
        local objs = { setmetatable({ k = 2 }, mt), setmetatable({ k = 1 }, mt) }
        table.sort(objs)
        add(objs[1].k, objs[2].k)
        add(table.find(objs, setmetatable({ k = 2 }, mt)), table.find({ 5, 6, 5 }, 5, 2), table.find(objs, {}))
        add(table.concat(table.move({ 1, 2, 3 }, 1, 3, 2), ","))
        add(table.foreachi({ "x", "y" }, function(i, v) if v == "y" then return i end end))
        add(table.foreach({ a = 1 }, function(k, v) return k .. v end))
        add(pcall(string.find, "x", "["))
        add(pcall(table.sort, { 1 }, 5))
        add(pcall(table.find, nil, 1))
        return out
        "##,
    );
    let ThemeValue::Array(out) = v.unwrap() else {
        panic!()
    };
    let out: Vec<&str> = out.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(
        out,
        [
            "5 7",
            "2 2",
            "3 4",
            "4 4",
            "nil",
            "key value",
            "nil",
            "one,two,three",
            "a+b+c 2",
            "1bc 3",
            "ABc 2",
            "4 2 3",
            "7|  2.2|s|named|3|%*",
            "123",
            "321",
            "1 2",
            "2 3 nil",
            "1,1,2,3",
            "2",
            "a1",
            // Errors are still errors, with the functions' own messages.
            "false malformed pattern (missing ']')",
            "false invalid argument #2 to 'sort' (function expected, got number)",
            "false invalid argument #1 to 'find' (table expected, got nil)",
        ]
    );
}

#[test]
fn a_theme_listed_but_not_chosen_cant_hold_up_settings() {
    // Settings (and About) run every installed theme's file to list it:
    // a theme only installed, never chosen, ran this at each opening.
    use gmnb_theme::theme::{self, Source};
    let dir = std::env::temp_dir().join(format!("gmnb-engine-{}-search", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("theme.luau"),
        "local s = string.rep('a', 400000)
         string.find(s, string.rep('a', 200000) .. 'b', 1, true)
         return { api = 1, name = 'Search' }",
    )
    .unwrap();
    let t = Instant::now();
    let listed = theme::list(&[Source::folder("search", &dir)], &Limits::default());
    let _ = std::fs::remove_dir_all(&dir);
    let error = listed[0].meta.as_ref().unwrap_err();
    assert!(
        error.contains("string.find"),
        "{error} after {:?}",
        t.elapsed()
    );
}

#[test]
fn reading_what_a_hook_returns_counts_against_its_time() {
    // The script takes no time at all; reading back the 20 000-entry map
    // it returns (raw, as the caps allow) takes the host some 9 ms. That
    // is the call's work, past a 2 ms limit (a `place`, a `sound`): a
    // timeout, and the hook off after three (at once if it took four
    // times the limit), as for a slow script.
    let big = "local t = {} for i = 1, 19990 do t['key' .. i] = i end
               return function() return t end";
    let e = roomy();
    let f = hook(&e, big);
    while !f.is_disabled() {
        let r = e.call(&f, Budget::Custom(ms(2)), ()).map(|_| "a value");
        assert_eq!(r, Err(ThemeError::Timeout(ms(2))));
        assert!(f.strikes() <= 3);
    }
    // Within its time it is read as before.
    let f = hook(&e, big);
    assert!(matches!(
        e.call(&f, Budget::Custom(ms(1000)), ()),
        Ok(ThemeValue::Map(m)) if m.len() == 19990
    ));
    // A span shared by several calls (a keypad frame's `place` calls): the
    // call whose reading uses it up is the one that fails.
    let f = hook(&e, big);
    let span = e.span(Budget::Custom(ms(2)));
    let r = e.call_in(&f, &span, ()).map(|_| "a value");
    assert_eq!(r, Err(ThemeError::Timeout(ms(2))));
    assert_eq!(f.strikes(), 1);
}
