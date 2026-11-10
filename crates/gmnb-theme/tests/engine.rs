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
    let f = hook(&e, "return function() while true do end end");
    let t = Instant::now();
    assert_eq!(call(&e, &f), Err(ThemeError::Timeout(ms(30))));
    assert_prompt(t);
    // The style budget is its own.
    let t = Instant::now();
    assert_eq!(
        e.call(&f, Budget::Style, ()),
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
    // returning late is a timeout all the same.
    let e = engine();
    let f = hook(
        &e,
        "local t = table.create(200000) for i = 1, #t do t[i] = (i * 7919) % 200003 end
         return function() table.sort(t) table.sort(t, function(a, b) return a > b end) end",
    );
    assert_eq!(
        e.call(&f, Budget::Custom(Duration::from_micros(1)), ()),
        Err(ThemeError::Timeout(Duration::from_micros(1)))
    );
}

// ---- Recursion and the C stack -------------------------------------------

#[test]
fn recursion_bombs_fail_cleanly() {
    let e = engine();
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
        let t = Instant::now();
        match load(&e, src) {
            // xpcall's handler retrying each failure makes a tree of calls,
            // which the deadline cuts.
            Err(ThemeError::Runtime { .. } | ThemeError::Memory(_) | ThemeError::Timeout(_)) => {}
            // pcall recursion unwinds into its own catchers.
            Ok(_) if src.contains("pcall") => {}
            other => panic!("{src}: {other:?}"),
        }
        assert_prompt(t);
    }
    assert_usable(&e);
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
    let e = roomy();
    assert_eq!(
        load(&e, "return #string.rep('x', 64 * 1024 * 1024)"),
        Err(ThemeError::Memory(8 << 20))
    );
    assert_eq!(
        load(&e, "local s = 'x' while true do s = s .. s end"),
        Err(ThemeError::Memory(8 << 20))
    );
    assert_usable(&e);
}

#[test]
fn table_growth_memory_bomb() {
    let e = roomy();
    let f = hook(
        &e,
        "return function() local t = {} for i = 1, 1e9 do t[i] = i end end",
    );
    assert_eq!(
        e.call(&f, Budget::Load, ()),
        Err(ThemeError::Memory(8 << 20))
    );
    // What the failed call left was collected: the next has the room.
    let g = hook(
        &e,
        "return function() local t = table.create(300000, 1) return #t end",
    );
    assert_eq!(e.call(&g, Budget::Load, ()), Ok(ThemeValue::Int(300000)));
    assert!(e.used_memory() < 8 << 20);
    assert_usable(&e);
}

#[test]
fn a_caught_memory_error_stays_under_the_cap() {
    // A script may catch the error; the cap held all the same.
    let e = roomy();
    assert_eq!(
        load(
            &e,
            "local ok = pcall(string.rep, 'x', 64 * 1024 * 1024) return ok"
        ),
        Ok(ThemeValue::Bool(false))
    );
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
        Ok(ThemeValue::Nil) => {}
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
            "debug",
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
             for _, lib in {'os', 'debug'} do
                 for k in pairs(_G[lib]) do out[#out + 1] = lib .. '.' .. k end
             end
             table.sort(out)
             return table.concat(out, ' ')"
        ),
        Ok(ThemeValue::String(
            "debug.info debug.traceback os.clock os.date os.difftime os.time".into()
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
    // an error.
    assert!(matches!(
        load(&e, "error(string.rep('x', 4 * 1024 * 1024))"),
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
    // Strikes are per hook, and any failure counts.
    assert_eq!(call(&e, &good), Ok(ThemeValue::Int(1)));
    for _ in 0..3 {
        assert_eq!(call(&e, &slow), Err(ThemeError::Timeout(ms(30))));
    }
    assert_eq!(call(&e, &slow), Err(ThemeError::Disabled(3)));
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
    limit("return string.rep('s', 70000)", "a string of 70000 bytes");
    limit(
        "local t = {} for i = 1, 20 do t[i] = string.rep('s', 60000) .. i end return t",
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
