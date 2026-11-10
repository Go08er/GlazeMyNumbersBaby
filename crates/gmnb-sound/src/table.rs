//! Which sound for which event (docs/theming-media.md, "Which sound for
//! which event" and "From event to sound"): the names an event may go by,
//! a theme's `sounds` table, and how an entry becomes a play (its `pick`,
//! its ranges, `pan = "position"`), with the rule that the same event does
//! not sound twice within 30 ms.
//!
//! No sound is made here; this is arithmetic the app does on its own
//! thread before it calls [`crate::SoundEngine::play`].

use std::time::Instant;

use crate::mixer::PlayParams;
use crate::recipe::limits;

/// A sound of a theme: the number `gmnb.synth` or `gmnb.sound` gave it,
/// from 1. 0 is no sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SoundId(pub u32);

impl SoundId {
    pub const NONE: SoundId = SoundId(0);

    /// Its place in the theme's list of sounds.
    pub fn index(self) -> Option<usize> {
        (self.0 as usize).checked_sub(1)
    }
}

/// The names of the `sounds` table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventName {
    Digit,
    Operator,
    Func,
    Equals,
    Toggle,
    Clear,
    Backspace,
    Memory,
    Button,
    Result,
    Error,
    Mode,
    /// Any keypad key that has no entry of its own.
    Key,
    /// Anything pressed that has no entry of its own.
    Press,
}

impl EventName {
    pub const ALL: [EventName; 14] = [
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
        EventName::Key,
        EventName::Press,
    ];

    /// As a theme writes it.
    pub fn name(self) -> &'static str {
        match self {
            EventName::Digit => "digit",
            EventName::Operator => "operator",
            EventName::Func => "func",
            EventName::Equals => "equals",
            EventName::Toggle => "toggle",
            EventName::Clear => "clear",
            EventName::Backspace => "backspace",
            EventName::Memory => "memory",
            EventName::Button => "button",
            EventName::Result => "result",
            EventName::Error => "error",
            EventName::Mode => "mode",
            EventName::Key => "key",
            EventName::Press => "press",
        }
    }

    pub fn parse(name: &str) -> Option<EventName> {
        EventName::ALL.into_iter().find(|n| n.name() == name)
    }

    /// The names tried for it, in order: itself, then what it falls back
    /// to.
    pub fn row(self) -> &'static [EventName] {
        use EventName::*;
        match self {
            Digit => &[Digit, Key, Press],
            Operator => &[Operator, Key, Press],
            Func => &[Func, Key, Press],
            Equals => &[Equals, Key, Press],
            Toggle => &[Toggle, Key, Press],
            Clear => &[Clear, Key, Press],
            Backspace => &[Backspace, Key, Press],
            Memory => &[Memory, Button, Press],
            Button => &[Button, Press],
            Result => &[Result],
            Error => &[Error],
            Mode => &[Mode],
            Key => &[Key, Press],
            Press => &[Press],
        }
    }

    /// The name of a `press` event: its `action` if it has one (`"clear"`,
    /// `"backspace"`, `"memory"`); else, for a key, by its `key.kind`
    /// (`"number"` is `digit`; `"other"` and anything unknown are `func`);
    /// else `button`.
    pub fn of_press(action: Option<&str>, key_kind: Option<&str>) -> EventName {
        match (action, key_kind) {
            (Some("clear"), _) => EventName::Clear,
            (Some("backspace"), _) => EventName::Backspace,
            (Some("memory"), _) => EventName::Memory,
            (_, Some("number")) => EventName::Digit,
            (_, Some("operator")) => EventName::Operator,
            (_, Some("equals")) => EventName::Equals,
            (_, Some("toggle")) => EventName::Toggle,
            (_, Some(_)) => EventName::Func,
            (_, None) => EventName::Button,
        }
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// A value, or a range to pick from for each play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Varied {
    One(f32),
    Range(f32, f32),
}

/// How an entry chooses among its sounds and within its ranges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pick {
    /// The list in turn; ranges at random.
    #[default]
    Cycle,
    /// Both at random.
    Random,
    /// By the key: the same key always sounds the same. (An event with no
    /// key cycles.)
    Key,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pan {
    At(Varied),
    /// Where the control is across the window, at 60% width.
    Position,
}

/// One entry of the `sounds` table.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// 1 to 8 sounds to choose from.
    pub sounds: Vec<SoundId>,
    pub pick: Pick,
    /// 0 to 1.
    pub volume: Varied,
    /// 0.5 to 2.
    pub rate: Varied,
    pub pan: Pan,
    /// Seconds, 0 to 1.
    pub delay: f32,
}

