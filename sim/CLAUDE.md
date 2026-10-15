# CLAUDE.md — sim

@CONCEPT.md
@SPEC.md

You are implementing sim: si for your own machine. sim is a separate binary in the `sim/` member of the si workspace. It reuses si's library (`si::core`, `si::io`) and adds a tree panel, several open files, marks for changed and likely files, typographic syntax highlighting and the mouse.
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
    tree.rs        the tree: entries, stable order, expansion, the visible rows (pure)
    find.rs        fuzzy matching of paths and ranking (pure)
    rank.rs        recency/frequency likelihood from the open history (pure)
    marks.rs       git status and on-disk changes → marks for files and directories (pure)
    syn.rs         syntect → typographic spans per line, with cached parse states
    layout.rs      the panel and the editor composed into one Frame (pure)
    io.rs          directory reads, git, the history file (the impure parts)
```

- Functional core, imperative shell, as in si: the pure modules never touch the file system, processes, clocks or the terminal
- Each open file is one `si::core::app::App`. sim never reaches into an App's internals beyond its public API; when sim needs something si doesn't offer, add the smallest extension point to si (and keep si within its own 3,000 lines and its SPEC)
- Colors: si's palette only. Syntax highlighting uses bold, italic and the secondary gray; the change color stays the only hue (P3)
- Dependencies: si, syntect (pure-Rust regex backend). Anything else needs a reason in SPEC first
- sim/src stays within 3,000 lines (counted like si's). Rules from si's CLAUDE.md (edition 2024, `#![forbid(unsafe_code)]`, clippy denies unwrap/expect/panic, no panics on I/O errors, `//!` ID headers, English everywhere) apply

## Tests and checks

- Unit/property tests for the pure modules, integration tests that feed events to sim's state, goldens for composed frames, E2E in tmux on its own socket, benches for SPEED
- `sim/scripts/check.sh`: fmt, clippy, tests, `sim/scripts/trace.sh`, sim/src ≤ 3,000 lines. si's own check.sh must keep passing too
- Never touch the real `$HOME` / `$XDG_STATE_HOME` / git config in tests (check.sh runs them with no global git config; repositories in tests are made with none either)
- Goldens are in `sim/tests/golden/`; `SIM_BLESS=1` rewrites them. Benches are the ignored `*_bench_*` tests: `cargo test --release -p sim -- --ignored bench --nocapture`

## Milestones

`.milestone` in sim/ holds sim's current milestone.

- **S0**: the workspace member, skeleton, check.sh, trace.sh
- **S1**: several files and the tree panel
- **S2**: finding, marks for changed and likely files, focus
- **S3**: typographic syntax highlighting
- **S4**: the mouse, benches, polish
