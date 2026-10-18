# CLAUDE.md — sim

@CONCEPT.md
@SPEC.md

You are implementing sim: the next-generation Vim, written in Rust. It is one program in two crates, and a window in a third: the core (`sim-editor-core`, in `core/`: the buffer, vi's grammar, saves that lose nothing, the merge with what other processes write, the terminal) and the editor (`sim-editor`, in `sim/`: the binary `sim`, which runs the core's editor for each open file and adds the tree, finding, search, marks, syntax, the looks and the mouse). `sim --plain` is the core alone. The window (`sim-gui`, in `gui/`: another binary, not published) draws the editor's screen as surfaces on a GPU; it is part three of SPEC and the program `sim` does not depend on it.
Before starting any work, always read CONCEPT.md and SPEC.md. Part one of SPEC is the core's and holds for every file open in sim; part two is the editor's and only adds; part three is the window's.

## The pyramid

```
          ┌─────────┐
          │ CONCEPT │  why       the principles and the findings  CONCEPT.md
        ┌─┴─────────┴─┐
        │    SPEC     │  what      requirements with IDs         SPEC.md
      ┌─┴─────────────┴─┐
      │      CODE       │  how       core/src ≤ 3,575, sim/src ≤ 11,385, gui/src ≤ 5,100
    ┌─┴─────────────────┴─┐
    │        TEST         │  really?   a test for every ID       core/tests, sim/tests, gui/tests and the E2E scripts
    └─────────────────────┘
```

