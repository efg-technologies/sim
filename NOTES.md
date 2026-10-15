# NOTES — si

Features we wanted but did not build (CONCEPT: "only written down here").
Each entry: what, why it was wanted, and which principle keeps it out for now.

- similar 3.2 ships `TextMerge`, a structured line-level three-way merge. PROP-3 asks for our own merge built from two line diffs, so it is not used.
- Old snapshots of files that are never reopened (for example random temp files from other tools) stay in `$XDG_STATE_HOME/si/`. Pruning them is not specified (P4).
