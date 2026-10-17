# Changelog

All notable changes to sim are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The changes view: `<Space>ge` opens the panel with only the files that differ from `HEAD`, staged and unstaged together and untracked ones too, in their folders. Each file shows its status letter and `+n −n` from `git diff --numstat HEAD` (an untracked file its line count) and each folder the sums. Deleted files are listed, and Enter opens `HEAD`'s version of one read-only, as a buffer named `HEAD:path` whose tab says so; a rename is one row, `old → new`, with a cut that keeps the new name whole and marks every cut with `…`. `<Space>ge` again, or `q`, goes back to the tree. In `sim --plain` the key is refused, naming plain mode (DIFF-1 to DIFF-6).
- `sim-gui`, a window for sim, in `gui/` (a third workspace member, `publish = false`: it is not on crates.io, Homebrew or the AUR yet, and the `sim` binary and the `sim-editor` and `sim-editor-core` crates do not depend on it). It draws the depth look on the GPU (boxes raised from a floor with soft shadows, a chip for the shown tab, a window that floats over a dimmed page, a char shown in another form as a rounded pill) from the editor's own screen: the keys, the modes, the colors and the jobs are sim's, and the window keeps no rule of its own (GUI-1 to GUI-17). It is built with `cargo run -p sim-gui -- <file>`; the text is set in the platform's monospace font (`SIM_GUI_FONT` names another, `sim-gui --help` lists everything it reads from the environment), the input method works where a key is text, and `SIM_GUI_REDUCED_MOTION` turns the motion off. Its dependency list is closed and has no webview and nothing that speaks to a network. Not seen on a real display yet.

### Changed

- sim's git is built in one function, and what a command line can stop is neutralized instead of making sim skip the repository: a file system monitor, hooks (`core.hooksPath` points to an empty private directory), a filter (emptied by name, except git-lfs's own filter as `git lfs install` writes it, with only git-lfs's harmless settings beside it), an external diff and a text conversion run nothing, and git answers. A repository whose config still could run a command that nothing stops, such as `core.sshCommand` or an `include`, is skipped as before, with the key named (GIT-1 to GIT-3).
- A submodule is one entry with its status and no child git runs in it: `git status` sees its commit and not its work tree, and every diff of sim's takes no submodule (GIT-4).

## [0.1.0] - 2026-10-17

First public release: sim, the next-generation Vim. It shows even what is hidden, it is secure, and it is written in Rust. One program, `sim`, in two crates: the core (`sim-editor-core`, library `sim_core`) and the editor (`sim-editor`, the binary).

### Added

#### Safety: nobody's text is lost without an explicit action (SAFE)

- Atomic saves: a temporary file in the same directory (mode 0600, `O_EXCL`), written, fsynced, renamed over the file, and the directory fsynced; whichever step fails, the old or the new version stays intact (SAFE-1). A file owned by someone else, with hard links, or in a directory that is not writable is saved in place, after a recovery copy has been written and synced (SAFE-2).
- Compare-and-swap saves: right before saving, the file on disk is compared with what was read; if it changed, nothing is written over it (SAFE-5).
- Opening and saving without editing gives byte-identical output: line endings, final newline, BOM and permissions (SAFE-3). A file that is not valid UTF-8, or has a NUL in its first 8 KB, opens read-only and says why (SAFE-4).
- Unsaved changes are snapshotted at most once per second into a private state directory (`$XDG_STATE_HOME/sim`, else `~/.local/state/sim`, mode 0700). After a crash, `kill -9` or a closed terminal, reopening the file offers them back as one undo step (SAFE-6, STATE-1). SIGTERM and SIGHUP snapshot and exit; every way out, a panic included, restores the terminal (CLI-4).
- Hidden characters are shown, never sent raw to the terminal: control characters as `^[`, bidirectional controls, U+200B and a stray U+FEFF as `<U+202E>`, drawn in the alert's block in every state a cell can be in (SAFE-8, UI-15).
- No modelines: settings and commands are never read from file contents. No network access: no source file of sim uses `std::net`, and the core depends on an allowlist of crates that `scripts/check.sh` enforces (SAFE-9).
- A file's name is a path for git, never a pattern (`GIT_LITERAL_PATHSPECS`), and the sign column's diff never enters a submodule (`--ignore-submodules=all`, `diff.submodule=short`): a file named `*` or `:(top)` in a repository whose submodule has config of its own could make git run that submodule's external diff (MARK-1, UI-5).

#### Changes from outside are merged and marked (DISK)

