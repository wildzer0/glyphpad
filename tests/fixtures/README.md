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
| `four-finger-taps.jsonl` | six four-finger taps | six `tap-4` |
| `three-finger-double-taps.jsonl` | four three-finger double taps | four `double-tap-3` |
| `five-finger-holds.jsonl` | two five-finger holds of about 1 s | two `hold-5` |
| `three-finger-swipes-up.jsonl` | three three-finger swipes up, the first only 23 mm long | three `swipe-3-up` |
| `four-finger-swipes-left.jsonl` | three four-finger swipes left | three `swipe-4-left` |
| `three-finger-L.jsonl` | an L drawn three times with three fingers | used from M3 (symbols) |