impl Entry {
    /// `sound` played as it is.
    pub fn plain(sound: SoundId) -> Entry {
        Entry {
            sounds: vec![sound],
            pick: Pick::Cycle,
            volume: Varied::One(1.0),
            rate: Varied::One(1.0),
            pan: Pan::At(Varied::One(0.0)),
            delay: 0.0,
        }
    }
}

/// What the table holds for a name.
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    /// `false`: plays nothing, and the search stops here.
    Silent,
    Entry(Entry),
}

/// A theme's `sounds` table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundTable {
    slots: [Option<Slot>; 14],
}

impl SoundTable {
    pub fn set(&mut self, name: EventName, slot: Slot) {
        self.slots[name.slot()] = Some(slot);
    }

    pub fn get(&self, name: EventName) -> Option<&Slot> {
        self.slots[name.slot()].as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// The entry an event of this name plays: the first of its row that
    /// the table has, with the name it was found under. `None` if there
    /// is none, or if that one is `false`.
    pub fn find(&self, name: EventName) -> Option<(EventName, &Entry)> {
        for n in name.row() {
            match self.get(*n) {
                Some(Slot::Entry(e)) => return Some((*n, e)),
                Some(Slot::Silent) => return None,
                None => {}
            }
        }
        None
    }
}

/// One sound, played one way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Play {
    pub sound: SoundId,
    pub params: PlayParams,
}

/// What an event is, as far as choosing its sound goes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trigger {
    /// The key's `id`, for a press on a key.
    pub key: Option<u64>,
    /// Where it happened across the window: 0 at the left edge to 1 at
    /// the right.
    pub across: Option<f32>,
}

fn xorshift(s: &mut u32) -> u32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    *s
}

