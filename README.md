**English** | [日本語](README.ja.md) | [简体中文](README.zh-cn.md) | [繁體中文](README.zh-tw.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Português (Brasil)](README.pt-br.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Русский](README.ru.md)

# sim, the next-generation Vim

**sim** (also called *sim editor*, short for *Secure vIM*) is the next-generation Vim: written from scratch in Rust, it keeps Vim's modal editing and core keys. It reads no modelines, loads no plugins and opens no network connection, and it puts on screen what is usually hidden: invisible characters, writes from outside the editor, repositories that would run code.

**No plugins. No modelines. No network.**

Website: <https://sim.efg-technologies.com> · Linux and macOS · MIT OR Apache-2.0 · [Changelog](CHANGELOG.md) · [Security policy](SECURITY.md)

Everything sim promises, it promises for every file open in it: **without an explicit action, nobody's text is lost**, yours or another process's.

## Install

The binary is `sim`. The package is `sim-editor` on the AUR and crates.io, and `efg-technologies/tap/sim` on Homebrew; the library it is built on is `sim-editor-core` on crates.io.

sim 0.1.0 is out. Install it with Homebrew:

```sh
brew install efg-technologies/tap/sim
```

The source of the release is on the [v0.1.0 release page](https://github.com/efg-technologies/sim/releases/tag/v0.1.0) (`sim-0.1.0.tar.gz`). To build it from source:

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

The AUR package `sim-editor` and the crates.io crates `sim-editor` and `sim-editor-core` are coming.

Then, if you like it as your editor:

```sh
export EDITOR=sim                     # Claude Code's Ctrl-g, crontab -e, …
git config --global core.editor sim   # git commit (ZZ commits, :cq aborts)
export SUDO_EDITOR='sim --plain'      # sudoedit (sim is plain there by itself on Linux)
```

Needs Rust 1.88 or later. Runs on Linux and macOS in any terminal; colors, the kitty keyboard protocol and OSC 52 are used when the terminal offers them (Ghostty, kitty, WezTerm, tmux). git and ripgrep are optional: sim uses them when they are on `PATH`. Icons need a Nerd Font; they start on in Ghostty, kitty and WezTerm and off elsewhere, with a one-time hint, and `zi` turns them on and off. More on the [install page](https://sim.efg-technologies.com/install/).

## What it does

**Shows what is hidden**

- A right-to-left override, a zero-width space or a control character is drawn as `<U+202E>` or `^[` on a magenta block, in every state a cell can be in, and is never sent raw to the terminal. The literal text `<U+202E>` in a file gets no block.
- Trailing blanks show as `·`, a no-break space as `⍽`, a tab as `»`.
- When another process rewrites the open file, the changed lines flash for a moment and are then marked in the text, the box says how many changes are on disk, the tab shows `↯`, and the tree marks the file. `]c` and `[c` go to the next and the previous mark.
- Changes against git HEAD show as gutter signs, a status letter for each file in the tree (a folder shows the letter of the strongest status below it) and `+n ~n -n` in the status line.
- git is not run where the repository's own config could run a command (`core.fsmonitor`, a filter driver, and more): the box says `git not run` on the alert's block, and the status line names the key and the file. Where git does run with the repository's own `core.fsmonitor`, hooks path, filters or external diff turned off, the box says how many (`2 git settings off`).
- A Markdown link whose text names an address that its target is not (`[https://good.example](https://evil.example)`, a reference link `[good.example][1]` whose definition says another place, a link that goes over lines or has a backslash or an entity in its target, a raw HTML link `<a href="…">`) has its target, and for a reference its text, on the magenta block, in the read view and in calm too; the read view never hides a link's brackets or URL.

**Refuses what it should, keeps your text**

- Settings and commands are never read from file contents: no modelines, no config files, no plugins, no network.
- Saves are atomic (temporary file, fsync, rename, directory fsync) and check the file's hash first: if the file changed on disk, nothing is overwritten. When a save finds a conflict, `:w!` keeps yours after copying theirs to the state directory, and `:e!` takes theirs after a snapshot of yours.
- Unsaved changes are snapshotted every second; after a crash, `kill -9` or a closed terminal, reopening the file offers them back as one undo step.
- Search finds ripgrep itself, runs it with an empty environment and `--no-config`, and never follows a symlink out of the project.
- `sim --plain` is the core alone: one file, no panel, and no external program (no git, no ripgrep, no filter, no hook). It is the default as root and as the editor of sudoedit.

**Reach, speed and reading**

- A project tree that never moves, `Ctrl-p` to find files, `Ctrl-f` to search text with a preview, likely files marked with `•`, several files with a tab bar.
- In the project's own benchmark a key takes under 1 ms at p99 (100,000-line file, release build); the first frame in a repository of 100,000 files was measured at 1.8 to 8.7 ms.
- Syntax colors from your terminal's palette, Markdown read in place (markers hidden, tables aligned, and the line you are on always the file's own text), three looks (`zl`), Japanese kinsoku wrapping and full-width keys.
- The editor's title is a breadcrumb, `src › sim.rs › leader_key`: the path and the heading or function the cursor is in. A fact is in one place: the status line has the mode, the branch, the counts and the position, and says `CRLF`, `BOM` or a file that is not UTF-8 only when it is one; the tab has the file and `●`, the scrollbar the position, and the tree is not in fog.
- `NO_COLOR` and terminals with fewer than 8 colors draw with weight, underline and inverse video only, and meaning never depends on color alone: a changed line is `┆`, folders show status letters, a tab shows `↯` for a file changed on disk, and the scrollbar's handle is a `║` rail, not an inverse block.

**Keys you can find**

- `Space` is the leader key. A pause on a prefix (`Space`, `g`, `z`, `Z`, `[`, `]`, or an operator such as `d`) opens a small which-key window that lists the keys that can come next; typing on works exactly as without it.
- `:help` and `F1` open a searchable list of every key and command, with what each one does; `:help tab` finds `Tab`, `Shift-Tab` and `<Space>bp`.
- `<Space>bp` puts a letter on each tab, and the letter jumps to its file; `<Space>bc` does the same to close one (a file with changes stays open, as with `:q`). A hint names `Tab` and `Shift-Tab` the first time a second file is open, until you have used them three times.
- While the matches of a search are lit, the status line says which one you are at and how many there are, `[3/12]`.
- `sim --help` prints the usage and the environment sim reads (`NO_COLOR`, `SIM_ON_NORMAL`, `XDG_STATE_HOME`); `sim --version` prints the version. Neither touches the terminal.

## sim in one screen

| Keys | What they do |
|---|---|
| `i` | insert |
| `Ctrl-s` | save |
| `ZZ` | save and quit |
| `q` | quit |
| `ZQ` | quit without saving |
| `u` | undo |
| `U` | redo |
| `Y` | yank to end of line |
| `/` | search forward |
| `n` | next match |
| `*` | search word forward |
| `yy` | yank lines |
| `]c` | next change from outside |
| `[c` | previous change from outside |
| `:s` | substitute |
| `:{range}!cmd` | filter |
| `:seen` | clear change marks |
| `Ctrl-n` | toggle the tree |
| `Ctrl-p` | find files |
| `Ctrl-f` | search file contents |
| `.` | changed and recent files (in the tree) |
| `Tab` | next file |
| `Shift-Tab` | previous file |
| `zh` | syntax colors on/off |
| `zl` | next look |
| `zi` | icons on/off |
| `<Space>` | leader |
| `<Space>bp` | pick a tab by letter |
| `<Space>bc` | pick a tab to close |
| `<Space>ge` | changes from HEAD |
| `<Space>gi` | what the guard did to this repository |
| `<Space>um` | markdown read view on/off |
| `F1` | help |
| `:help` | help |

Everything else is vi: motions, operators, text objects, counts, `.`, Visual mode, `:s`, ranges. `Y` yanks to the end of the line, `U` redoes, and every yank also reaches the system clipboard through OSC 52, also through tmux. Opening a file again puts you where you left it. With the IME on, full-width keys work as commands; `SIM_ON_NORMAL='fcitx5-remote -c'` turns the IME off whenever you return to Normal mode (`SI_ON_NORMAL`, its old name, still works). Every key is in one table, `sim/keys.tsv`, which `:help` and which-key read, and which the tests check against what the keys do. The full list is on the [docs page](https://sim.efg-technologies.com/docs/).

## Security model

You open files and repositories you did not write. The attacker controls their content, not your machine. sim tries to make sure that content cannot hide from you, cannot reconfigure the editor, and cannot make the editor run code. It is **not a sandbox**: it runs with your privileges, it does not detect look-alike letters from other scripts, and its claims come from the project's own spec and tests, not from an independent audit. Language servers are not built yet; a sandboxed client is planned. To report a vulnerability, read the [security policy](SECURITY.md).

The full threat model, one threat at a time, is on the [security page](https://sim.efg-technologies.com/security/). Three long articles go deeper: [hidden and bidirectional characters](https://sim.efg-technologies.com/blog/hidden-characters/), [repositories that run code](https://sim.efg-technologies.com/blog/repositories-that-run-code/), and [safe saves and secrets](https://sim.efg-technologies.com/blog/safe-saves/).

## Compared with Vim, Neovim and Helix

Vim, Neovim and Helix are mature editors and sim is new. Only what a primary source says is compared; the [comparison page](https://sim.efg-technologies.com/compare/) cites all of it (checked on 2026-10-17: Vim 9.2.1202, Neovim 0.12.6, Helix 25.07.1).

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| Extensions | Vim script, packages | Lua, Vim script | no plugin system in a release; Steel PR [#8675](https://github.com/helix-editor/helix/pull/8675) is a draft | none, by design |
| Modelines | on (off for root); `'modelinestrict'` since [9.2.0350](https://github.com/vim/vim/commit/4c2879471) | on (off for root) | not parsed ([#729](https://github.com/helix-editor/helix/issues/729)) | never read |
| Project-local config | `'exrc'`, off by default | `'exrc'`, trust list | `.helix/config.toml` loaded in 25.07.1; trust is on the development branch | none exists |
| U+202E and friends | `<202e>` | `<202e>` | no marker in 25.07.1 (our test; [#11255](https://github.com/helix-editor/helix/issues/11255)) | `<U+202E>` on a magenta block |

What the others still do better, for now: Neovim and Helix ship a language-server client, Vim and Neovim have plugins and macros. sim has none of these yet.

## Roadmap (planned, not shipped)

Vim registers, macros and marks · clipboard integration beyond OSC 52 · a sandboxed language-server client · `:tutor` · a git diff tree · a GUI with real depth (a prototype) · packages for the AUR and crates.io.

## Working on it

sim is one program in two crates: the **core** (`sim-editor-core`, in `core/`: the buffer, vi's grammar, saves, the merge with what other processes write, the terminal) and the **editor** (`sim-editor`, in `sim/`: the binary `sim`, with the tree, finding, search, marks, syntax, the looks and the mouse). The why behind each choice is in [CONCEPT.md](CONCEPT.md), with the research it rests on. What sim must do is in [SPEC.md](SPEC.md): every requirement has an ID, and every ID has tests.

The pyramid: [CONCEPT](CONCEPT.md) (why) → [SPEC](SPEC.md) (what, with IDs) → `core/src` and `sim/src` (how) → tests (proof). Changes flow down; proof flows up. [CLAUDE.md](CLAUDE.md) holds the rules.

```sh
sh scripts/check.sh          # fmt, clippy, tests, SPEC↔test trace, both line caps, the core's dependencies, binary size
sh scripts/e2e.sh            # the core (as `sim --plain`) and the editor in tmux (own socket)
cargo test --release --workspace -- --ignored bench   # performance requirements
```

Property tests, fuzzers with a no-loss model of every side's text, fault injection at each step of a save, an nvim differential oracle, and goldens of the rendered screen keep the promises honest.

## License

MIT OR Apache-2.0, at your option: [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE). Copyright EFG Technologies.
