//! Sound files: every kind the chapter lists is read, every limit is hit
//! on purpose, and files that are cut short, mislabelled or lie about
//! their size are refused without a panic.

use std::io::Cursor;

use gmnb_sound::file::{Kind, decode, sniff};
use gmnb_sound::{FileData, SoundSource, limits, prepare_all};

const TONE_MONO: &[u8] = include_bytes!("data/tone-mono-44100.ogg");
const TONE_STEREO: &[u8] = include_bytes!("data/tone-stereo-48000.ogg");
const LONG: &[u8] = include_bytes!("data/long-mono-8000.ogg");
const THREE: &[u8] = include_bytes!("data/three-channels.ogg");

/// A WAV file of `frames` frames: channel `c`'s sample `i` is
/// `level(i, c)`, from -1 to 1.
fn wav(
    channels: u16,
    rate: u32,
    bits: u16,
    float: bool,
    frames: usize,
    level: impl Fn(usize, u16) -> f64,
) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: if float {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    };
    let mut out = Cursor::new(Vec::new());
    let mut w = hound::WavWriter::new(&mut out, spec).unwrap();
    for i in 0..frames {
        for c in 0..channels {
            let v = level(i, c);
            if float {
                w.write_sample(v as f32).unwrap();
            } else {
                let full = f64::from(1u32 << (bits - 1)) - 1.0;
                w.write_sample((v * full).round() as i32).unwrap();
            }
        }
    }
    w.finalize().unwrap();
    out.into_inner()
}

fn tone(hz: f64, rate: u32) -> impl Fn(usize, u16) -> f64 {
    move |i, _| 0.5 * (std::f64::consts::TAU * hz * i as f64 / f64::from(rate)).sin()
}

fn peak(samples: &[i16]) -> u16 {
    samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0)
}

/// How many times the left channel goes from below zero to above.
fn rises(samples: &[i16]) -> usize {
    let left: Vec<i16> = samples.iter().step_by(2).copied().collect();
    left.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count()
}

#[test]
fn a_file_is_what_its_first_bytes_say() {
    assert_eq!(
        sniff(&wav(1, 8000, 16, false, 4, |_, _| 0.0)),
        Some(Kind::Wav)
    );
    assert_eq!(sniff(TONE_MONO), Some(Kind::OggVorbis));
    assert_eq!(sniff(b"RIFF\0\0\0\0AVI LIST"), None);
    assert_eq!(sniff(b"\x89PNG\r\n\x1a\n"), None);
    assert_eq!(sniff(b"fLaC"), None);
    assert_eq!(sniff(b"ID3\x03"), None);
    assert_eq!(sniff(b""), None);
    assert_eq!(sniff(b"RIF"), None);
    for bad in [&b"\x89PNG\r\n\x1a\n...."[..], b"", b"OggX", b"RIFF"] {
        let e = decode(bad, None).unwrap_err();
        assert!(e.contains("neither a WAV nor an Ogg Vorbis"), "{e}");
    }
}

#[test]
fn sixteen_bit_stereo_at_48_khz_plays_as_recorded() {
    let file = wav(2, 48_000, 16, false, 1000, |i, c| {
        (i as f64 / 1000.0 - 0.5) * if c == 0 { 1.0 } else { -0.5 }
    });
    let samples = decode(&file, None).unwrap();
    assert_eq!(samples.len(), 2000);
    for (i, frame) in samples.as_chunks::<2>().0.iter().enumerate() {
        let l = ((i as f64 / 1000.0 - 0.5) * 32767.0).round() as i16;
        let r = ((i as f64 / 1000.0 - 0.5) * -0.5 * 32767.0).round() as i16;
        assert_eq!(*frame, [l, r], "frame {i}");
    }
}

#[test]
fn every_kind_of_wav_is_read() {
    for (bits, float) in [
        (8, false),
        (16, false),
        (24, false),
        (32, false),
        (32, true),
    ] {
        for channels in [1, 2] {
            for rate in [8_000, 22_050, 44_100, 48_000, 96_000] {
                let frames = rate as usize / 10;
                let file = wav(channels, rate, bits, float, frames, tone(440.0, rate));
                let what = format!("{bits}-bit float={float} {channels}ch {rate} Hz");
                let samples = decode(&file, None).unwrap_or_else(|e| panic!("{what}: {e}"));
                // A tenth of a second, at the mixer's rate, in stereo.
                assert_eq!(samples.len(), 4800 * 2, "{what}");
                // 440 Hz for 0.1 s is 44 periods, at half of full scale.
                let n = rises(&samples);
                assert!((42..=45).contains(&n), "{what}: {n} periods");
                let p = peak(&samples);
                assert!((15_900..=16_700).contains(&p), "{what}: peak {p}");
                // Mono is copied to both channels.
                if channels == 1 {
                    assert!(
                        samples.as_chunks::<2>().0.iter().all(|f| f[0] == f[1]),
                        "{what}"
                    );
                }
            }
        }
    }
}

