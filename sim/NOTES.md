# NOTES — sim

Features we wanted but did not build yet.

- Persistent undo across sessions (`undofile` in the author's nvim): needs a compact on-disk undo history per file; snapshots already cover crashes.
- Reopen the last session's files when started in the same directory (persistence.nvim in the author's nvim).
- Search across file contents (Telescope live_grep, Ctrl-f in the author's nvim), e.g. through ripgrep.
- Matches of a search shown in place with an underline (`hlsearch`), and trailing blanks made visible (`listchars` trail), both without a hue.
- The characters of a search that matched, shown in the panel (bold or underlined in the file name): `Found::positions` has them already.
- The terminal's own foreground (OSC 10) as the base of the selection's text, instead of a neutral gray.
- Double and triple clicks to select a word and a line, scrolling by dragging past the edge of the text, and scrolling by a notch of the wheel in rows rather than lines when a line wraps (the App's view starts at a line).
- Clicking in the status line or a tab-like list of files to switch files (BufferLine's click in the author's nvim); the position `2/5` is only text now.

## Known limits

Found in the last bug hunt before the first release and left as they are.

- A message that names several files (`no write since last change in a.rs, b.rs (:qa! to discard)`, `not saved: conflicts …`) can lose its instruction at the width when the names are long. The status lines shorten their paths from the left (si's VIS-9 rule); these messages do not yet.
- The narrowed tree (MARK-3) can list recent files that no longer exist, and its cursor can move when the list is rebuilt.
- syntect 5.3 never finishes parsing some lines of JavaScript embedded in HTML, PHP or Textile. JavaScript files themselves are read with TypeScript's syntax, which does not loop. A watchdog gives such a file up as plain after 5 s plus 100 µs per line, and the other files keep their colors, but the thread that hangs cannot be stopped: it keeps a core busy until sim exits.
- The release profile is `panic = "abort"`, so a panic in a background thread would end sim and lose up to a second of typing. The hunt found none.
