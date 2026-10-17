# NOTES — sim

Features we wanted but did not build (CONCEPT: "only written down here").
Each entry: what, why it was wanted, and which principle keeps it out for now.

The core's:

- similar 3.2 ships `TextMerge`, a structured line-level three-way merge. DISK-3 asks for our own merge built from two line diffs, so it is not used.
- Old snapshots of files that are never reopened (for example random temp files from other tools) stay in `$XDG_STATE_HOME/sim/`. Pruning them is not specified (P4).

The editor's:

- Persistent undo across sessions (`undofile` in the author's nvim): needs a compact on-disk undo history per file; snapshots already cover crashes.
- Reopen the last session's files when started in the same directory (persistence.nvim in the author's nvim).
- The query of the picker edited in place (the cursor in the middle of it; it is typed at its end, as FIND-1's is, with only `Ctrl-w` and `Ctrl-u` to edit it), a click on a result (the mouse does nothing while the picker is open), scrolling the preview, and a preview of a file bigger than 2 MiB.
- Matches of a search shown in place with an underline (`hlsearch`), and trailing blanks made visible (`listchars` trail), both without a hue.
- The characters of a search that matched, shown in the panel (bold or underlined in the file name): `Found::positions` has them already.
- The terminal's own foreground (OSC 10) as the base of the selection's text, instead of a neutral gray.
- Double and triple clicks to select a word and a line, scrolling by dragging past the edge of the text, and scrolling by a notch of the wheel in rows rather than lines when a line wraps (the App's view starts at a line).
- The mockups of depth also fade the text beside the panel (to about 60%) while the panel has the focus during a search, as the panel recedes while the text has it; only the panel's side is built (LOOK-7), because the text is what is being read while the tree is browsed.
- The position and the mode (`1:1  NORMAL`) in the editor's bottom border, and a hint such as `^p find` in the panel's, as the mockups have; the core's status line stays whole instead (LOOK-3), and the borders are plain in Normal mode.
- Clicking in the status line or a tab-like list of files to switch files (BufferLine's click in the author's nvim); the position `2/5` is only text now.

The window's (`sim-gui`, part three of SPEC):

- A native clipboard (smithay-clipboard or arboard) instead of `wl-copy`, `wl-paste`, `xclip`, `pbcopy` and `pbpaste` found on `PATH` (GUI-10); no drag and drop and no primary selection. A crate for it is a dependency and a window-system connection of its own (the window's P1), so the tools stand in.
- The system's own reduced-motion setting (only `SIM_GUI_REDUCED_MOTION` is read, GUI-13), and an accessibility tree: a window of pixels has none, and the terminal's screen readers read its text; this needs a design of its own.
- The tree's scroll is not animated (only the text's). Wide-glyph kerning, ligatures and emoji presentation are cosmic-text's defaults; color emoji are rasterized by swash and have not been looked at.
- The scrollbar cannot be dragged: sim ignores a click in its column in the terminal too. Tabs, the tree and the picker's list are clicked through sim's own mouse handling.
- Window decorations of its own, a title that follows the file, several windows, session restore.
- A recovery from a lost GPU device (a lost surface is recovered).
- The scroll's state in the `Screen` (the top line, the rows skipped) so that the animator need not compare the rows' text to see how far the text moved: it hashes the text after the gutter, and a file of identical lines can fool it into no scroll.
- Input types of sim's own (`Key`, modifiers, a mouse event with a cell and a button) instead of crossterm's, with `convert_all` taking them: the window carries crossterm and `mio` only for three plain types.
- Windows (the operating system): `wgpu` is built with Vulkan and Metal only, so there is no DX12 backend.

## Known limits

Found in the last bug hunt before the first release and left as they are, since each fix costs lines that the caps (P4) do not have.

The core's:

- A very long single line costs time in proportion to its length on every key: about 23 ms at 1 MB and 117 ms at 5 MB. Every queued event also builds a whole frame, though only the last one is drawn.
- A filter's stdout and stderr are kept in memory without a bound: `:%!yes` grows until Ctrl-c ends it.
- `:e` checks what the path is before it opens it, on the main thread: a path swapped for a FIFO between the two hangs the core (a race only a deliberate flipper hits; the fix is `O_NONBLOCK` and an `fstat`, about 4 lines).
- A command line being typed is cleared when a slow `:e` load arrives.
- The snapshots of a session that died are not offered back while its pid has been reused by a live process.
- The atomic save does not keep ACLs or extended attributes (that would need a dependency).

The editor's:

- Which rows have a change (LOOK-2) is read from the gutter, which says nothing of rows that have scrolled out of the frame: the first rows of a changed line taller than the screen, scrolled partly above it, are taken for rows of no change (no tint).
- A message that names several files (`no write since last change in a.rs, b.rs (:qa! to discard)`, `not saved: conflicts …`) can lose its instruction at the width when the names are long. The status lines shorten their paths from the left (the core's VIS-9 rule); these messages do not yet.
- The narrowed tree (MARK-3) can list recent files that no longer exist, and its cursor can move when the list is rebuilt.
- syntect 5.3 never finishes parsing some lines of JavaScript embedded in HTML, PHP or Textile. JavaScript files themselves are read with TypeScript's syntax, which does not loop. A watchdog gives such a file up as plain after 5 s plus 100 µs per line, and the other files keep their colors, but the thread that hangs cannot be stopped: it keeps a core busy until sim exits.
- The release profile is `panic = "abort"`, so a panic in a background thread would end sim and lose up to a second of typing. The hunt found none.
- The search of contents checks that a file is a regular file below the root before it opens it, and does not hold the file's directory while it reads: a file swapped for a link in between would be followed (a race that only a deliberate flipper hits, like the FIFO one of the core's `:e`; closing it needs `O_NOFOLLOW`, whose value differs by platform and by architecture, so a dependency).
- Plain mode runs no program of its own, with one exception: on a system without `/proc` (macOS) the journal runs `kill -0` to ask whether the process of an old snapshot is alive. `sudoedit` is recognised by the name of the parent process in `/proc`, so on macOS only the uid (root) turns plain mode on by itself; `SUDO_EDITOR='sim --plain'` says it there (PLAIN-1).
- Icons start on only where the environment names Ghostty, kitty or WezTerm. A terminal that has the symbols some other way (iTerm2 with a patched font, Alacritty with a Nerd Font set as its font, Windows Terminal) gets the hint and `zi`, once. Over ssh the variables do not travel, so the hint comes there too (ICON-1). Not tried on a real terminal other than the author's.
- A submodule is one entry with its status (GIT-4), and git runs with `--ignore-submodules=dirty`: a submodule whose commit has not moved shows nothing though its own work tree is dirty, and a moved one has a letter and no counts. Showing what is inside needs a child git under the submodule's own config, which the guard cannot read.
- Neutralizing a filter (GIT-2) makes a file that it converts look changed: an LFS file whose filter is not exactly what `git lfs install` writes (`--skip-smudge`, a changed command), or whose repository sets an `lfs.*` key outside git-lfs's harmless ones (`lfs.storage`, `lfs.extension.*`, `lfs.customtransfer.*`, a key it does not have yet), shows ` M`, and so does a file of any other filter that changes its content. The exception for git-lfs looks at the `lfs.*` keys only, not at the urls of a `remote.<name>.lfspushurl` (git-lfs's clean writes locally and asks nothing of a remote). It runs `git-lfs` along the user's PATH, which sim does not filter: an empty or relative entry of it would find a program in the repository, as it finds `git` itself.
- `include` and `includeIf` in a repository's own config still make sim skip git (GIT-3), though git's listing expands them: what a tracked file brings in can change between the listing and the command.
- The changes view (DIFF-1 to DIFF-4): a rename git does not have in the index (a moved file that was not `git add`ed) is a deleted and an untracked file, two rows; only the first 1,000 untracked files have their lines counted, none over 1 MiB, and a binary one has no count; the blob of a deleted file over 20 MiB is not shown. Before the first commit the numbers are against the empty tree. `<Space>ge` needs the panel to have the focus or a file to be open (Space is a leader there): Ctrl-n first.
- Which-key is one column. A prefix with more rows than the screen has room for (an operator's motions and objects on a short screen) lists what fits and ends in `… n more (:help)`; a second column is not built. It opens for the prefixes of Normal and Visual mode and the tree, not for the char `f`, `t` and `r` wait for, not in Insert mode and not for a count.
- The search count (`[3/12]`, UI-17) is kept for a file of at most 5,000 lines (`HIT_LINES`, the scrollbar's own limit, UI-11) and counts only the first 100 matches, so more than 99 are `>99` and the total of a file with more is not known; a larger file shows no count. It is found again when the text changes, not while it is typed.
- The hint for Tab (KEY-6) counts a use only for Tab and Shift-Tab that showed another file; `Space b p` and a click on a tab teach nothing, so a user who only uses those sees the hint in each run until Tab has been used three times. It is a message, not a window, and the next key puts it out.
- `Space` leads and F1 opens the help in `sim --plain` too (they run no program and need no panel); there `Space b p` and `Space b c` have one file to pick, and the letters go on the status line.
- `Space b c` closes through `:q`, so a tab with changes stays (with `:q`'s message) and its file is the one shown; closing it with the changes thrown away is `:q!` in that file.
- The help (`:help`, F1) lists the keys and commands of the table; the README's table of keys is written by hand and not yet made from it.
- With `NO_COLOR` or a terminal of fewer than 8 colors the interface is still drawn (boxes, tabs, bar) in weight, underline and inverse; that was not looked at on a real VT100 or a Linux console with a serial line, only on the screens of the tests (COLOR-2, COLOR-3).

The window's:

- **The window has not been seen on a real display.** It compiles, starts and fails cleanly with no display, and everything it is made of is tested offscreen (the scenes, the pixels on a software adapter, the input end to end into a running sim); but no event loop has run on Wayland, X11 or macOS. The surface format, `Fifo` presentation, the redraw cadence while animating, the input method's enabling and the place of its candidate window are written, not observed. The input method itself has been tried only as text arriving (`Ime::Commit`) and as preedit state; its drawing is in the Japanese picture, but no input method produced it.
- The pixel goldens are llvmpipe's, compared with a tolerance of 8 levels on all but one in a thousand pixels; another software adapter's rounding may need them blessed again (`SIM_GUI_BLESS=1`). The scene goldens are exact and change when sim's looks change, which is their job.
- Frame times were measured offscreen (submit to GPU idle, one frame at a time) on one discrete GPU; a window's present (vsync, the compositor), the first frame (a cold font cache, about 15 ms for the glyphs of a first screen at 1080p) and a screen of CJK text are not measured.
- The clipboard needs one of the tools above on `PATH`; with none, a paste and a yank do nothing outside the window.
