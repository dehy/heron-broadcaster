# Audio engine

Rules for `heron-engine`, the real-time playout process. Functional requirements for playout (decks, crossfading, Gap Killer, loudness) are in [Requirements](requirements.md#2-playout).

## Format and clock

- Internal format: **f32, stereo, 48 kHz**, fixed block size.
- **No sound card required.** A real-time pacing clock drives the mixer and the encoders.
- A local monitor output (`cpal`) is optional.

## Real-time safety

- The audio thread **never allocates, locks, does I/O or logs synchronously.**
- Communication with decode threads, the control plane and encoders goes through lock-free ring buffers and message queues.
- Decode ahead into buffers. The mixer only reads audio that is ready.
- **The engine never touches the database in the audio path** ([Locked decisions](decisions.md)).

## Signal quality

- Parameter changes (gain, EQ, DSP) are **smoothed** to avoid clicks.

## Failure handling

Every failure path ends with audio continuing, in this order:

1. next track;
2. fallback directory;
3. generated silence filler, as the very last resort (with an alert).

See [Requirements – Reliability](requirements.md#5-reliability-247) and [M3 – 24/7 reliability](roadmap/m3-reliability.md).

## Testability

- Crossfade rendering must be testable offline: the same mixer code can render a transition to a buffer for comparison against a reference.