- **Changes flow down.** Don't build what isn't in the spec. When something is needed, first add a line to SPEC.md with the principle it follows. Then implement it
- **Proof flows up.** Every ID in SPEC has at least one test whose name starts with that ID
- **Never change an upper layer for the convenience of a lower one.** Don't loosen the spec to make a test pass. When the spec changes, explain in one line how it fits the principles
- **When in doubt, decide by the priority of the principles** (P1 > P2 > P3 > P4 > P5; the core's table for part one, the editor's for part two, the window's for part three). Write the decision into SPEC as a **Decided** sub-item
- **Don't fill ambiguities in the spec with guesses; ask**

## Code

### Structure (functional core, imperative shell)

```
Cargo.toml          the workspace: members core, sim and gui; the release profile
core/               the package sim-editor-core, the library crate `sim_core` (published for sim-editor's sake)
  src/
    lib.rs          exposes core/ and io/
    core/           pure. Never touches std::fs, std::process, clocks or the terminal
      cli.rs        the arguments of `sim [+cmd] [file]` → the file and the keys to type first
      app.rs        App::update(&mut self, Event) -> Vec<Effect>
      buffer.rs     ropey and undo
      vi.rs         key sequence → Command (Normal and Visual mode, the command line's editing)
      edit.rs       applies a Command to the Buffer: motions, operators, text objects, Insert mode
      ex.rs         ex commands and their ranges, `:s`, search
      reg.rs        the registers (memory only, ropes), what a yank, a delete and `:registers` do with them, and their hand-over between Apps (VI-15)
      mark.rs       the marks `ma`-`mz` and the place before the last jump: where an edit, an undo and a change from the disk leave them, `'` `` ` `` `''` and `:marks` (VI-17)
      merge.rs      diff3 and the word-level diff
      disk.rs       what App does when the file changes on disk: reload or merge, marks, the conflict of SAFE-7; the result of a filter; `]c` `[c`
      view.rs       state → Frame (a grid of characters and styles), wrapping, kinsoku
      color.rs      OKLCH, APCA, color vision simulation, computing the palette
    io/
      file.rs       reading a file, atomic save, comparison by hash right before the rename, keeping attributes
      journal.rs    snapshots and recovery, the places files were closed at, the state directory (`sim`, and the copy of the old `si` one, STATE-1), and where they live (a private state directory, never /tmp)
      watch.rs      notify. Debounces changes and sends the disk content as an Event; looks at the file once a second too, in case the watch died
      filter.rs     running sh -c, and cancelling it; the one-way switch that forbids every program (PLAIN-1)
      term.rs       crossterm, diffed Frame output, OSC 11 / 52, cursor shape
  tests/            the core's tests; scripts/e2e.sh runs `sim --plain` in tmux
sim/                the package sim-editor: the library `sim` and the binary `sim`
  src/
    main.rs         the terminal's loop: the input thread, the Painter; the start and the signals are shell.rs's, the Effects are run by it
    shell.rs        the start of sim for every front end (`arguments`, `begin`: the rules of plain mode, the state directory and git's hooks, once, from a `World` the system says and a `Front` the front end brings; `signals`), and what is done with the Effects `Sim::update` returns, on threads (the binary) or inline (a window's tests and renders): read, open, save, watch, git, highlight, search, filter, the clipboard; `Out` (GUI-4)
    lib.rs          exposes the modules to main.rs and tests/
    sim.rs          the state: open files (core Apps), events in and effects out, the frame (pure)
    sim/panel.rs    the panel's keys, the reveal of directories, the search, the narrowed tree (pure)
    tree.rs         the tree: entries, stable order, expansion, the visible rows (pure)
    find.rs         fuzzy matching of paths and ranking (pure)
    changes.rs      the changes view's numbers: `+n −n` of each file from `git diff --numstat -z`, the lines of untracked files, the sums of folders (pure)
    keys.rs         the key table (`keys.tsv`: the only source of which-key, `:help` and the README's table of keys, KEY-1), the keys that can follow a prefix, the help's rows, which-key's window (pure)
    crumb.rs        the breadcrumb `dir › file › section`: the section the cursor is in (a heading, or the definition it is inside, by a few string tests), the parts of the path, and how they are cut (pure, UI-18)
    md.rs           the Markdown read view: which chars of a line are not drawn and what is put in their place (markers, aligned tables), the tables of a screen, and the map from the core's columns to the drawn ones (pure, MD-1 to MD-8)
    tutor.rs        `:tutor`: the lessons of ten languages (the text in `tutor/<lang>.txt`, packed into `tutor/tutor.deflate` by `scripts/tutor-pack.sh`), the language the locale asks for, the exercises and their grading (pure, TUTOR-1 to TUTOR-4)
    sim/lesson.rs   the open lesson in the state: its scratch buffer (never written), the marks drawn over `[ ]`, the events seen
    sim/guide.rs    what guides the hands in the state: the leader, the prefix typed, the pause that opens which-key, the letters on the tabs, the hint for Tab (pure)
    tabs.rs         the tab bar and the letters of `<Space>bp` (pure)
    grep.rs         the content search's literal smart-case query, the lines it finds in a text, the rows of the picker's list made of them, rg's output (pure)
    sim/live.rs     the picker in the state: Ctrl-f's search of contents and Ctrl-p's finder of files, their keys, the results (a search's arrive in batches), the preview of the selected file with its colors and the count (pure)
    pick.rs         the picker's window: where it is for a screen's size, its rows (border, prompt, list, preview, count), and how it is laid over the rows of a screen (pure)
    rank.rs         recency/frequency likelihood from the open history (pure)
    marks.rs        git status and on-disk changes → marks for files and directories (pure)
    sim/ptr.rs      the mouse: clicks, drags and the wheel in the panel and the text (pure)
    syn.rs          syntect (bat's syntaxes) → colored spans per line (markdown's looks too, UI-8), with cached parse states
    color.rs        what the core's `core::color` does not have: the syntax colors from the ANSI colors (OSC 4), the selection's gray, the tones of the looks (green, red, tints, the card's surface, shadow, lines, the interface's six accents, their blocks and its neutral surfaces: OKLab mixes) and the fade, and the terminal's replies put together out of the keys (pure)
    paint.rs        the looks beyond the core's four styles, the paints (a background, a color of its own, a fade for a run of cells, its cue without color), the Screen, the Painter that writes it (in color, or in weight, underline and inverse when there is none, COLOR-2), the mouse modes on and off
    layout.rs       the panel and the editor composed into one Screen: marks, search, colors; the three looks (calm, flat, depth): geometry, green and red, tints, boxes, ruler, fog; the char under a point (pure)
    io.rs           directory reads, git (one function builds every command line: the guard of GIT-1 to GIT-4), the file list, the history, look and icons files, the detection of plain mode, of `NO_COLOR` and of terminfo, the threads, the content search (rg or the built-in reader) and the preview's read (the impure parts)
  keys.tsv          the key table as data: keys, modes, a line of description and a longer text for each entry
  help.txt          what `sim --help` prints (`{version}` is the Cargo version)
  tests/            the editor's tests and goldens (`gui_N.rs` are the seams for the window); scripts/e2e.sh runs `sim` in tmux
gui/                the package sim-gui: the library `sim_gui` and the binaries `sim-gui` (the window) and `gui-shots` (offscreen pictures and frame times). Reaches sim through `src/api.rs` only (GUI-8)
  src/
    scene.rs        sim's layers and roles → surfaces with an elevation, and glyphs: a display list, pure, dumped as text for the goldens
    depth.rs        the numbers of the elevations: shadow, radius, rim, scrim
    cells.rs        the cells of a Screen, each as sim's `Resolved` (from sim's `Resolver`)
    frame.rs        one settled frame for the renderer: cells, tones, layers, cursor
    anim.rs         motion: the cursor's glide, the scroll, the fog in time; reduced motion
    input.rs        window events → the terminal's events (keys, input method, paste, pointer, wheel)
    theme.rs        the two themes, handed to sim as `SimEvent::Colors`
    session.rs      sim and its shell held together; headless.rs the same with the jobs inline and keys from a script
    font.rs blocks.rs metrics.rs   text: shaping, fallback, the glyph atlas; box-drawing chars as shapes; where the cells are
    gpu.rs offscreen.rs studio.rs overlay.rs shaders/   the renderer (two pipelines, simple WGSL), offscreen targets, the cursor and the input method's text
    app.rs main.rs  the window (winit): the only code that opens one
    safe_io.rs demo.rs bin/gui-shots.rs   private directories and new files; the neutral demo project; the pictures and the timings
  tests/            golden/ (scene dumps and pixels on llvmpipe), input end to end, private paths, the api seam, the limits
scripts/            check.sh, trace.sh, e2e.sh (runs both suites), e2e-lib.sh (where a case failed), vendor-check.sh, bench.sh, pty_bench.py
vendor/syntect/     syntect 5.3.0 with a guard in its parser (SPEC SYN-3); vendor/README.md, syntect.patch, syntect.sha256. Not a workspace member
core/scripts/e2e.sh the core's cases, run against `sim --plain`
sim/scripts/e2e.sh  the editor's cases
```

- **Event**: `Key` `Paste` `Resize` `Opened(path, Result, seq)` `Recover(base, text, age, file, deadline)` `Disk { bytes, hash, deadline, seq }` `Saved(Result, seq)` `SaveConflict(Option<Vec<u8>>, deadline, seq)` `Notice(String)` `Kept(Result<String, String>)` `FilterDone` `Place(path, line, col)` `Bg(Rgb)` `Tick(time)` `Signal(i32)`. A `deadline` is the instant a diff must give up at, stamped by the shell because the core reads no clock (DISK-5); the `hash` of a `Disk` is made by the thread that read the file, so that no key waits for it (P3). A `seq` numbers a read of the disk from one counter of the shell, taken as the read starts (a save's answer: when the save is done), and App drops a read older than the newest it has taken in (DISK-1)
- **Effect**: `Render(Frame)` `Write { path, bytes, expect_hash }` `Open(path)` `Watch(path)` `Spawn` `Kill` `Clipboard(String)` `Snapshot { path, base, text }` `Unsnapshot` `Keep { path, bytes, ext }` `Remember { path, line, col }` `Hook(String)` `Quit(i32)`
- Only the App on the main thread holds state; the editor holds one App for each open file. Other threads only send Events over mpsc. No locks
- `Write` runs as a compare-and-swap. If the hash on disk differs from `expect_hash`, it writes nothing and returns `SaveConflict` (SAFE-5), and it checks right before the rename, looking at the file's stamp before and after the read so that a write during the read is seen too. `expect_hash: None` is `:w!`: no compare, and a file without write permission is written too (SAFE-1). A `:w!` over a version of the disk that conflicts with mine waits for `Keep` (the disk's bytes copied to the state directory) and its answer `Kept` before it writes, and an `:e!` of a conflicting buffer copies mine the same way before it reads (SAFE-7). When either is done the marks of the conflict and of the disk's changes are cleared
- `Watch(path)` starts watching the file and reads it once, answering with `Event::Disk`: the new watcher's own thread does that read (never the main thread), before the debounced ones. The App asks for it when it gets a file name, after every save's answer, since what the watcher said while the save was in flight cannot be ordered against the save, and after every answer to an `:e`, since the shell silences the old watcher when that read begins, so that nothing it read of the file being left is merged into the next (DISK-1)
- A Frame has only four styles: `Primary` `Secondary` `Change` `Hidden` (a char shown in another form, SAFE-8). color.rs decides which real colors they map to. (The Visual selection is an underline, not a style; see VI-5)
- Plain mode (PLAIN-1) and no color (COLOR-2) are decided once, at the start, by the shell (`io::plain_by_default`, `io::no_color_here`); `filter::forbid_programs` is the one-way switch every place that starts a program asks first. A new place that starts a program asks it too

### Dependencies

The core depends only on ropey, crossterm, notify, rustix (for `geteuid`, its `process` feature only), similar, unicode-width and regex-lite (see SAFE-9; `scripts/check.sh` and `core/tests/safe_9.rs` hold the list). The editor adds syntect (vendored, with a guard: vendor/README.md) and two-face (bat's syntaxes; pure-Rust regex backend) and signal-hook, and uses rustix too (`geteuid`, `getppid`). The only dev dependency is proptest. The window has a closed list of its own (GUI-17: sim-editor, sim-editor-core, crossterm for the event types, winit, wgpu with Vulkan and Metal, cosmic-text, bytemuck, pollster, png); no webview and nothing that can speak to a network, ever, and none of it in the tree of the editor or of the core (`scripts/check.sh` holds all three lists).
If another one is needed, first write the reason into SPEC.

### Rules

- edition 2024. `unsafe_code = "forbid"` (the `[lints.rust]` table of each Cargo.toml, so it covers the libraries and the binary)
- clippy denies `unwrap_used`, `expect_used` and `panic` (tests may use them, through clippy.toml; a helper in a test file allows what it needs at the top of the file)
- Never panic on an I/O error. Say what happened, in words, on the status line
- At the top of each module, list the IDs it implements (e.g. `//! SAFE-1, SAFE-3`)
- A key is added to `sim/keys.tsv` in the same change that adds it to the core or to sim: `sim/tests/key_1.rs` feeds every key to a Sim and fails for a key that does something and is not in the table, and for an entry that does nothing
- `[profile.release]` (in the workspace) has `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. A panic hook restores the terminal
- core/src stays within 3,575 lines, sim/src within 11,385 and gui/src within 5,100 (blank lines and lines that are only a `//` comment don't count). If one is about to go over, propose features to cut; a cap moves only with the reason written in CONCEPT.md. sim never reaches into an App's internals beyond its public API; when the editor needs something the core doesn't offer, add the smallest extension point to the core (and keep the core within its cap)
- Colors: the four styles are the core's palette (`Event::Bg`, given to every App); syntax colors come from the terminal's ANSI colors (OSC 4), adjusted for contrast against that background; in calm the change color stays the only chromatic background and no text color shares its hue (P3); flat and depth show what comes on green and what goes on red (LOOK-2); with no color at all (COLOR-2) weight, underline and inverse stand in, and no meaning is carried by color alone (COLOR-3)
- Code, comments, messages, docs and commit messages are in English. sim is the next-generation Vim (新時代の Vim): in README, CONCEPT, this file, the crates' descriptions and `--help`, it is never described as a likeness of Vim (nothing "-like", nothing "-style")

## Tests

The tests form a pyramid too. Roughly: unit and property 70%, integration 25%, E2E 5%.

- **Unit/property** (inside the modules): proptest checks diff3 (DISK-4), the save round trip (SAFE-3), undo, and wrapping with kinsoku (VIS-7)
- **Integration** (core/tests, sim/tests): feed a sequence of Events to App, or of SimEvents to Sim (the `Rig` in sim/tests/common plays the shell), then check the Effects and the state. No terminal and no clock are involved, so the results are deterministic. Concurrency, such as editing while a filter runs or an external change right before a save, is reproduced here as well
- **Golden**: turn a Frame or a Screen into text made of characters and style symbols, and compare it with tests/golden/. `SIM_BLESS=1` updates them. A golden never holds a private-use code point (ICON-1)
- **Fault injection**: each step of the save in core/src/io/file.rs can be made to fail (`cfg(test)` only: the `#[cfg(test)]` module at the end of the file sets a thread-local step number, and `step()` fails that one). Check that wherever it fails, either the old or the new version is fully intact
- **E2E**: scripts/e2e.sh runs both suites in tmux. core/scripts/e2e.sh runs `sim --plain` (and `sim` for the few cases that need a filter or the hook); sim/scripts/e2e.sh runs `sim`. Case names start with the ID. They use their own tmux socket and no tmux config, so they never touch the user's tmux server, and sim runs in an environment of its own (`env -i`: PATH, HOME, the state directory, tmux's TERM and TMUX, a UTF-8 LANG, no git configuration; nothing of the terminal they run in, a developer's Ghostty or a runner's, so the screens are the same; the hint of ICON-1 counts as given); no screen of a case may hold a private-use code point unless it turned icons on. A case's script must run with BSD tools as well (macOS): no `sed -i`, no GNU-only flags. A case that fails says which of its lines did and the command as it ran (it runs traced; scripts/e2e-lib.sh; the line is `?` and the command is all there is in Debian's and Ubuntu's dash, which has no `LINENO`, so a script keeps to what that dash has, and `${LINENO:-?}` where it says `$LINENO`); a fact that sim says when it is known (the branch, the saved choice of `zi`) is waited for, never read the moment the first thing on the screen is there. `SIM_E2E_KEEP=dir` keeps what each failed case saw and the last commands it ran (CI uploads it)
- **Fuzz and oracle**: core/tests/fuzz.rs runs random texts and keys (and late answers from the world) against the invariants that protect the text; its default case counts keep `cargo test` fast, and `SIM_FUZZ_CASES=50000 cargo test -p sim-editor-core --test fuzz` runs every property that long. core/tests/model.rs drives the App with a model of the shell (one event channel, a save thread, a watcher, an outside process) and checks that nobody's words are lost and a clean buffer is the file; sim/tests/buf_model.rs is the same for many files (`SIM_FUZZ_CASES=30000 cargo test --release -p sim-editor --test buf_model`); core/tests/shift.rs checks marks and hunks under every edit; core/tests/events.rs throws every kind of event at the App; core/tests/shell.rs does the model's job with the real watcher and threads (ignored, slow); core/tests/races.rs holds one test per race found by review. core/tests/oracle.rs compares commands with `nvim --headless` (`cargo test -p sim-editor-core --test oracle -- --ignored --nocapture`, see its header)
- **Benchmarks**: `cargo test --release --workspace -- --ignored bench --nocapture` and hyperfine (scripts/bench.sh). The thresholds follow PERF and SPEED in SPEC
- Tests never touch the real `$XDG_STATE_HOME`, `$HOME` or git config; they use temporary directories (check.sh runs them with no global git config; repositories in tests are made with none either). A test that runs the real binary gives it an environment of its own (`common::clean_env`, `pty_bench.py`'s, the e2e suites' `env -i`): a CI runner has no `TERM`, a developer's terminal sets `TERM_PROGRAM`, `COLORTERM`, `GHOSTTY_*`, `KITTY_*`, `WEZTERM_*` and `TMUX`, and a run must not depend on which

## Checks

Before every commit, pass scripts/check.sh (POSIX sh).

1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace --no-fail-fast`
4. scripts/trace.sh
5. core/src has at most 3,575 lines
6. sim/src has at most 11,385 lines
7. gui/src has at most 5,100 lines
8. the core depends on its allowlist only
9. the editor depends on its own list only, and no crate of the window is in the tree of the editor or of the core
10. the window depends on its allowlist only, and no network crate or webview is in its tree
11. the name is *Secure vIM* and the crates are sim-editor and sim-editor-core: a grep keeps the earlier name stories and the earlier crate name from coming back
12. the release binary `sim` (built alone, `-p sim-editor`) is at most 5 MiB and 32 KiB (5,275,648 bytes)
13. vendor/syntect is upstream's syntect 5.3.0 and vendor/syntect.patch, nothing else (scripts/vendor-check.sh: the patch taken off a copy, every file against the sha256 list)
14. the traced path of the e2e harnesses works in the `sh` that runs check.sh, with no tmux and no sim (scripts/e2e-lib-check.sh: a passing case and a failing one, and the failing one is told by its command: Ubuntu's dash has no LINENO)

scripts/trace.sh checks these three things, and prints a table with one row per ID (ID, milestone, number of tests, implemented or not).

- every ID up to the current milestone written in `.milestone` has a test or an E2E case whose name starts with that ID
- each of those IDs is listed at the top (`//!`) of some module in core/src, sim/src or gui/src
- no test claims an ID that isn't in SPEC

## Milestones

The `[M?]`, `[S?]` and `[G?]` in SPEC decide which milestone each ID belongs to; `.milestone` holds the current one of each (`M5 S7 G1`). When a milestone starts, rewrite `.milestone`; when it is done, pass check.sh and commit.

The core's:

- **M0**: the repository skeleton, check.sh, trace.sh, `.milestone` (M0)
- **M1**: buffer, vi, rendering, atomic save, CLI. The change color may still be stood in for by reverse video
- **M2**: snapshots and recovery. Try it as `$EDITOR` for git commit and for Claude Code's prompt editing
- **M3**: external changes, 3-way merge, marks and conflicts
- **M4**: filters, IME support
- **M5**: the visual finish (APCA palette, color vision simulation, kinsoku); one program (plain mode, one state directory)
- **M6**: the perception benchmark (optional)

The editor's:

- **S0**: the workspace member, skeleton, check.sh, trace.sh
- **S1**: several files and the tree panel
- **S2**: finding, marks for changed and likely files, focus
- **S3**: typographic syntax highlighting
- **S4**: the mouse (PTR-1, PTR-2), the benches again, polish; on the core's M5 (numbered reads, the terminal's colors by the core's palette)
- **S5**: the looks: calm, flat and depth (LOOK-1 to LOOK-9); no color and the icons' defaults (COLOR-2, COLOR-3, ICON-1)
- **S6**: the interface that matches and beats the author's Neovim: tree icons and git status, tabs, signs, guides, TODO badges, markdown, the status line (UI-1 to UI-12)

The window's:

- **G1**: the seams in the editor (GUI-1 to GUI-7), each a small public API with a test that the terminal's own code is written on, and the window (GUI-8 to GUI-17): a crate of its own that draws sim's depth look on a GPU, with the input of a terminal; offscreen renders, goldens and frame times. No window is opened by a test, a tool or an agent on a desktop that is somebody's: only `cargo run -p sim-gui -- <file>` opens one, and the GPU of the tests is llvmpipe
