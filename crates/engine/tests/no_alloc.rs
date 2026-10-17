// SPDX-License-Identifier: AGPL-3.0-or-later

//! The audio thread must never allocate or free memory: a global allocator
//! counts every allocation made while a thread is marked as real-time, and
//! the mixer is driven through everything it does (crossfades, skips, pauses,
//! the FX player, underruns, queue changes) while marked.

// A global allocator can only be written with `unsafe`.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use heron_core::FadeCurve;
use heron_engine::mixer::{
    AudioSource, BLOCK_FRAMES, CHANNELS, Command, FrameRamp, FrameTransition, LoadedTrack, Mixer,
    VecSource,
};

struct CountingAllocator;

static REALTIME_ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static REALTIME: Cell<bool> = const { Cell::new(false) };
}

fn count_if_realtime() {
    // `try_with`: the thread-local may already be gone while a thread exits.
    if REALTIME.try_with(Cell::get).unwrap_or(false) {
        REALTIME_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_if_realtime();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count_if_realtime();
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_if_realtime();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_if_realtime();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Runs `f` as the audio thread would and returns how many allocations and
/// frees it made.
fn realtime<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = REALTIME_ALLOCATIONS.load(Ordering::Relaxed);
    REALTIME.with(|flag| flag.set(true));
    let result = f();
    REALTIME.with(|flag| flag.set(false));
    (
        result,
        REALTIME_ALLOCATIONS.load(Ordering::Relaxed) - before,
    )
}

#[test]
fn the_detector_catches_allocations() {
    let (vec, allocations) = realtime(|| vec![0_u8; 16]);
    assert!(allocations > 0);
    drop(vec);
}

fn track(entry_id: u64, frames: usize) -> Box<LoadedTrack> {
    let samples = (0..frames * CHANNELS)
        .map(|n| (n as f32 * 0.01).sin() * 0.5)
        .collect();
    Box::new(LoadedTrack {
        entry_id,
        source: Box::new(VecSource::new(samples)),
        length: frames as u64,
        gain: 0.8,
        fade_in: Some(FrameRamp {
            start: 0,
            length: 4_800,
            curve: FadeCurve::SCurve,
            level: 0.0,
        }),
    })
}

/// A source that has run dry without being finished.
struct Starved;

impl AudioSource for Starved {
    fn read(&mut self, _out: &mut [f32]) -> usize {
        0
    }

    fn is_finished(&self) -> bool {
        false
    }
}

#[test]
fn the_mixer_does_not_allocate() {
    let (mut mixer, mut handle) = Mixer::new(BLOCK_FRAMES);
    let mut block = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
    let crossfade = FrameTransition {
        next_start: 30_000,
        fade_out: Some(FrameRamp {
            start: 30_000,
            length: 12_000,
            curve: FadeCurve::Exponential,
            level: 0.0,
        }),
    };

    // Every command is prepared (and allocated) outside the audio thread.
    let mut script: Vec<(usize, Command)> = vec![
        (
            0,
            Command::Cue {
                track: track(1, 48_000),
                transition: crossfade,
            },
        ),
        (
            10,
            Command::Cue {
                track: track(2, 48_000),
                transition: crossfade,
            },
        ),
        // Replaces the cued track.
        (
            20,
            Command::Cue {
                track: track(3, 48_000),
                transition: crossfade,
            },
        ),
        (40, Command::PlayFx(track(4, 9_600))),
        (45, Command::PlayFx(track(5, 9_600))),
        (50, Command::StopFx),
        (
            110,
            Command::Cue {
                track: track(6, 96_000),
                transition: FrameTransition::back_to_back(),
            },
        ),
        (
            120,
            Command::Skip {
                fade_frames: 2_400,
                curve: FadeCurve::Logarithmic,
            },
        ),
        (130, Command::Pause),
        (150, Command::Resume),
        (
            160,
            Command::Cue {
                track: Box::new(LoadedTrack {
                    entry_id: 7,
                    source: Box::new(Starved),
                    length: 48_000,
                    gain: 1.0,
                    fade_in: None,
                }),
                transition: FrameTransition::back_to_back(),
            },
        ),
        (170, Command::Uncue),
    ];
    script.reverse();

    let mut total = 0;
    for index in 0..400 {
        while script.last().is_some_and(|(at, _)| *at == index) {
            if let Some((_, command)) = script.pop() {
                assert!(handle.commands.push(command).is_ok());
            }
        }
        let ((), allocations) = realtime(|| mixer.render(&mut block));
        total += allocations;
        // The control thread's side: events are read, tracks dropped here.
        while handle.events.pop().is_ok() {}
        while let Ok(track) = handle.trash.pop() {
            drop(track);
        }
    }
    assert!(script.is_empty());
    assert_eq!(total, 0, "the audio thread allocated or freed memory");
}

#[test]
fn reading_decoded_tracks_does_not_allocate() {
    use heron_engine::decoder::DecodedSource;
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("track.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for n in 0..44_100 * 2 {
        writer.write_sample(((n % 200) * 100) as i16).unwrap();
    }
    writer.finalize().unwrap();

    let source = DecodedSource::spawn(path, 100, 900).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while source.buffered_frames() < 800 * 48 {
        assert!(Instant::now() < deadline, "decoding is too slow");
        std::thread::sleep(Duration::from_millis(5));
    }

    let (mut mixer, mut handle) = Mixer::new(BLOCK_FRAMES);
    let track = Box::new(LoadedTrack {
        entry_id: 1,
        source: Box::new(source),
        length: 800 * 48,
        gain: 1.0,
        fade_in: None,
    });
    assert!(
        handle
            .commands
            .push(Command::Cue {
                track,
                transition: FrameTransition::back_to_back(),
            })
            .is_ok()
    );
    let mut block = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
    let mut total = 0;
    for _ in 0..100 {
        let ((), allocations) = realtime(|| mixer.render(&mut block));
        total += allocations;
    }
    assert_eq!(total, 0, "the audio thread allocated or freed memory");
    // The track played to its end and was handed back.
    assert!(handle.trash.pop().is_ok());
}