/// FNV-1a over the key's id and which field is being chosen, then mixed
/// (the low bits choose among a few sounds, so every bit should depend on
/// the key).
fn key_hash(key: u64, field: u8) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in key.to_le_bytes().into_iter().chain([field]) {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// Turns events into plays for one loaded theme: it holds the theme's
/// table, each entry's place in its cycle, the random numbers, and when
/// each name last sounded.
#[derive(Clone, Debug)]
pub struct Resolver {
    table: SoundTable,
    /// How often each entry has been played (by the name it is under).
    turns: [u32; 14],
    random: u32,
    /// When each name last sounded; the last place is for events that
    /// have no name.
    last: [Option<Instant>; 15],
}

impl Resolver {
    /// `seed` starts the random numbers, once per theme load (0 is taken
    /// as a fixed other number).
    pub fn new(table: SoundTable, seed: u32) -> Resolver {
        Resolver {
            table,
            turns: [0; 14],
            random: if seed == 0 { 0x9e37_79b9 } else { seed },
            last: [None; 15],
        }
    }

    pub fn table(&self) -> &SoundTable {
        &self.table
    }

    /// Whether an event of this name (`None`: one that has none, an
    /// `option`) sounded less than 30 ms before `now` and is to be
    /// dropped. If not, `now` is noted as its time.
    pub fn too_soon(&mut self, name: Option<EventName>, now: Instant) -> bool {
        let last = &mut self.last[name.map_or(14, EventName::slot)];
        if last.is_some_and(|t| now.saturating_duration_since(t) < limits::REPEAT) {
            return true;
        }
        *last = Some(now);
        false
    }

    /// What the table plays for an event of this name: nothing, or one
    /// sound with its volume, rate, pan and delay chosen.
    pub fn resolve(&mut self, name: EventName, trigger: Trigger) -> Option<Play> {
        let (found, entry) = self.table.find(name)?;
        let entry = entry.clone();
        let n = entry.sounds.len() as u32;
        if n == 0 {
            return None;
        }
        // Without a key, "key" cycles.
        let key = trigger.key.filter(|_| entry.pick == Pick::Key);
        let turn = &mut self.turns[found.slot()];
        let which = match (entry.pick, key) {
            (_, Some(key)) => key_hash(key, 0) % n,
            (Pick::Random, None) => xorshift(&mut self.random) % n,
            (Pick::Cycle | Pick::Key, None) => {
                let which = *turn % n;
                *turn = turn.wrapping_add(1);
                which
            }
        };
        let mut vary = |v: Varied, field: u8| match v {
            Varied::One(x) => x,
            Varied::Range(low, high) => {
                let r = match key {
                    Some(key) => key_hash(key, field),
                    None => xorshift(&mut self.random),
                };
                low + (high - low) * (r as f32 / 4_294_967_296.0)
            }
        };
        let volume = vary(entry.volume, 1);
        let rate = vary(entry.rate, 2);
        let pan = match entry.pan {
            Pan::At(v) => vary(v, 3),
            Pan::Position => trigger.across.map_or(0.0, |x| (2.0 * x - 1.0) * 0.6),
        };
        Some(Play {
            sound: entry.sounds[which as usize],
            params: PlayParams {
                volume,
                rate,
                pan,
                delay: entry.delay,
            }
            .clamped(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn entry(sounds: &[u32]) -> Entry {
        Entry {
            sounds: sounds.iter().map(|s| SoundId(*s)).collect(),
            ..Entry::plain(SoundId(1))
        }
    }

    #[test]
    fn names_fall_back_along_their_rows() {
        let mut t = SoundTable::default();
        assert!(t.is_empty());
        t.set(EventName::Press, Slot::Entry(entry(&[1])));
        t.set(EventName::Key, Slot::Entry(entry(&[2])));
        t.set(EventName::Digit, Slot::Entry(entry(&[3])));
        t.set(EventName::Equals, Slot::Silent);
        t.set(EventName::Button, Slot::Entry(entry(&[4])));
        let found = |name| t.find(name).map(|(n, e)| (n, e.sounds[0].0));
        assert_eq!(found(EventName::Digit), Some((EventName::Digit, 3)));
        assert_eq!(found(EventName::Operator), Some((EventName::Key, 2)));
        assert_eq!(found(EventName::Backspace), Some((EventName::Key, 2)));
        // `false` stops the search, though `key` and `press` are set.
        assert_eq!(found(EventName::Equals), None);
        assert_eq!(found(EventName::Memory), Some((EventName::Button, 4)));
        assert_eq!(found(EventName::Button), Some((EventName::Button, 4)));
        // These three are themselves or nothing.
        assert_eq!(found(EventName::Result), None);
        assert_eq!(found(EventName::Error), None);
        assert_eq!(found(EventName::Mode), None);
        for name in EventName::ALL {
            assert_eq!(EventName::parse(name.name()), Some(name));
            assert_eq!(name.row()[0], name);
        }
        assert_eq!(EventName::parse("option"), None);
    }

    #[test]
    fn a_press_gets_its_name() {
        use EventName::*;
        assert_eq!(EventName::of_press(Some("clear"), Some("func")), Clear);
        assert_eq!(EventName::of_press(Some("backspace"), None), Backspace);
        assert_eq!(EventName::of_press(Some("memory"), None), Memory);
        assert_eq!(EventName::of_press(None, Some("number")), Digit);
        assert_eq!(EventName::of_press(None, Some("operator")), Operator);
        assert_eq!(EventName::of_press(None, Some("func")), Func);
        assert_eq!(EventName::of_press(None, Some("equals")), Equals);
        assert_eq!(EventName::of_press(None, Some("toggle")), Toggle);
        assert_eq!(EventName::of_press(None, Some("other")), Func);
        assert_eq!(EventName::of_press(None, None), Button);
        assert_eq!(EventName::of_press(Some("else"), None), Button);
    }

    #[test]
    fn cycle_takes_the_list_in_turn_per_entry() {
        let mut t = SoundTable::default();
        t.set(EventName::Key, Slot::Entry(entry(&[1, 2, 3])));
        t.set(EventName::Result, Slot::Entry(entry(&[7, 8])));
        let mut r = Resolver::new(t, 1);
        let mut sound = |name| r.resolve(name, Trigger::default()).unwrap().sound.0;
        // Digit and operator share `key`'s entry, and so its turn.
        assert_eq!(sound(EventName::Digit), 1);
        assert_eq!(sound(EventName::Operator), 2);
        assert_eq!(sound(EventName::Result), 7);
        assert_eq!(sound(EventName::Digit), 3);
        assert_eq!(sound(EventName::Digit), 1);
        assert_eq!(sound(EventName::Result), 8);
    }

    #[test]
    fn key_gives_each_key_its_own_sound_and_pitch() {
        let mut t = SoundTable::default();
        t.set(
            EventName::Key,
            Slot::Entry(Entry {
                pick: Pick::Key,
                rate: Varied::Range(0.9, 1.1),
                pan: Pan::At(Varied::Range(-0.5, 0.5)),
                volume: Varied::Range(0.5, 1.0),
                ..entry(&[1, 2, 3, 4])
            }),
        );
        let mut r = Resolver::new(t.clone(), 1);
        let mut other = Resolver::new(t, 999);
        let key = |k| Trigger {
            key: Some(k),
            across: None,
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut rates = Vec::new();
        for k in 0..40u64 {
            let a = r.resolve(EventName::Digit, key(k)).unwrap();
            // The same key again, and in another run: the same play.
            assert_eq!(r.resolve(EventName::Digit, key(k)).unwrap(), a);
            assert_eq!(other.resolve(EventName::Digit, key(k)).unwrap(), a);
            assert!((0.9..=1.1).contains(&a.params.rate));
            assert!((-0.5..=0.5).contains(&a.params.pan));
            assert!((0.5..=1.0).contains(&a.params.volume));
            seen.insert(a.sound.0);
            rates.push(a.params.rate);
        }
        assert_eq!(seen.len(), 4, "forty keys use all four sounds");
        rates.sort_by(f32::total_cmp);
        rates.dedup();
        assert!(rates.len() > 30, "keys differ in pitch");
        // The fields don't move together: rate and pan use other hashes.
        let a = r.resolve(EventName::Digit, key(5)).unwrap().params;
        let b = r.resolve(EventName::Digit, key(6)).unwrap().params;
        let (dr, dp) = ((a.rate - b.rate) / 0.2, (a.pan - b.pan) / 1.0);
        assert!((dr - dp).abs() > 1e-3);
        // No key: it cycles.
        let s: Vec<u32> = (0..5)
            .map(|_| {
                r.resolve(EventName::Digit, Trigger::default())
                    .unwrap()
                    .sound
                    .0
            })
            .collect();
        assert_eq!(s, [1, 2, 3, 4, 1]);
    }

    #[test]
    fn random_stays_in_range_and_varies() {
        let mut t = SoundTable::default();
        t.set(
            EventName::Press,
            Slot::Entry(Entry {
                pick: Pick::Random,
                rate: Varied::Range(0.97, 1.03),
                ..entry(&[1, 2, 3])
            }),
        );
        let mut r = Resolver::new(t.clone(), 7);
        let mut same_seed = Resolver::new(t, 7);
        let mut counts = [0; 4];
        let mut rates = std::collections::BTreeSet::new();
        for _ in 0..300 {
            let p = r.resolve(EventName::Button, Trigger::default()).unwrap();
            assert_eq!(
                same_seed.resolve(EventName::Button, Trigger::default()),
                Some(p)
            );
            assert!((0.97..=1.03).contains(&p.params.rate));
            counts[p.sound.0 as usize] += 1;
            rates.insert(p.params.rate.to_bits());
        }
        assert!(
            counts[1] > 60 && counts[2] > 60 && counts[3] > 60,
            "{counts:?}"
        );
        assert!(rates.len() > 250);
    }

    #[test]
    fn position_pans_at_sixty_percent() {
        let mut t = SoundTable::default();
        t.set(
            EventName::Key,
            Slot::Entry(Entry {
                pan: Pan::Position,
                ..entry(&[1])
            }),
        );
        let mut r = Resolver::new(t, 1);
        let mut pan = |across| {
            let t = Trigger { key: None, across };
            r.resolve(EventName::Digit, t).unwrap().params.pan
        };
        assert!((pan(Some(0.0)) + 0.6).abs() < 1e-6);
        assert!(pan(Some(0.5)).abs() < 1e-6);
        assert!((pan(Some(1.0)) - 0.6).abs() < 1e-6);
        assert_eq!(pan(None), 0.0);
        // Outside the window is still inside a play's limits.
        assert_eq!(pan(Some(9.0)), 1.0);
    }

    #[test]
    fn the_same_event_does_not_sound_twice_in_thirty_milliseconds() {
        let mut r = Resolver::new(SoundTable::default(), 1);
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        assert!(!r.too_soon(Some(EventName::Digit), t0));
        assert!(r.too_soon(Some(EventName::Digit), t0 + ms(10)));
        assert!(r.too_soon(Some(EventName::Digit), t0 + ms(29)));
        // Another name is another matter.
        assert!(!r.too_soon(Some(EventName::Result), t0 + ms(10)));
        assert!(!r.too_soon(None, t0 + ms(10)));
        assert!(r.too_soon(None, t0 + ms(20)));
        // Measured from the last one that sounded, not the last one tried:
        // a held key rattles every 30 ms.
        assert!(!r.too_soon(Some(EventName::Digit), t0 + ms(30)));
        assert!(r.too_soon(Some(EventName::Digit), t0 + ms(59)));
        assert!(!r.too_soon(Some(EventName::Digit), t0 + ms(60)));
    }
}
