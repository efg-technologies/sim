# NOTES — si

Features we wanted but did not build (CONCEPT: "only written down here").
Each entry: what, why it was wanted, and which principle keeps it out for now.

- similar 3.2 ships `TextMerge`, a structured line-level three-way merge. DISK-3 asks for our own merge built from two line diffs, so it is not used.
- Old snapshots of files that are never reopened (for example random temp files from other tools) stay in `$XDG_STATE_HOME/si/`. Pruning them is not specified (P4).

## Known limits

Found in the last bug hunt before the first release and left as they are, since each fix costs lines that src/ (P4) does not have.

- A very long single line costs time in proportion to its length on every key: about 23 ms at 1 MB and 117 ms at 5 MB. Every queued event also builds a whole frame, though only the last one is drawn.
- A filter's stdout and stderr are kept in memory without a bound: `:%!yes` grows until Ctrl-c ends it.
- `:e` checks what the path is before it opens it, on the main thread: a path swapped for a FIFO between the two hangs si (a race only a deliberate flipper hits; the fix is `O_NONBLOCK` and an `fstat`, about 4 lines).
- A command line being typed is cleared when a slow `:e` load arrives.
- The snapshots of a session that died are not offered back while its pid has been reused by a live process.
- The atomic save does not keep ACLs or extended attributes (that would need a dependency).
