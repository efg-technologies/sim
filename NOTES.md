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
- There is no `--help`: an argument sim does not know prints the usage and exits with 2 (CLI-1).
- With `NO_COLOR` or a terminal of fewer than 8 colors the interface is still drawn (boxes, tabs, bar) in weight, underline and inverse; that was not looked at on a real VT100 or a Linux console with a serial line, only on the screens of the tests (COLOR-2, COLOR-3).
