//! Renders a theme's sounds to WAV files, to listen to, to look at in an
//! editor and to measure (`tools/theming/sound_check.py`) without running
//! the app. Nothing is played: no sound server is contacted and no thread
//! is started.
//!
//!     cargo run -p gmnb-sound --example render -- <theme folder | theme.luau> <out folder>
//!
//! Written to the out folder:
//!
//! - every sound the theme makes, named as `GMNB_SOUND_DUMP` names them
//!   (`03-<hash>.wav`: its order, and a hash of its recipe or file name);
//! - for every event the app has a name for, `event-<name>.wav`: eight of
//!   that event in a row, as the mixer would play them, on eight different
//!   keys spread across the window. Each is decided as the app decides it:
//!   by the theme's `sound` function if it has one and answers, else by
//!   its `sounds` table. This is where `pick`, a `rate` range or
//!   `pan = "position"` can be heard. A result or an error comes after
//!   the equals key, which the function is told of first;
//! - if the theme has a `sound` function, `option-<id>-<value>.wav` for
//!   every value of each of its choices and toggles that the function
//!   plays something for;
//! - `typing.wav`: twenty presses at eight a second (fifteen number keys,
//!   four operators and the equals key, with the results the app reports
//!   after an operator and after equals);
//! - `index.txt`: what is what, and anything the theme's author should
//!   know; `index.tsv`: the same list for a program (the files, what each
//!   is, and the frame at which each take starts).
//!
//! Everything is mixed at full volume (the app's Volume setting at 100%);
//! in the app the default is 50%, which is 12 dB quieter. Takes never
//! overlap: each starts once the one before has ended.

#[path = "support/stand_in.rs"]
mod stand_in;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use gmnb_sound::{
    EventName, Mixer, Pan, Pick, Play, Slot, SoundSource, Varied, dump_name, limits, prepare_all,
    wav,
};
use gmnb_theme::events::{EventKind, KeyKind, Page};
use gmnb_theme::theme::{OptionKind, Source, Theme};
use gmnb_theme::{Limits, OptionValue};
use stand_in::{By, StandIn};

/// How many of an event are played in its file, and how far apart at
/// least.
const TAKES: usize = 8;
const GAP_SECONDS: f64 = 0.28;
/// The silence between the end of one take and the start of the next, at
/// least.
const REST_SECONDS: f64 = 0.1;
/// Frames mixed at a time: a take starts on a multiple of this.
const BLOCK: usize = 64;
/// `typing.wav`: how many presses, and how many a second.
const TYPING_PRESSES: usize = 20;
const TYPING_RATE: f64 = 8.0;

const RATE: f64 = 48_000.0;

/// The events the app has a name for.
const EVENTS: [EventName; 12] = [
    EventName::Digit,
    EventName::Operator,
    EventName::Func,
    EventName::Equals,
    EventName::Toggle,
    EventName::Clear,
    EventName::Backspace,
    EventName::Memory,
    EventName::Button,
    EventName::Result,
    EventName::Error,
    EventName::Mode,
];

type Sounds = [Option<Arc<[i16]>>];

fn describe(source: &SoundSource) -> String {
    match source {
        SoundSource::Silent => "silent (it was asked for wrongly)".into(),
        SoundSource::Synth(r) => format!(
            "recipe: {} layer(s), {} effect(s), gain {}",
            r.layers.len(),
            r.effects.len(),
            r.gain
        ),
        SoundSource::File { name, gain, .. } => match gain {
            Some(gain) => format!("file {name}, gain {gain}"),
            None => format!("file {name}, as recorded"),
        },
    }
}

fn varied(v: Varied) -> String {
    match v {
        Varied::One(x) => x.to_string(),
        Varied::Range(low, high) => format!("{low} to {high}"),
    }
}

/// A number as short as it reads: three decimals at most.
fn short(x: f32) -> String {
    let text = format!("{x:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0" } else { text }.to_string()
}

fn pcm<'a>(play: &Play, sounds: &'a Sounds) -> Option<&'a Arc<[i16]>> {
    sounds.get(play.sound.index()?)?.as_ref()
}

/// How long after its event a play ends, in frames.
fn ends(play: &Play, sounds: &Sounds) -> usize {
    let Some(pcm) = pcm(play, sounds) else {
        return 0;
    };
    let p = play.params.clamped();
    let frames = (pcm.len() / 2) as f64 / f64::from(p.rate);
    (f64::from(p.delay) * RATE + frames).ceil() as usize
}