- The directory of the open file is watched, and a change on disk is taken in within 100 ms, saves by rename, deletion and re-creation included. Events from sim's own saves are recognized by hash and ignored (DISK-1).
- A clean buffer is reloaded and what changed is marked. A dirty buffer gets a three-way merge of base, yours and the disk's (our own, built from two line diffs, with no conflict-marker strings): what does not conflict is taken in and marked, word by word, until you have looked at it (DISK-2, DISK-3, DISK-4).
- A conflict is never merged away: your text stays, marked, and the file on disk is not written over. `:w!` keeps yours and `:e!` takes the disk's, and each copies the version it gives up into the state directory first (SAFE-7).
- `]c` and `[c` go to the next and the previous mark; a mark stays until the cursor enters its line; `:seen` clears them all (DISK-6).
- Filters: `:{range}!cmd` and `!` in Visual mode run asynchronously with `sh -c`, can be cancelled with Ctrl-c, and replace their lines as one undo step; a failing filter changes nothing and shows its stderr (FILT-1, FILT-2).

#### Keys: the vi grammar, minus vi's accidents (VI, CLI)

- Motions, operators, text objects, counts, `.`, Visual mode (`v V o`), `:s` and ranges; `/ ? n N * #` with smartcase and incremental search, in the syntax of `regex-lite` (VI-1 to VI-7, VI-14).
- `Y` yanks to the end of the line, `U` redoes, Ctrl-s saves in every mode, `ZZ` saves and quits, `ZQ` quits discarding, `q` quits, `:cq` exits with 1 to abort a git commit; the arrows, Home, End, PageUp, PageDown and Delete work in every mode (VI-13, VI-12, CLI-3).
- One register without names. Every yank also reaches the system clipboard through OSC 52, also through tmux (VI-8).
- The kitty keyboard protocol is enabled when offered, Esc has no delay, and a bracketed paste is one undo unit (VI-9).
- With the IME on, full-width ASCII is read as half-width in Normal mode; `SIM_ON_NORMAL` runs a command (for example `fcitx5-remote -c`) each time Normal mode is entered (VI-10, VI-11).
- Reopening a file puts the cursor where you left it (the last 1,000 files; not for `+cmd`, `file:line` or git's message files) (CLI-6).
- `sim`, `sim dir`, `sim file`, `sim file:42[:col]`, `sim +N file`, `sim +/re file`, `sim +cmd file`; `--` ends the options (CLI-1, CLI-2).

#### The project at your fingertips (the editor)

- A tree of the project on the left (`sim`, `sim dir`): directories first, then files by name, numbers compared as numbers, never reordered; read in the background; Ctrl-n opens, focuses and closes it (TREE-1 to TREE-5).
- Several files open at once, each a complete core editor with its own undo, snapshots, watcher and saves; Tab and Shift-Tab switch; `:wa`, `:qa` and `:qa!` act on all of them (BUF-1 to BUF-3).
- Finding a file: `/` in the tree lights up fuzzy matches and the folders leading to them in place, without moving anything; Ctrl-p anywhere opens a picker with the matches and a preview (FIND-1 to FIND-4).
- Searching contents: Ctrl-f anywhere searches the files as you type (literal, smartcase, `.gitignore` respected), using ripgrep when it is on `PATH`; results are grouped by file with a preview; Enter opens there, Esc puts everything back. A search never writes, never follows a symlink out of the root, and never runs code a repository controls, such as ripgrep's config or `--pre` (GREP-1 to GREP-8, PICK-1 to PICK-5).
- Marks: files git reports as changed, and files that changed on disk while sim runs, carry the change color, and folders carry the letter of the strongest status below them. The five files you are most likely to open next, ranked from your own history, carry `•`. `.` in the tree narrows it to what changed, is open or was opened recently (MARK-1 to MARK-3).
- Syntax highlighting for the languages of bat's syntax set, in the terminal's own palette; `zh` turns it on and off; it never delays a key (SYN-1 to SYN-5).
- The mouse: click to open or place the cursor, wheel to scroll, drag to select. It never edits by itself (PTR-1, PTR-2).

#### Readable by measurement, meaning without color (LOOK, UI, COLOR, ICON, VIS)

- Three looks, cycled with `zl` and remembered: calm (one change color), flat (green for what comes, red for what goes, with a tint behind every row of a change) and depth (the default: boxes, an overview ruler, and text that fades with its distance from the cursor) (LOOK-1 to LOOK-4, LOOK-6 to LOOK-9).
- Colors computed from the terminal's background with APCA: text at Lc 90, secondary text at Lc 60, and text on every surface at Lc 75 or more on the eight reference backgrounds of SPEC. The calm look's change color stays distinguishable under protan, deutan and tritan color vision (VIS-4, VIS-5, UI-12, LOOK-9).
- The interface of the editor: a header and icons in the tree, git status per entry, a tab bar, a sign column for changes against `HEAD`, indent guides and whitespace marks, `TODO`/`NOTE`/`FIXME` badges, Markdown drawn with every character still on screen, a status line with the security state, a scrollbar, a flash on yank, and lit matches of the last search (`:noh` clears them) (UI-1 to UI-14).
- One hue, one meaning across the interface: grays and weight are structure, green, amber and red are changes, blue is where you act, cyan is a heading, magenta is only what is hidden or dangerous (COLOR-1).
- With `NO_COLOR` set, or on a terminal that cannot do 8 colors, sim writes no color and asks the terminal for none: weight, underline and inverse video say what the colors said, and a changed, an added and a deleted line are three shapes in the sign column (COLOR-2, COLOR-3).
- Icons need a Nerd Font, which a terminal does not announce: they start on in Ghostty, kitty and WezTerm and off elsewhere, with a one-time hint to turn them on with `zi` (ICON-1).
- Prose (`.md`, `.txt`, `COMMIT_EDITMSG` and files without an extension) wraps at 72 columns in the core and at the pane's width in the editor, with Japanese kinsoku; a tab is 4 columns; the cursor line is kept in the middle of the screen after a jump (VIS-6 to VIS-8, VIS-12, UI-9).

#### Plain mode and `$EDITOR`

- `sim --plain` is the core alone: the calm look, one file, no panel, and no program of its own run (no git, ripgrep, filter or hook). It is the default for root and when sim is the editor of `sudoedit` (on Linux; `SUDO_EDITOR='sim --plain'` says it anywhere) (PLAIN-1).
- Meant to be `$EDITOR`: `git commit` works end to end with `ZZ` to commit and `:cq` to abort (CLI-3). `sim --plain` without a file shows a short intro until the first key (CLI-5).
- sim reads the old name `SI_ON_NORMAL` and copies the state directory of `si`, the minimal editor it grew from, once, without touching the old one (VI-11, STATE-1).

#### Performance requirements

- SPEC sets them and the repository checks them with benchmarks (`cargo test --release --workspace -- --ignored bench`): `sim --plain +q` on a 10,000-line file within 5 ms; a key within 1 ms at p99 on a 100,000-line file, also with the panel open and highlighting on; a first frame within 20 ms in a repository of 100,000 files; a release binary of at most 5 MB (PERF-1 to PERF-5, SPEED-1, SPEED-2).

#### Keys you can find (KEY, CLI)

- One key table, `sim/keys.tsv`, is the only source of which-key and `:help` (and of the README's table of keys). A test feeds every key to a sim and fails for a key that does something and is not in the table, and for an entry that does nothing (KEY-1).
- `Space` is a leader key in Normal mode and in the tree: `<Space> b p` and `<Space> b c` run from it. Space followed by a key no sequence has does nothing, Esc ends the sequence, and where Space is text (Insert mode, the command line, the query of the tree or of the picker) it is the character (KEY-2).
- A pause of 200 ms on a prefix (`Space` and what follows it, `g`, `z`, `Z`, `[`, `]`, and an operator with what follows it, in Normal and Visual mode and in the tree) opens which-key: a small window near the bottom of the screen that lists the next keys, one line each, from the key table. Typing on is exactly what it is without the window, Esc closes it, and the terminal's cursor stays at the cursor of the text (KEY-3).
- `:help` and `F1` open the picker as `Help · keys and commands`: one row for each entry of the key table, fuzzy-filtered as the finder filters paths, with a preview of the longer text. Enter and Esc close it and it runs nothing; `:help {words}` opens it with the words typed, and `tab` finds `Tab`, `Shift-Tab` and `<Space>bp`. Keys are written as people write them, with no escape of the table (`<` for `<lt>`, alternatives as `h / <Left>`) (KEY-4).
- `<Space> b p` puts a letter on each tab and the letter jumps to its file; `<Space> b c` does the same to close one. A modified file is not closed: it follows `:q` (KEY-5).
- The first time a second file is open, the message line says `Tab / S-Tab: next / previous file`, once for the session, until Tab or Shift-Tab has been used three times (KEY-6).
- While the matches of a search are lit, the status line says which one the cursor is at and how many there are, `[3/12]` (UI-17).
- The boxes' titles say what the box is, with no number: the panel's says `files`, the editor's says nothing, since the file's name is in its tab (UI-16).
- `sim --version` prints `sim 0.1.0` and exits 0. `sim --help` prints the usage, with the environment sim reads (`NO_COLOR`, `SIM_ON_NORMAL`, `XDG_STATE_HOME`), and exits 0. Any other argument that starts with `-` (and is not `--plain` or `--`) prints the same usage to stderr and exits 2. None of them touches the terminal (CLI-7, CLI-1).

[Unreleased]: https://github.com/efg-technologies/sim/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/efg-technologies/sim/releases/tag/v0.1.0
