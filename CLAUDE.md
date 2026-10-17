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
      │      CODE       │  how       at most 3,603 lines       src/
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
    disk.rs       what App does when the file changes on disk: reload or merge, marks, hunks
    review.rs     `]c` `[c`, REVIEW mode (`y n Y N`), and the refusals of SAFE-7
    view.rs       state → Frame (a grid of characters and styles), wrapping, kinsoku
    color.rs      OKLCH, APCA, color vision simulation, computing the palette
  io/
    file.rs       reading a file, atomic save, comparison by hash right before the rename, keeping attributes
    journal.rs    snapshots and recovery, the places files were closed at, and where they live (a private state directory, never /tmp)
    watch.rs      notify. Debounces changes and sends the disk content as an Event; looks at the file once a second too, in case the watch died
    filter.rs     running sh -c, and cancelling it
    term.rs       crossterm, diffed Frame output, OSC 11 / 52, cursor shape
```

- **Event**: `Key` `Paste` `Resize` `Opened(path, Result, seq)` `Recover(base, text, age, file, deadline)` `Disk { bytes, hash, deadline, seq }` `Saved(Result, seq)` `SaveConflict(Option<Vec<u8>>, deadline, seq)` `Notice(String)` `FilterDone` `Place(path, line, col)` `Bg(Rgb)` `Tick(time)` `Signal(i32)`. A `deadline` is the instant a diff must give up at, stamped by the shell because the core reads no clock (PROP-7); the `hash` of a `Disk` is made by the thread that read the file, so that no key waits for it (P3). A `seq` numbers a read of the disk from one counter of the shell, taken as the read starts (a save's answer: when the save is done), and App drops a read older than the newest it has taken in (PROP-1)
- **Effect**: `Render(Frame)` `Write { path, bytes, expect_hash }` `Open(path)` `Watch(path)` `Spawn` `Kill` `Clipboard(String)` `Snapshot { path, base, text }` `Unsnapshot` `Remember { path, line, col }` `Hook(String)` `Quit(i32)`
- Only the App on the main thread holds state. Other threads only send Events over mpsc. No locks
- `Write` runs as a compare-and-swap. If the hash on disk differs from `expect_hash`, it writes nothing and returns `SaveConflict` (SAFE-5), and it checks right before the rename, looking at the file's stamp before and after the read so that a write during the read is seen too. `expect_hash: None` is `:w!`: no compare, and a file without write permission is written too (SAFE-1)
- `Watch(path)` starts watching the file and reads it once, answering with `Event::Disk`: the new watcher's own thread does that read (never the main thread), before the debounced ones. The App asks for it when it gets a file name, after every save's answer, since what the watcher said while the save was in flight cannot be ordered against the save, and after every answer to an `:e`, since the shell silences the old watcher when that read begins, so that nothing it read of the file being left is merged into the next (PROP-1)
- A Frame has only four styles: `Primary` `Secondary` `Change` `Deleted`. color.rs decides which real colors they map to. (The Visual selection is an underline, not a style; see VI-5)

### Dependencies

The binary depends only on ropey, crossterm, notify, similar, unicode-width, regex-lite and signal-hook (see SAFE-9). The only dev dependency is proptest.
If another one is needed, first write the reason into SPEC.

### Rules

- edition 2024. `unsafe_code = "forbid"` (the `[lints.rust]` table of Cargo.toml, so it covers the library and the binary)
- clippy denies `unwrap_used`, `expect_used` and `panic` (tests may use them, through clippy.toml)
- Never panic on an I/O error. Say what happened, in words, on the status line
- At the top of each module, list the IDs it implements (e.g. `//! SAFE-1, SAFE-3`)
- `[profile.release]` has `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. A panic hook restores the terminal
- src/ stays within 3,603 lines (blank lines and lines that are only a `//` comment don't count). If it is about to go over, propose features to cut
- Code, comments, messages, docs and commit messages are in English

## Tests

The tests form a pyramid too. Roughly: unit and property 70%, integration 25%, E2E 5%.

- **Unit/property** (inside core/): proptest checks diff3 (PROP-4), the save round trip (SAFE-3), undo, and wrapping with kinsoku (VIS-7)
- **Integration** (tests/): feed a sequence of Events to App, then check the Effects and the state. No terminal and no clock are involved, so the results are deterministic. Concurrency, such as editing while a filter runs or an external change right before a save, is reproduced here as well
- **Golden**: turn a Frame into text made of characters and style symbols, and compare it with tests/golden/. `SI_BLESS=1` updates them
- **Fault injection**: each step of the save in io/file.rs can be made to fail (`cfg(test)` only: the `#[cfg(test)]` module at the end of io/file.rs sets a thread-local step number, and `step()` fails that one). Check that wherever it fails, either the old or the new version is fully intact
- **E2E**: scripts/e2e.sh runs si inside tmux. Case names start with the ID. It uses its own tmux socket and no tmux config, so it never touches the user's tmux server
- **Fuzz and oracle**: tests/fuzz.rs runs random texts and keys (and late answers from the world) against the invariants that protect the text; its default case counts keep `cargo test` fast, and `SI_FUZZ_CASES=50000 cargo test --test fuzz` runs every property that long. tests/model.rs drives the App with a model of the shell (one event channel, a save thread, a watcher, an outside process) and checks that nobody's words are lost and a clean buffer is the file; tests/shift.rs checks marks and hunks under every edit; tests/events.rs throws every kind of event at the App; tests/shell.rs does the model's job with the real watcher and threads (ignored, slow); tests/races.rs holds one test per race found by review. tests/oracle.rs compares commands with `nvim --headless` (`cargo test --test oracle -- --ignored --nocapture`, see its header)
- **Benchmarks**: `cargo test --release -- --ignored bench` and hyperfine. The thresholds follow PERF in SPEC
- Tests never touch the real `$XDG_STATE_HOME` or `$HOME`; they use temporary directories

## Checks

Before every commit, pass scripts/check.sh (POSIX sh).

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. scripts/trace.sh
5. src/ has at most 3,603 lines
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
