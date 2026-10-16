# CLAUDE.md — sim

@CONCEPT.md
@SPEC.md

You are implementing sim: si for your own machine. sim is a separate binary in the `sim/` member of the si workspace. It reuses si's library (`si::core`, `si::io`) and adds a tree panel, several open files, marks for changed and likely files, syntax highlighting in color and the mouse.
Its event loop is si's main.rs for many files: whatever si's shell does for its one file (numbered disk reads, the watcher, filters, the hook, places, the terminal's colors, the bounded wait at the quit) it does for each, and where si's library has a function sim calls it.
Before starting any work, read sim's CONCEPT.md and SPEC.md, and si's CONCEPT.md, SPEC.md and CLAUDE.md in the repository root: si's rules hold here unless this file says otherwise.

## The pyramid

The same as si's: changes flow down (CONCEPT → SPEC → CODE → TEST), proof flows up (every ID has a test whose name starts with it), never loosen an upper layer for a lower one, decide by principle priority (P1 > P2 > P3 > P4 > P5) and write the decision into SPEC as a **Decided** sub-item, and ask instead of guessing when no principle decides.

## Code

```
sim/
  Cargo.toml       depends on si (path ".."), syntect
  src/
    main.rs        the event loop: several si Apps, the panel, the threads, running Effects
    lib.rs         exposes the modules to main.rs and tests/
    sim.rs         the state: open files (si Apps), events in and effects out, the frame (pure)
    sim/panel.rs   the panel's keys, the reveal of directories, the search, the narrowed tree (pure)
    tree.rs        the tree: entries, stable order, expansion, the visible rows (pure)
    find.rs        fuzzy matching of paths and ranking (pure)
    grep.rs        the content search's literal smart-case query, the lines it finds in a text, the rows of the panel made of them, rg's output (pure)
    sim/live.rs    the content search in the state: its keys, the results that arrive in batches, the preview in the editor pane, its status line (pure)
    rank.rs        recency/frequency likelihood from the open history (pure)
    marks.rs       git status and on-disk changes → marks for files and directories (pure)
    sim/ptr.rs     the mouse: clicks, drags and the wheel in the panel and the text (pure)
    syn.rs         syntect (bat's syntaxes) → colored spans per line (markdown's looks too, UI-8), with cached parse states
    color.rs       what si's `core::color` does not have: the syntax colors from the ANSI colors (OSC 4), the selection's gray, the tones of the looks (green, red, tints, card, shadow, lines, the interface's six accents, their blocks and its neutral surfaces: OKLab mixes) and the fade, and the terminal's replies put together out of the keys (pure)
    paint.rs       the looks beyond si's four styles, the paints (a background, a color of its own, a fade for a run of cells), the Screen, the Painter that writes it, the mouse modes on and off
    layout.rs      the panel and the editor composed into one Screen: marks, search, colors; the three looks (calm, flat, depth): geometry, green and red, tints, boxes, ruler, card, fog; the char under a point (pure)
    io.rs          directory reads, git, the file list, the history and look files, the threads, the content search (rg or the built-in reader) and the preview's read (the impure parts)
```

- Functional core, imperative shell, as in si: the pure modules never touch the file system, processes, clocks or the terminal
- Each open file is one `si::core::app::App`. sim never reaches into an App's internals beyond its public API; when sim needs something si doesn't offer, add the smallest extension point to si (and keep si within its own 3,600 lines and its SPEC; when si is at its cap, only a visibility may change there)
- Colors: the four styles are si's palette (`Event::Bg`, given to every App; `term::sgr` paints them); syntax colors come from the terminal's ANSI colors (OSC 4), adjusted for contrast against that background; in calm the change color stays the only chromatic background and no text color shares its hue (P3); flat and depth show what comes on green and what goes on red (LOOK-2)
- Dependencies: si, syntect and two-face (bat's syntaxes; pure-Rust regex backend), ropey. Anything else needs a reason in SPEC first
- sim/src stays within 6,400 lines (counted like si's; the reason it is not 3,000 any more is in CONCEPT.md). Rules from si's CLAUDE.md (edition 2024, `#![forbid(unsafe_code)]`, clippy denies unwrap/expect/panic, no panics on I/O errors, `//!` ID headers, English everywhere) apply

## Tests and checks

- Unit/property tests for the pure modules, integration tests that feed events to sim's state, goldens for composed frames, E2E in tmux on its own socket, benches for SPEED
- `tests/buf_model.rs` is si's `tests/model.rs` for many files: a model of the shell (one event channel, a save thread, a watcher and a filter for each file, late listings and highlights, the real journal thread, an outside process that edits, deletes and moves files) drives `Sim::update` with random events and checks that no word is lost or crosses to another file, that no watcher outlives its file and that no row of the screen holds a char SAFE-8 forbids. `SI_FUZZ_CASES=30000 cargo test --release -p sim --test buf_model` runs it longer (the default is 2000 cases)
- `sim/scripts/check.sh`: fmt, clippy, tests, `sim/scripts/trace.sh`, sim/src ≤ 6,400 lines. si's own check.sh must keep passing too
- Never touch the real `$HOME` / `$XDG_STATE_HOME` / git config in tests (check.sh runs them with no global git config; repositories in tests are made with none either)
- Goldens are in `sim/tests/golden/`; `SIM_BLESS=1` rewrites them. Benches are the ignored `*_bench_*` tests: `cargo test --release -p sim -- --ignored bench --nocapture`

## Milestones

`.milestone` in sim/ holds sim's current milestone.

- **S0**: the workspace member, skeleton, check.sh, trace.sh
- **S1**: several files and the tree panel
- **S2**: finding, marks for changed and likely files, focus
- **S3**: typographic syntax highlighting
- **S4**: the mouse (PTR-1, PTR-2), the benches again, polish; on si's M5 (numbered reads, the terminal's colors by si's palette)
- **S5**: the looks: calm, flat and depth (LOOK-1 to LOOK-9)
- **S6**: the interface that matches and beats the author's Neovim: tree icons and git status, tabs, signs, guides, TODO badges, markdown, the status line (UI-1 to UI-12)
