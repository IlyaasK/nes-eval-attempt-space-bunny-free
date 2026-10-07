# NES Eval attempt: Space Bunny Free (opencode, medium thinking, 2 hours)

One run of [NES Eval](https://github.com/IlyaasK/nes-eval): a model gets an
empty Rust project, the NES Eval task, and a black-box reference emulator,
and has to write an NES emulator that compiles to WebAssembly. This
repository is the model's working directory, saved every 15 minutes, plus
the transcript and the grades. It follows the layout of GBA Eval's attempt
repositories (e.g.
[gba-eval-attempt-claude-opus-5](https://github.com/mechanize-work/gba-eval-attempt-claude-opus-5)).

| | |
|---|---|
| Model | `opencode/space-bunny-free` (OpenCode Zen, free tier) |
| Thinking | `--variant medium` |
| Agent harness | opencode 1.18.35, `opencode run --auto --format json` |
| Environment | NES Eval `quickstart/` task container (commit `f74eee9`) |
| Run length | 2 hours, 2026-10-07 04:18:56 to 06:18:56 UTC |
| Saves | every 15 minutes (`supervisor checkpoint …` commits) |
| Grades | at 1 hour and at the end |

## Score

| Checkpoint | Overall | Procedural | Replay | Audio |
|---|---|---|---|---|
| 1 hour | 0.0578 | 0.0000 | 0.0072 | 0.2674 |
| Final (2 hours) | **0.0594** | 0.0000 | 0.0098 | 0.2673 |

For scale, on the same grader: the reference scores 1.0000, NES Eval's
hand-written baseline `nes-core` 0.5811, and a blank screen 0.0039.

## What happened

The model wrote 3,538 lines of Rust (`src/`: CPU, PPU, APU, mappers 0–4,
the ABI) plus its own tools: a native frame dumper (`examples/dump.rs`) and
scripts that compare its output with the oracle's (`tools/`). It called the
oracle 78 times and kept reference dumps for 39 test ROMs and games (the
`ref_*` directories).

- **The picture is mirrored.** In the final build, every 8-pixel tile is
  drawn right to left: nestest's menu reads "stset llA nuR". At 1h24m the
  model found that its background shift registers moved the wrong way
  ("I had the direction backwards!") and changed them. Before that fix the
  1-hour build drew almost no background at all; after it, text appears
  but each tile is flipped. A tile-level mirror breaks every edge the video
  metric compares, so visible progress scored close to nothing (0.0578 to
  0.0594). The end screens are in `grading/final/` (`*.cand.png` is this
  emulator, `*.ref.png` the reference).
- **Input and games.** nestest never starts its tests (every row still
  shows `~~` after the replay presses Start), and Nova the Squirrel ends on
  a flat grey screen. The model's last investigation was why a game's
  nametable and pattern data were missing.
- **Audio** is the only section with real credit (0.27): sound is produced
  and partly matches.

**The run lost its last 25 minutes.** At 05:53:15 UTC (1h34m) OpenCode
Zen's free tier answered one request with "Rate limit exceeded. Please try
again later." opencode logged the error and then waited without retrying
or exiting, so NES Eval's supervisor, which restarts an agent only when its
process ends, never restarted it. The model's effective working time was
1h34m. This is a harness limitation, not something the model did.

## Files

- `src/`, `examples/`, `tools/`, `Cargo.toml`: the model's emulator and
  tools, as of its last save.
- `ref_*`, `aud_*.wav`: reference output the model dumped with the oracle.
- `TASK.md`, `spec/`, `dev-roms/`: the task environment as handed to the
  model (first commit).
- `transcripts/transcript.md`: readable transcript (reasoning, messages,
  every tool call; long outputs cut to 4,000 characters). Rendered with
  NES Eval's `scripts/render-opencode-transcript.py`.
- `transcripts/opencode-session.json.xz`: the complete `opencode export`
  of the session.
- `transcripts/opencode-events.jsonl.xz`: the event stream opencode
  printed during the run.
- `grading/`: the supervisor log and `run.json`, both checkpoints'
  `summary.json` (per-testcase scores), and the final end-screen images.

## Reproduce the grade

```sh
git clone https://github.com/IlyaasK/nes-eval-attempt-space-bunny-free && cd nes-eval-attempt-space-bunny-free
cargo build --release --lib --target wasm32-unknown-unknown
# then, from a NES Eval checkout:
target/release/grader <path>/target/wasm32-unknown-unknown/release/nes_emu.wasm corpus/ out/space-bunny
```

The task prompt reached the model wrapped in literal double quotes (the
first and last characters of the first message are `"`); its content was
otherwise complete.
