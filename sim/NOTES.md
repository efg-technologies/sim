# NOTES — sim

Features we wanted but did not build yet.

- Persistent undo across sessions (`undofile` in the author's nvim): needs a compact on-disk undo history per file; snapshots already cover crashes.
- Reopen the last session's files when started in the same directory (persistence.nvim in the author's nvim).
- Search across file contents (Telescope live_grep, Ctrl-f in the author's nvim), e.g. through ripgrep.
- Matches of a search shown in place with an underline (`hlsearch`), and trailing blanks made visible (`listchars` trail), both without a hue.
- The characters of a search that matched, shown in the panel (bold or underlined in the file name): `Found::positions` has them already.
- The terminal's own foreground (OSC 10) as the base of the selection's text, instead of a neutral gray.