#[test]
fn gain_scales_to_that_peak() {
    let file = wav(1, 44_100, 16, false, 4410, tone(1000.0, 44_100));
    for (gain, want) in [
        (0.5, 16_384),
        (1.0, 32_767),
        (0.1, 3277),
        (0.0, 0),
        (7.0, 32_767),
    ] {
        assert_eq!(
            peak(&decode(&file, Some(gain)).unwrap()),
            want,
            "gain {gain}"
        );
    }
    assert_eq!(peak(&decode(&file, Some(f64::NAN)).unwrap()), 0);
    // A silent file stays silent, whatever the gain.
    let silent = wav(2, 48_000, 16, false, 100, |_, _| 0.0);
    assert_eq!(peak(&decode(&silent, Some(1.0)).unwrap()), 0);
}

#[test]
fn ogg_vorbis_is_read() {
    // 0.25 s of 440 Hz, mono, at 44.1 kHz.
    let samples = decode(TONE_MONO, None).unwrap();
    assert_eq!(samples.len(), 12_000 * 2);
    assert!(samples.as_chunks::<2>().0.iter().all(|f| f[0] == f[1]));
    let n = rises(&samples);
    assert!((108..=111).contains(&n), "{n} periods");
    // ffmpeg's sine is an eighth of full scale.
    let p = peak(&samples);
    assert!((3900..=4300).contains(&p), "peak {p}");
    // 0.1 s of 660 Hz in stereo at 48 kHz: no resampling.
    let samples = decode(TONE_STEREO, None).unwrap();
    assert_eq!(samples.len(), 4800 * 2);
    let n = rises(&samples);
    assert!((64..=67).contains(&n), "{n} periods");
    assert_eq!(peak(&decode(TONE_STEREO, Some(0.25)).unwrap()), 8192);
}

#[test]
fn more_than_two_channels_are_refused() {
    let e = decode(&wav(3, 48_000, 16, false, 10, |_, _| 0.0), None).unwrap_err();
    assert!(e.contains("3 channels"), "{e}");
    let e = decode(&wav(6, 48_000, 16, false, 10, |_, _| 0.0), None).unwrap_err();
    assert!(e.contains("6 channels"), "{e}");
    let e = decode(THREE, None).unwrap_err();
    assert!(e.contains("3 channels"), "{e}");
}

#[test]
fn rates_outside_8_to_96_khz_are_refused() {
    for rate in [7_999, 4_000, 96_001, 192_000] {
        let e = decode(&wav(1, rate, 16, false, 10, |_, _| 0.0), None).unwrap_err();
        assert!(e.contains("samples a second"), "{rate}: {e}");
    }
    for rate in [8_000, 96_000] {
        assert!(decode(&wav(1, rate, 16, false, 10, |_, _| 0.0), None).is_ok());
    }
}

#[test]
fn more_than_four_seconds_are_refused() {
    // Exactly 4 s is a sound; one frame more is not.
    let four = wav(1, 8_000, 8, false, 32_000, |_, _| 0.1);
    assert_eq!(decode(&four, None).unwrap().len(), limits::FRAMES * 2);
    let over = wav(1, 8_000, 8, false, 32_001, |_, _| 0.1);
    assert_eq!(decode(&over, None).unwrap_err(), "longer than 4 s");
    // 4 s at 44.1 kHz comes to exactly 4 s at 48 kHz.
    let four = wav(1, 44_100, 16, false, 176_400, |_, _| 0.1);
    assert_eq!(decode(&four, None).unwrap().len(), limits::FRAMES * 2);
    // Vorbis does not say how long it is: it is stopped as it is decoded.
    assert_eq!(decode(LONG, None).unwrap_err(), "longer than 4 s");
}

