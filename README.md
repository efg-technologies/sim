# sim

> 隠れたものまで見える、セキュアな Rust 製 Vim

sim is the next-generation Vim (新時代の Vim): it shows even what is hidden, it is secure, and it is written in Rust. *SI iMproved*, as Vim is *Vi IMproved*.

Files that other processes rewrite: you read them as fast as possible, fix them, and save them without losing anything. A project's files a few keys away, several of them open at once, and syntax colors that never take the change color's hue.

sim is one program in two crates: the **core** (`sim-core`, in `core/`: the buffer, vi's grammar, saves, the merge with what other processes write, the terminal) and the **editor** (`sim-editor`, in `sim/`: the binary `sim`, with the tree, finding, search, marks, syntax, the looks and the mouse). `sim --plain` is the core alone: the calm look, one file, no panel, and no program of its own run (no git, ripgrep, language server, filter or hook). It is what sim is as root and as the editor of `sudoedit`.

Everything sim promises, it promises for every file open in it: **without an explicit action, nobody's text is lost** — yours or another process's.

## What makes it different

- **Changes from outside are marked.** When a file changes on disk while you edit it, sim merges what doesn't conflict with your edits and marks it, word by word, until you've looked at it; `]c` goes to the next mark. Where it does conflict, your text stays, marked, and nothing is written over the file: `:w!` keeps yours and `:e!` takes the disk's, and each copies the version it gives up into the state directory first. When the choice is made, the marks go.
- **Saves that can't lose.** Saves are atomic (temp file, fsync, rename, directory fsync) and compare-and-swap: if the file changed since you read it, nothing is overwritten. Unsaved changes are snapshotted every second; after a crash, `kill -9` or a closed terminal, reopening the file offers them back as one undo step.
- **Hidden things are shown.** A character that must not reach the terminal, or that looks like another, is drawn as itself in the alert's block (`^X`, `<U+202E>`), and nothing around it can hide it.
- **Readable by measurement.** Text is 90 Lc and secondary text 60 Lc (APCA), computed from your terminal's background; the one change color stays distinguishable under protan, deutan and tritan color vision; prose wraps at 72 columns with Japanese kinsoku.
- **Meaning without color.** With `NO_COLOR` set, or on a terminal that cannot do 8 colors, sim writes no color at all and asks the terminal for none: weight, underline and inverse video say what the colors said, and a changed, an added and a deleted line are three shapes in the sign column.
- **Keys your hands already know.** The vi grammar, minus vi's accidents: `Y` yanks to the end of the line, `U` redoes, search is smartcase, every yank reaches the system clipboard (also through tmux), Ctrl-s saves, `q` quits, arrows and Home/End work everywhere.

The why behind each choice is in [CONCEPT.md](CONCEPT.md), with the research it rests on. What sim must do is in [SPEC.md](SPEC.md): every requirement has an ID, and every ID has tests.

## Install

```sh
cargo install --locked sim-editor    # from crates.io, once it is released
```

or from a checkout:

```sh
git clone https://github.com/efg-technologies/sim.git && cd sim
cargo install --locked --path sim    # the binary `sim`
```

Then, if you like it as your editor:

```sh
export EDITOR=sim                     # Claude Code's Ctrl-g, crontab -e, …
git config --global core.editor sim   # git commit
export SUDO_EDITOR='sim --plain'      # sudoedit (sim is plain there by itself on Linux)
```

Needs Rust 1.88 or later (the code uses `let` chains). Runs on Linux and macOS, in any terminal; colors, the kitty keyboard protocol and OSC 52 are used when the terminal offers them (Ghostty, kitty, WezTerm, tmux). Icons need a Nerd Font: they start on in Ghostty, kitty and WezTerm and off elsewhere, with a hint to turn them on (`zi`).

## sim in one screen

| | |
|---|---|
| `sim`, `sim dir`, `sim file`, `sim file:42`, `sim +/re file` | open the tree of the current directory, of `dir`, or a file (at a line, at a search) |
| `i a o` … `Esc` | type |
| `Ctrl-s` · `ZZ` · `q` · `ZQ` · `:cq` | save · save and quit · quit · quit discarding · abort a git commit |
| `u` · `U` | undo · redo |
| `/re` `n` `N` `*` | search (incremental, smartcase) |
| `yy` `y{motion}` | copy, also to the system clipboard |
| `]c` `[c` | next / previous change from outside |
| `:'<,'>!cmd` | run the selection through `cmd`; the result replaces it, as one undo step |
| `:seen` | clear the change marks |
| `Ctrl-n` | open / close the tree |
| `j k` `l h` `Enter` `-` | in the tree: move, open / close, open, collapse all |
| `/` in the tree, `Ctrl-p` anywhere | find a file: matches and the folders leading to them light up in place |
| `Ctrl-f` anywhere | search the contents of the files as you type (ripgrep if it is on PATH): results grouped by file in the panel, the file under the cursor previewed with the match bold; `j k` move, `Enter` opens there, `Esc` puts everything back |
| `.` in the tree | only the files that changed, are open, or were opened recently |
| `Tab` `Shift-Tab` | next / previous open file |
| `:wa` `:qa` `:qa!` | save all · quit all · quit all discarding |
| `zh` · `zl` · `zi` | syntax colors on / off · the next look (calm, flat, depth) · icons on / off |
| mouse | click to open or place the cursor, wheel to scroll, drag to select |

Everything else is vi: motions, operators, text objects, counts, `.`, Visual mode, `:s`, ranges. Opening a file again puts you where you left it. With the IME on, full-width keys work as commands; `SIM_ON_NORMAL='fcitx5-remote -c'` turns the IME off whenever you return to Normal mode (`SI_ON_NORMAL`, its name before sim was one program, still works).

Changed files (git, or changed on disk while sim runs) carry the change color in the tree, folders carry the letter of the strongest status below them, and the five files you're most likely to open next carry `•`. In `sim --plain` there is the file, the calm look and the syntax colors, and nothing else.

## Working on it

The pyramid: [CONCEPT](CONCEPT.md) (why) → [SPEC](SPEC.md) (what, with IDs) → `core/src` and `sim/src` (how) → tests (proof). Changes flow down; proof flows up. [CLAUDE.md](CLAUDE.md) holds the rules.

```sh
sh scripts/check.sh          # fmt, clippy, tests, SPEC↔test trace, both line caps, the core's dependencies, binary size
sh scripts/e2e.sh            # the core (as `sim --plain`) and the editor in tmux (own socket)
cargo test --release --workspace -- --ignored bench   # performance requirements
```

Property tests, fuzzers with a no-loss model of every side's text, fault injection at each step of a save, an nvim differential oracle, and goldens of the rendered screen keep the promises honest.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