fn blocks(frames: f64) -> usize {
    (frames / BLOCK as f64).ceil() as usize * BLOCK
}

/// What `takes` sound like through the mixer: each is the frame it starts
/// at (a multiple of `BLOCK`, in order) and what starts then.
fn perform(takes: &[(usize, Vec<Play>)], sounds: &Sounds) -> Vec<i16> {
    let mut mixer = Mixer::new(1.0);
    let mut out = Vec::new();
    let mut block = [0i16; BLOCK * 2];
    let mut next = 0;
    // A 4 s sound and a 1 s delay at most after the last take.
    let last = takes.last().map_or(0, |(start, _)| *start);
    let most = (last + 6 * RATE as usize) * 2;
    while next < takes.len() || (mixer.is_active() && out.len() < most) {
        while next < takes.len() && takes[next].0 <= out.len() / 2 {
            for play in &takes[next].1 {
                if let Some(pcm) = pcm(play, sounds) {
                    mixer.play(pcm.clone(), play.params);
                }
            }
            next += 1;
        }
        mixer.mix(&mut block);
        out.extend_from_slice(&block);
    }
    out
}

/// Takes one after the other, each starting once the one before has
/// ended and rested, and not sooner than `GAP_SECONDS` after it.
fn in_a_row(takes: Vec<Vec<Play>>, sounds: &Sounds) -> Vec<(usize, Vec<Play>)> {
    let longest = takes
        .iter()
        .flatten()
        .map(|play| ends(play, sounds))
        .max()
        .unwrap_or(0);
    let gap = blocks((GAP_SECONDS * RATE).max(longest as f64 + REST_SECONDS * RATE));
    takes
        .into_iter()
        .enumerate()
        .map(|(i, plays)| (i * gap, plays))
        .collect()
}

fn plays_text(plays: &[Play]) -> String {
    let each: Vec<String> = plays
        .iter()
        .map(|p| {
            format!(
                "sound {} (volume {}, rate {}, pan {}, delay {} s)",
                p.sound.0,
                short(p.params.volume),
                short(p.params.rate),
                short(p.params.pan),
                short(p.params.delay)
            )
        })
        .collect();
    match each.len() {
        0 => "nothing".into(),
        _ => each.join(" + "),
    }
}

/// One line of `index.tsv`.
fn row(
    tsv: &mut String,
    file: &str,
    kind: &str,
    name: &str,
    by: &str,
    takes: &[(usize, Vec<Play>)],
) {
    let starts: Vec<String> = takes.iter().map(|(start, _)| start.to_string()).collect();
    let _ = writeln!(tsv, "{file}\t{kind}\t{name}\t{by}\t{}", starts.join(","));
}

/// The number keys of a keypad, by what they show: where each is across
/// the window (three columns of four), and a number for it.
fn digit(d: u32) -> (u32, f32) {
    let column = match d {
        0 => 1,
        d => (d - 1) % 3,
    };
    (d + 1, (column as f32 + 0.5) / 4.0)
}

/// Twenty presses as somebody types a sum: what each plays, the results
/// the app reports with them included.
fn typing(app: &mut StandIn) -> Vec<Vec<Play>> {
    // 12.5 × 4 + 37.99 − 800 ÷ 64 =   (10 is the point.)
    const KEYS: [char; TYPING_PRESSES] = [
        '1', '2', '.', '5', '*', '4', '+', '3', '7', '.', '9', '9', '-', '8', '0', '0', '/', '6',
        '4', '=',
    ];
    let result = stand_in::outcome(EventKind::Result, Page::Calculator);
    KEYS.iter()
        .map(|c| {
            let (event, reported) = match c {
                '=' => (stand_in::key(30, KeyKind::Equals, None, 0.875), true),
                '*' | '+' | '-' | '/' => {
                    let id = 20 + "*+-/".find(*c).unwrap_or(0) as u32;
                    (stand_in::key(id, KeyKind::Operator, None, 0.875), true)
                }
                c => {
                    let (id, across) = digit(c.to_digit(10).unwrap_or(10));
                    (stand_in::key(id, KeyKind::Number, None, across), false)
                }
            };
            let mut plays = app.decide(&event).0;
            if reported {
                plays.extend(app.decide(&result).0);
            }
            plays
        })
        .collect()
}