#[test]
fn a_file_over_a_mebibyte_is_refused_unread() {
    // A good WAV header, then a mebibyte: it is never parsed.
    let mut big = wav(1, 8_000, 8, false, 100, |_, _| 0.0);
    big.resize((1 << 20) + 1, 0);
    let e = decode(&big, None).unwrap_err();
    assert!(e.contains("at most 1 MiB"), "{e}");
    big.truncate(1 << 20);
    assert!(!decode(&big, None).is_err_and(|e| e.contains("1 MiB")));

    // And from disk, through a theme's sound: the size is looked at before
    // the file is read.
    let dir = std::env::temp_dir().join(format!("gmnb-sound-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("big.wav");
    std::fs::write(&path, vec![0u8; (1 << 20) + 1]).unwrap();
    let good = dir.join("good.wav");
    std::fs::write(&good, wav(2, 48_000, 16, false, 480, tone(440.0, 48_000))).unwrap();
    let file = |name: &str, data: FileData| SoundSource::File {
        name: name.into(),
        data,
        gain: Some(0.5),
    };
    let made = prepare_all(&[
        file("big.wav", FileData::Path(path)),
        file("good.wav", FileData::Path(good)),
        file("gone.wav", FileData::Path(dir.join("gone.wav"))),
        file("dir", FileData::Path(dir.clone())),
        file("built-in.ogg", FileData::Bytes(TONE_STEREO.into())),
    ]);
    assert!(made[0].as_ref().is_err_and(|e| e.contains("at most 1 MiB")));
    assert_eq!(peak(made[1].as_ref().unwrap()), 16_384);
    assert!(made[2].is_err());
    assert!(made[3].is_err());
    assert_eq!(made[4].as_ref().unwrap().len(), 9600);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn other_sample_sizes_are_refused() {
    // hound writes what it is told; 64-bit floats are not on the list.
    let mut file = wav(1, 48_000, 32, true, 8, |_, _| 0.0);
    // The fmt chunk: bits per sample at byte 34.
    file[34] = 64;
    assert!(decode(&file, None).is_err());
    let mut file = wav(1, 48_000, 16, false, 8, |_, _| 0.0);
    file[34] = 12;
    assert!(decode(&file, None).is_err());
}

#[test]
fn a_header_that_lies_about_its_size_costs_nothing() {
    // A data chunk of 4 GiB in a file of a few bytes: refused by its
    // length, before anything is read or set aside for it.
    let mut file = wav(2, 48_000, 16, false, 16, |_, _| 0.25);
    let data = file.windows(4).position(|w| w == b"data").unwrap();
    file[data + 4..data + 8].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
    file[4..8].copy_from_slice(&0xffff_fff8u32.to_le_bytes());
    let started = std::time::Instant::now();
    assert_eq!(decode(&file, None).unwrap_err(), "longer than 4 s");
    assert!(started.elapsed() < std::time::Duration::from_millis(100));
    // One that claims a second and holds a few frames: cut short.
    let mut file = wav(2, 48_000, 16, false, 16, |_, _| 0.25);
    file[data + 4..data + 8].copy_from_slice(&(48_000u32 * 4).to_le_bytes());
    assert!(decode(&file, None).is_err());
    // A fmt chunk that claims to be huge.
    let mut file = wav(1, 48_000, 16, false, 16, |_, _| 0.25);
    file[16..20].copy_from_slice(&0x7fff_ffffu32.to_le_bytes());
    assert!(decode(&file, None).is_err());
}

#[test]
fn files_cut_short_are_refused_or_shorter() {
    let file = wav(2, 44_100, 24, false, 4410, tone(440.0, 44_100));
    for cut in [0, 3, 11, 12, 20, 43, 44, 45, 1000, file.len() - 1] {
        assert!(decode(&file[..cut], None).is_err(), "cut at {cut}");
    }
    // Vorbis has no length to check against: a file that stops early is a
    // shorter sound or an error, never a panic and never more than there
    // was.
    for file in [TONE_MONO, TONE_STEREO] {
        let whole = decode(file, None).unwrap().len();
        for cut in (0..file.len()).step_by(97) {
            if let Ok(samples) = decode(&file[..cut], None) {
                assert!(samples.len() <= whole, "cut at {cut}");
            }
        }
    }
}

/// Every file with a byte changed, a run of bytes zeroed, or its end cut
/// off: read or refused, never a panic, never more than 4 s.
#[test]
fn damaged_files_never_panic() {
    let mut seed = 0x2545_f491u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let files: Vec<Vec<u8>> = vec![
        wav(2, 44_100, 16, false, 300, tone(440.0, 44_100)),
        wav(1, 8_000, 8, false, 300, tone(440.0, 8_000)),
        wav(2, 96_000, 32, true, 300, tone(440.0, 96_000)),
        wav(1, 22_050, 24, false, 300, tone(440.0, 22_050)),
        TONE_MONO.to_vec(),
        TONE_STEREO.to_vec(),
        THREE.to_vec(),
    ];
    let mut read = 0;
    for original in &files {
        for round in 0..400 {
            let mut file = original.clone();
            match round % 4 {
                0 => {
                    let at = next() as usize % file.len();
                    file[at] ^= 1 << (next() % 8);
                }
                1 => {
                    // Headers matter most: aim there half the time.
                    let at = next() as usize % file.len().min(256);
                    file[at] = next() as u8;
                }
                2 => {
                    let at = next() as usize % file.len();
                    let n = (next() as usize % 64).min(file.len() - at);
                    file[at..at + n].fill(if next() % 2 == 0 { 0 } else { 0xff });
                }
                _ => file.truncate(next() as usize % file.len()),
            }
            if let Ok(samples) = decode(&file, Some(0.5)) {
                assert!(samples.len() <= limits::FRAMES * 2);
                assert!(samples.len() % 2 == 0);
                read += 1;
            }
        }
    }
    // The test is of the files that are still read, too.
    assert!(read > 100, "only {read} damaged files were read");
}
