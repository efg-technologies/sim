# NOTES — si

Features we wanted but did not build (CONCEPT: "only written down here").
Each entry: what, why it was wanted, and which principle keeps it out for now.

- similar 3.2 ships `TextMerge`, a structured line-level three-way merge. PROP-3 asks for our own merge built from two line diffs, so it is not used.
- Old snapshots of files that are never reopened (for example random temp files from other tools) stay in `$XDG_STATE_HOME/si/`. Pruning them is not specified (P4).
- An editor for AI agents ("sima", *SIM for Agents*) — considered and not built (P4). Agents edit files through their own tools and need no screen; their writes already reach si as changes on disk, merged and marked without touching unsaved work (PROP-2), and `:ref` hands them locations (AI-1). Reviewing an agent's edit before it lands is what the agent's own harness does (Claude Code's edit approval). If that ever falls short, the smallest step is a convention — an agent leaves a proposed version beside the file and the open si shows it as Proposals — not a new editor.

## Known limits

Found in the last bug hunt before the first release and left as they are, since each fix costs lines that src/ (P4) does not have.

- A very long single line costs time in proportion to its length on every key: about 23 ms at 1 MB and 117 ms at 5 MB, and 700 ms to enter REVIEW over a 3 MB hunk (measured, release). Every queued event also builds a whole frame, though only the last one is drawn.
- A filter's stdout and stderr are kept in memory without a bound: `:%!yes` grows until Ctrl-c ends it.
- `:e` checks what the path is before it opens it, on the main thread: a path swapped for a FIFO between the two hangs si (a race only a deliberate flipper hits; the fix is `O_NONBLOCK` and an `fstat`, about 4 lines).
- A command line being typed is cleared when a slow `:e` load arrives.
- The snapshots of a session that died are not offered back while its pid has been reused by a live process.
- The atomic save does not keep ACLs or extended attributes (that would need a dependency).
