//! Renders a theme's sounds to WAV files, to listen to and to look at in
//! an editor without running the app. Nothing is played: no sound server
//! is contacted and no thread is started.
//!
//!     cargo run -p gmnb-sound --example render -- <theme folder | theme.luau> <out folder>
//!
//! Written to the out folder:
//!
//! - every sound the theme makes, named as `GMNB_SOUND_DUMP` names them
//!   (`03-<hash>.wav`: its order, and a hash of its recipe or file name);
//! - for every name of the theme's `sounds` table, `event-<name>.wav`:
//!   eight of that event in a row, as the mixer would play them, on eight
//!   different keys spread across the window. This is where `pick`, a
//!   `rate` range or `pan = "position"` can be heard;
//! - `index.txt`: what is what, and anything the theme's author should
//!   know.
//!
//! The events are mixed at full volume (the app's Volume setting at 100%);
//! in the app the default is 50%, which is 12 dB quieter.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use gmnb_sound::{
    EventName, Mixer, Pan, Pick, Resolver, Slot, SoundSource, Trigger, Varied, dump_name, limits,
    prepare_all, wav,
};
use gmnb_theme::Limits;
use gmnb_theme::theme::{Source, Theme};

/// How many of an event are played in its file, and how far apart.
const TAKES: usize = 8;
const GAP_SECONDS: f64 = 0.28;

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

/// `TAKES` of the event, one every `GAP_SECONDS` (or once the longest of
/// its sounds has ended, if that is longer), through the mixer.
fn perform(name: EventName, resolver: &mut Resolver, sounds: &[Option<Arc<[i16]>>]) -> Vec<i16> {
    let Some((_, entry)) = resolver.table().find(name) else {
        return Vec::new();
    };
    let longest = entry
        .sounds
        .iter()
        .filter_map(|id| sounds.get(id.index()?)?.as_ref())
        .map(|s| s.len() / 2)
        .max()
        .unwrap_or(0);
    // A long sound (a chime) is let ring before the next one starts.
    let gap = ((GAP_SECONDS * 48_000.0) as usize).max(longest.min(48_000) * 3 / 4);
    let mut mixer = Mixer::new(1.0);
    let mut out = Vec::new();
    let mut block = [0i16; 512];
    for take in 0..TAKES {
        let trigger = Trigger {
            key: Some(take as u64),
            across: Some(take as f32 / (TAKES - 1) as f32),
        };
        if let Some(play) = resolver.resolve(name, trigger)
            && let Some(Some(pcm)) = play.sound.index().and_then(|i| sounds.get(i))
        {
            mixer.play(pcm.clone(), play.params);
        }
        for _ in 0..gap / 256 {
            mixer.mix(&mut block);
            out.extend_from_slice(&block);
        }
    }
    // Let the last one end, a 4 s sound and a 1 s delay at most.
    while mixer.is_active() && out.len() < TAKES * gap * 2 + 6 * 96_000 {
        mixer.mix(&mut block);
        out.extend_from_slice(&block);
    }
    out
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
    let _ = writeln!(index, "{} ({})", theme.def.meta.name, dir.display());
    let _ = writeln!(index, "\nSounds, in the order the theme makes them:");
    let made = prepare_all(&declared.sounds);
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
        "  {} of {} sounds, {:.1} of {} KiB",
        declared.sounds.len(),
        limits::SOUNDS,
        total as f64 / 1024.0,
        limits::TOTAL_BYTES / 1024
    );

    let _ = writeln!(
        index,
        "\nEvents ({TAKES} in a row, on {TAKES} keys from the left of the window to the right):"
    );
    let mut resolver = Resolver::new(declared.table.clone(), 1);
    for name in EventName::ALL {
        if declared.table.get(name) == Some(&Slot::Silent) {
            let _ = writeln!(index, "  {:10} plays nothing (false)", name.name());
            continue;
        }
        let Some((found, entry)) = declared.table.find(name) else {
            continue;
        };
        // Only the names the theme wrote: what falls back to another name
        // sounds as that one does.
        if found != name {
            let _ = writeln!(index, "  {:10} as {}", name.name(), found.name());
            continue;
        }
        let entry = entry.clone();
        let file = format!("event-{}.wav", name.name());
        write(&file, &perform(name, &mut resolver, &sounds))?;
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
        let _ = writeln!(
            index,
            "  {:10} {file}: sound {} (pick {pick}), volume {}, rate {}, pan {pan}, delay {} s",
            name.name(),
            ids.join(", "),
            varied(entry.volume),
            varied(entry.rate),
            entry.delay
        );
    }
    if declared.table.is_empty() {
        let _ = writeln!(index, "  (the theme has no `sounds` table)");
    }
    if declared.hook {
        let _ = writeln!(
            index,
            "  The theme has a `sound` function: what it decides is not rendered here."
        );
    }

    let mut notes: Vec<String> = theme.notes.list.iter().map(|n| n.to_string()).collect();
    notes.extend(theme.take_sound_notes().iter().map(|n| n.to_string()));
    notes.extend(theme.take_log());
    if !notes.is_empty() {
        let _ = writeln!(index, "\nNotes:");
        for note in notes {
            let _ = writeln!(index, "  {note}");
        }
    }
    let path = out.join("index.txt");
    std::fs::write(&path, &index).map_err(|e| format!("{}: {e}", path.display()))?;
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
