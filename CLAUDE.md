# CLAUDE.md — si

@CONCEPT.md
@SPEC.md

You are implementing si: a minimal editor, written in Rust, that replaces vi.
Before starting any work, always read CONCEPT.md and SPEC.md.

## The pyramid

```
          ┌─────────┐
          │ CONCEPT │  why       one sentence, five principles  CONCEPT.md
        ┌─┴─────────┴─┐
        │    SPEC     │  what      ~50 requirements with IDs     SPEC.md
      ┌─┴─────────────┴─┐
      │      CODE       │  how       at most 3,000 lines       src/
    ┌─┴─────────────────┴─┐
    │        TEST         │  really?   a test for every ID       tests/ and more
    └─────────────────────┘
```

- **Changes flow down.** Don't build what isn't in the spec. When something is needed, first add a line to SPEC.md with the principle it follows. Then implement it
- **Proof flows up.** Every ID in SPEC has at least one test whose name starts with that ID
- **Never change an upper layer for the convenience of a lower one.** Don't loosen the spec to make a test pass. When the spec changes, explain in one line how it fits the principles
- **When in doubt, decide by the priority of the principles** (P1 > P2 > P3 > P4 > P5). Write the decision into SPEC
- **Don't fill ambiguities in the spec with guesses; ask**

## Code

### Structure (functional core, imperative shell)

```
src/
  main.rs         the event loop, running Effects (reading files, saving, the clipboard)
  lib.rs          exposes core/ and io/ to main.rs and tests/
  core/           pure. Never touches std::fs, std::process, clocks or the terminal
    cli.rs        the arguments of `si [+cmd] [file]` → the file and the keys to type first
    app.rs        App::update(&mut self, Event) -> Vec<Effect>
    buffer.rs     ropey and undo
    vi.rs         key sequence → Command (Normal and Visual mode, the command line's editing)
    edit.rs       applies a Command to the Buffer: motions, operators, text objects, Insert mode
    ex.rs         ex commands and their ranges, `:s`, search
    proposal.rs   diff3, word-level diff, Proposal and Hunk
    view.rs       state → Frame (a grid of characters and styles), wrapping, kinsoku
    color.rs      OKLCH, APCA, color vision simulation, computing the palette
  io/
    file.rs       atomic save, comparison by hash, keeping attributes
    journal.rs    snapshots and recovery
    watch.rs      notify. Debounces changes and sends the disk content as an Event
    filter.rs     running sh -c, and cancelling it
    term.rs       crossterm, diffed Frame output, OSC 11 / 52, cursor shape
```

- **Event**: `Key` `Paste` `Resize` `Opened(path, Result)` `Disk(Option<Vec<u8>>)` `Saved(Result)` `SaveConflict(Vec<u8>)` `FilterDone` `Bg(Rgb)` `Tick(time)` `Signal(i32)`
- **Effect**: `Render(Frame)` `Write { path, bytes, expect_hash }` `Open(path)` `Spawn` `Kill` `Clipboard(String)` `Snapshot` `Hook(String)` `Quit(i32)`
- Only the App on the main thread holds state. Other threads only send Events over mpsc. No locks
- `Write` runs as a compare-and-swap. If the hash on disk differs from `expect_hash`, it writes nothing and returns `SaveConflict` (SAFE-5)
- A Frame has only four styles: `Primary` `Secondary` `Change` `Deleted`. color.rs decides which real colors they map to. (The Visual selection is an underline, not a style; see VI-5)

### Dependencies

The binary depends only on ropey, crossterm, notify, similar, unicode-width, regex-lite and signal-hook (see SAFE-9). The only dev dependency is proptest.
If another one is needed, first write the reason into SPEC.

### Rules

- edition 2024. `#![forbid(unsafe_code)]`
- clippy denies `unwrap_used`, `expect_used` and `panic` (tests may use them, through clippy.toml)
- Never panic on an I/O error. Say what happened, in words, on the status line
- At the top of each module, list the IDs it implements (e.g. `//! SAFE-1, SAFE-3`)
- `[profile.release]` has `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. A panic hook restores the terminal
- src/ stays within 3,000 lines (blank lines and lines that are only a `//` comment don't count). If it is about to go over, propose features to cut
- Code, comments, messages, docs and commit messages are in English

## Tests

The tests form a pyramid too. Roughly: unit and property 70%, integration 25%, E2E 5%.

- **Unit/property** (inside core/): proptest checks diff3 (PROP-4), the save round trip (SAFE-3), undo, and wrapping with kinsoku (VIS-7)
- **Integration** (tests/): feed a sequence of Events to App, then check the Effects and the state. No terminal and no clock are involved, so the results are deterministic. Concurrency, such as editing while a filter runs or an external change right before a save, is reproduced here as well
- **Golden**: turn a Frame into text made of characters and style symbols, and compare it with tests/golden/. `SI_BLESS=1` updates them
- **Fault injection**: each step of the save in io/file.rs can be made to fail (`cfg(test)`). Check that wherever it fails, either the old or the new version is fully intact
- **E2E**: scripts/e2e.sh runs si inside tmux. Case names start with the ID. It uses its own tmux socket and no tmux config, so it never touches the user's tmux server
- **Benchmarks**: `cargo test --release -- --ignored bench` and hyperfine. The thresholds follow PERF in SPEC
- Tests never touch the real `$XDG_STATE_HOME` or `$HOME`; they use temporary directories

## Checks

Before every commit, pass scripts/check.sh (POSIX sh).

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. scripts/trace.sh
5. src/ has at most 3,000 lines
6. the release binary is at most 2 MB

scripts/trace.sh checks these three things, and prints a table with one row per ID (ID, milestone, number of tests, implemented or not).

- every ID up to the current milestone written in `.milestone` has a test or an E2E case whose name starts with that ID
- each of those IDs is listed at the top (`//!`) of some module in src/
- no test claims an ID that isn't in SPEC

## Milestones

The `[M?]` in SPEC decides which milestone each ID belongs to. When a milestone starts, rewrite `.milestone`; when it is done, pass check.sh and commit.

- **M0**: the repository skeleton, check.sh, trace.sh, `.milestone` (M0)
- **M1**: buffer, vi, rendering, atomic save, CLI. The change color may still be stood in for by reverse video
- **M2**: snapshots and recovery. Try it as `$EDITOR` for git commit and for Claude Code's prompt editing
- **M3**: external changes, 3-way merge, REVIEW mode
- **M4**: filters, AI-1, IME support
- **M5**: the visual finish (APCA palette, color vision simulation, kinsoku)
- **M6**: the perception benchmark (optional)