fn run(theme_path: &Path, out: &Path) -> Result<String, String> {
    let dir: PathBuf = if theme_path.is_dir() {
        theme_path.to_path_buf()
    } else if theme_path.file_name().is_some_and(|n| n == "theme.luau") {
        theme_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf()
    } else {
        return Err(format!(
            "{}: give a theme's folder, or its theme.luau",
            theme_path.display()
        ));
    };
    let theme = Theme::load(Source::folder("dev", &dir), Limits::default())
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let write = |name: &str, samples: &[i16]| {
        let path = out.join(name);
        wav::write(&path, samples).map_err(|e| format!("{}: {e}", path.display()))
    };

    let declared = theme.sounds();
    let mut index = String::new();
    let mut tsv = String::from("file\tkind\tname\tby\ttakes\n");
    let _ = writeln!(index, "{} ({})", theme.def.meta.name, dir.display());
    let _ = writeln!(index, "\nSounds, in the order the theme makes them:");
    let started = std::time::Instant::now();
    let made = prepare_all(&declared.sounds);
    let took = started.elapsed();
    let mut sounds: Vec<Option<Arc<[i16]>>> = Vec::new();
    let mut total = 0;
    for (i, (source, made)) in declared.sounds.iter().zip(made).enumerate() {
        let name = dump_name(i + 1, source);
        match made {
            Ok(samples) => {
                let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
                let _ = writeln!(
                    index,
                    "  {name}  {:7.1} ms  peak {:5.1}%  {}",
                    samples.len() as f64 / 96.0,
                    f64::from(peak) / 327.67,
                    describe(source)
                );
                write(&name, &samples)?;
                row(&mut tsv, &name, "sound", &(i + 1).to_string(), "", &[]);
                total += samples.len() * 2;
                sounds.push((samples.len() >= 2).then(|| samples.into()));
            }
            Err(why) => {
                let _ = writeln!(
                    index,
                    "  {:2}  REFUSED: {why}  ({})",
                    i + 1,
                    describe(source)
                );
                sounds.push(None);
            }
        }
    }
    if declared.sounds.is_empty() {
        let _ = writeln!(index, "  (none)");
    }
    let _ = writeln!(
        index,
        "  {} of {} sounds, {:.1} of {} KiB; made in {:.1} ms",
        declared.sounds.len(),
        limits::SOUNDS,
        total as f64 / 1024.0,
        limits::TOTAL_BYTES / 1024,
        took.as_secs_f64() * 1000.0
    );

    let _ = writeln!(
        index,
        "\nEvents ({TAKES} in a row, on {TAKES} keys from the left of the window to the right):"
    );
    let mut app = StandIn::new(&theme);
    for name in EVENTS {
        let mut takes = Vec::new();
        let mut deciders = Vec::new();
        for take in 0..TAKES {
            let across = take as f32 / (TAKES - 1) as f32;
            let Some(event) = stand_in::named(name, take as u32, across) else {
                continue;
            };
            if matches!(name, EventName::Result | EventName::Error) {
                // The function hears of the key before it hears of what
                // the key came to.
                let equals = stand_in::key(30, KeyKind::Equals, None, 0.875);
                let _ = app.ask(&equals);
            }
            let (plays, by) = app.decide(&event);
            if !deciders.contains(&by) {
                deciders.push(by);
            }
            takes.push(plays);
        }
        let by: Vec<String> = deciders
            .iter()
            .map(|by| match by {
                By::Function => "sound()".to_string(),
                By::Table(found) => format!("sounds.{}", found.name()),
                By::Nobody => "nobody".to_string(),
            })
            .collect();
        let by = by.join(", ");
        if takes.iter().all(Vec::is_empty) {
            let why = match (deciders.as_slice(), declared.table.get(name)) {
                ([By::Function], _) => "plays nothing (the sound function says so)",
                (_, Some(Slot::Silent)) => "plays nothing (false)",
                _ => "plays nothing",
            };
            let _ = writeln!(index, "  {:10} {why}", name.name());
            continue;
        }
        let file = format!("event-{}.wav", name.name());
        let how = match deciders.as_slice() {
            [By::Table(found)] => {
                let entry = declared.table.find(name).map(|(_, e)| e.clone());
                let mut how = String::new();
                if *found != name {
                    let _ = write!(how, "as {}: ", found.name());
                }
                if let Some(entry) = entry {
                    let ids: Vec<String> = entry.sounds.iter().map(|s| s.0.to_string()).collect();
                    let pick = match entry.pick {
                        Pick::Cycle => "cycle",
                        Pick::Random => "random",
                        Pick::Key => "key",
                    };
                    let pan = match entry.pan {
                        Pan::At(v) => varied(v),
                        Pan::Position => "position".into(),
                    };
                    let _ = write!(
                        how,
                        "sound {} (pick {pick}), volume {}, rate {}, pan {pan}, delay {} s",
                        ids.join(", "),
                        varied(entry.volume),
                        varied(entry.rate),
                        entry.delay
                    );
                }
                how
            }
            _ => {
                // What the function chose differs from take to take: the
                // first is written out, and the second if it is of
                // another shape.
                let mut how = format!("by {by}: {}", plays_text(&takes[0]));
                if let Some(other) = takes.iter().find(|t| t.len() != takes[0].len()) {
                    let _ = write!(how, "; then {}", plays_text(other));
                }
                how
            }
        };
        let takes = in_a_row(takes, &sounds);
        write(&file, &perform(&takes, &sounds))?;
        row(&mut tsv, &file, "event", name.name(), &by, &takes);
        let _ = writeln!(index, "  {:10} {file}: {how}", name.name());
    }

    if declared.hook {
        let _ = writeln!(index, "\nOptions the sound function plays something for:");
        let mut any = false;
        for option in &theme.def.options {
            let values: Vec<OptionValue> = match &option.kind {
                OptionKind::Choice { choices, .. } => choices
                    .iter()
                    .map(|c| OptionValue::Text(c.id.clone()))
                    .collect(),
                OptionKind::Toggle => vec![OptionValue::Bool(true), OptionValue::Bool(false)],
                _ => continue,
            };
            for value in values {
                let text = match &value {
                    OptionValue::Text(s) => s.clone(),
                    OptionValue::Number(n) => n.to_string(),
                    OptionValue::Bool(b) => b.to_string(),
                };
                // The app's `ctx` has the new value by the time the theme
                // is told of the change.
                app.set_option(&option.id, value.clone());
                let (plays, _) = app.decide(&stand_in::option(&option.id, value));
                if plays.is_empty() {
                    continue;
                }
                any = true;
                let file = format!("option-{}-{text}.wav", option.id);
                let how = plays_text(&plays);
                let takes = in_a_row(vec![plays], &sounds);
                write(&file, &perform(&takes, &sounds))?;
                let name = format!("{}={text}", option.id);
                row(&mut tsv, &file, "option", &name, "sound()", &takes);
                let _ = writeln!(index, "  {name:18} {file}: {how}");
            }
        }
        if !any {
            let _ = writeln!(index, "  (none)");
        }
    }

    // As somebody types: a fresh stand-in, the options at their defaults.
    let mut typist = StandIn::new(&theme);
    let step = RATE / TYPING_RATE;
    let takes: Vec<(usize, Vec<Play>)> = typing(&mut typist)
        .into_iter()
        .enumerate()
        .map(|(i, plays)| (blocks(i as f64 * step), plays))
        .collect();
    if takes.iter().any(|(_, plays)| !plays.is_empty()) {
        write("typing.wav", &perform(&takes, &sounds))?;
        let name = format!("{TYPING_PRESSES} presses at {TYPING_RATE} a second");
        row(&mut tsv, "typing.wav", "typing", &name, "", &takes[..1]);
        let _ = writeln!(index, "\nTyping:\n  typing.wav: {name}");
    }

    let later = theme
        .sounds_declared()
        .saturating_sub(declared.sounds.len());
    let mut notes: Vec<String> = theme.notes.list.iter().map(|n| n.to_string()).collect();
    notes.extend(theme.take_sound_notes().iter().map(|n| n.to_string()));
    notes.extend(theme.take_log());
    notes.extend(std::mem::take(&mut app.notes));
    notes.extend(std::mem::take(&mut typist.notes));
    if later > 0 {
        notes.push(format!(
            "the sound function made {later} more sound(s) when it was asked: they are not \
             rendered here, and in the app such a sound misses the event that made it"
        ));
    }
    notes.dedup();
    if !notes.is_empty() {
        let _ = writeln!(index, "\nNotes:");
        for note in notes {
            let _ = writeln!(index, "  {note}");
        }
    }
    let path = out.join("index.txt");
    std::fs::write(&path, &index).map_err(|e| format!("{}: {e}", path.display()))?;
    let path = out.join("index.tsv");
    std::fs::write(&path, &tsv).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(index)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [theme, out] = args.as_slice() else {
        eprintln!("usage: render <theme folder | theme.luau> <out folder>");
        return ExitCode::from(2);
    };
    match run(Path::new(theme), Path::new(out)) {
        Ok(index) => {
            print!("{index}");
            println!("\nWritten to {out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("render: {e}");
            ExitCode::FAILURE
        }
    }
}
