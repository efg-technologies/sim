# sim — si iMproved

**si** (*SIght*) is a vi for reading, where only the changes light up. **sim** (*SI iMproved*) is si for your own machine — the way vi (*VIsual*) and vim (*Vi IMproved*) go together.

Files that AI and other processes rewrite: you read them as fast as possible, fix them, and save them without losing anything.

This repository holds two editors built on one core:

| | **si** | **sim** |
|---|---|---|
| For | `$EDITOR` anywhere: git commit, Claude Code's prompt editing, quick fixes, over ssh | your own machine, all day |
| Looks | no syntax highlighting; color is used only for change | syntax colors from your terminal's palette, never the change color's hue; three looks (`zl`): calm, flat (a diff in green and red with tinted rows) and depth (btop-like boxes, a gradient, a raised card in REVIEW) |
| Adds | — | a project tree, typing to find files and to search their contents, changed and likely files marked, several files open, the mouse |
| Size | ≤ 3,600 lines of Rust (~1% of Neovim's C) | ≤ 3,500 more |

Both keep the same promise: **without an explicit action, nobody's text is lost** — yours, another process's, an AI's.

## What makes it different

- **Changes from outside are Proposals.** When a file changes on disk while you edit it, si merges what doesn't conflict (and marks it, word by word, until you've looked at it) and turns conflicts into Proposals. Output from a filter or an AI is always a Proposal. `]c` goes to the next one; `y` accepts, `n` rejects, `Y` / `N` decide all, `u` undoes a decision.
- **Saves that can't lose.** Saves are atomic (temp file, fsync, rename, directory fsync) and compare-and-swap: if the file changed since you read it, nothing is overwritten. Unsaved changes are snapshotted every second; after a crash, `kill -9` or a closed terminal, reopening the file offers them back as one undo step.
- **Readable by measurement.** Text is 90 Lc and secondary text 60 Lc (APCA), computed from your terminal's background; the one change color stays distinguishable under protan, deutan and tritan color vision; prose wraps at 72 columns with Japanese kinsoku.
- **Keys your hands already know.** The vi grammar, minus vi's accidents: `Y` yanks to the end of the line, `U` redoes, search is smartcase, every yank reaches the system clipboard (also through tmux), Ctrl-s saves, `q` quits, arrows and Home/End work everywhere.

The why behind each choice is in [CONCEPT.md](CONCEPT.md) (si) and [sim/CONCEPT.md](sim/CONCEPT.md) (sim), with the research it rests on. What each editor must do is in [SPEC.md](SPEC.md) and [sim/SPEC.md](sim/SPEC.md): every requirement has an ID, and every ID has tests.

## Install

```sh
git clone https://github.com/efg-technologies/sim.git && cd sim
cargo install --locked --path .      # si
cargo install --locked --path sim    # sim
```

Then, if you like it as your editor:

```sh
export EDITOR=si                     # Claude Code's Ctrl-g, crontab -e, …
git config --global core.editor si   # git commit
```

Needs Rust 1.88 or later (the code uses `let` chains). Runs on Linux and macOS, in any terminal; colors, the kitty keyboard protocol and OSC 52 are used when the terminal offers them (Ghostty, kitty, WezTerm, tmux).

## si in one screen

| | |
|---|---|
| `si file`, `si file:42`, `si +/re file` | open (at a line, at a search) |
| `i a o` … `Esc` | type |
| `Ctrl-s` · `ZZ` · `q` · `ZQ` · `:cq` | save · save and quit · quit · quit discarding · abort a git commit |
| `u` · `U` | undo · redo |
| `/re` `n` `N` `*` | search (incremental, smartcase) |
| `yy` `y{motion}` | copy, also to the system clipboard |
| `]c` `[c` | next / previous change or Proposal; on a Proposal: `y` `n` `Y` `N` |
| `:'<,'>!cmd` | run the selection through `cmd`; the result comes back as Proposals |
| `:ref` | copy `path:12-15` for an agent |
| `:seen` | clear the change marks |

Everything else is vi: motions, operators, text objects, counts, `.`, Visual mode, `:s`, ranges. Opening a file again puts you where you left it. With the IME on, full-width keys work as commands; `SI_ON_NORMAL='fcitx5-remote -c'` turns the IME off whenever you return to Normal mode.

### An AI as a filter

[contrib/ai](contrib/ai) is a small script around `claude -p`. Put it on your PATH and:

```
:'<,'>!ai 'fix the typos'
```

The answer arrives as Proposals while you keep editing; review them with `]c`, then `y` or `n`.

## sim in one screen

| | |
|---|---|
| `sim`, `sim dir`, `sim file` | open the tree of the current directory, of `dir`, or a file |
| `Ctrl-n` | open / close the tree |
| `j k` `l h` `Enter` `-` | move, open / close, open, collapse all |
| `/` in the tree, `Ctrl-p` anywhere | find a file: matches and the folders leading to them light up in place |
| `Ctrl-f` anywhere | search the contents of the files as you type (ripgrep if it is on PATH): results grouped by file in the panel, the file under the cursor previewed with the match bold; `j k` move, `Enter` opens there, `Esc` puts everything back |
| `.` in the tree | only the files that changed, are open, or were opened recently |
| `Tab` `Shift-Tab` | next / previous open file |
| `:wa` `:qa` `:qa!` | save all · quit all · quit all discarding |
| `zh` | syntax colors on / off |
| mouse | click to open or place the cursor, wheel to scroll, drag to select |

Changed files (git, or changed on disk while sim runs) carry the change color in the tree, with `~` on the folders above them; the five files you're most likely to open next carry `•`.

## Working on it

The pyramid: [CONCEPT](CONCEPT.md) (why) → [SPEC](SPEC.md) (what, with IDs) → `src/` (how) → tests (proof). Changes flow down; proof flows up. [CLAUDE.md](CLAUDE.md) holds the rules.

```sh
sh scripts/check.sh          # fmt, clippy, tests, SPEC↔test trace, line cap, binary size (si)
sh sim/scripts/check.sh      # the same for sim
sh scripts/e2e.sh            # si in tmux (own socket)
sh sim/scripts/e2e.sh        # sim in tmux
cargo test --release -- --ignored bench   # performance requirements
```

Property tests, fuzzers with a no-loss model of every side's text, fault injection at each step of a save, an nvim differential oracle, and goldens of the rendered screen keep the promises honest.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
