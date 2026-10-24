# Touch fixtures

Real sessions recorded with `glyphpad capture` on the reference hardware
(Amazon Basics touchpad, Telink 248A:8278, 155.8 × 115.2 mm, 5 contacts).
`crates/core/tests/fixtures.rs` replays each one through the engine with the
gestures in `gestures.toml` and checks the recognized gestures listed in
`expected.toml`.

| File | What was done | Expected |
|---|---|---|
| `one-finger-circle.jsonl` | slow circle with one finger | nothing |
| `clicks-and-two-finger-taps.jsonl` | physical clicks with one and two fingers, two light two-finger taps | the two taps only |
| `palm.jsonl` | palm laid on the surface twice; reported as fingers before being flagged | nothing |
| `three-finger-L.jsonl` | an L drawn three times with three fingers | used from M3 (symbols) |
