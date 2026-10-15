# NOTES — si

Features we wanted but did not build (CONCEPT: "only written down here").
Each entry: what, why it was wanted, and which principle keeps it out for now.

- similar 3.2 ships `TextMerge`, a structured line-level three-way merge. PROP-3 asks for our own merge built from two line diffs, so it is not used.
- Old snapshots of files that are never reopened (for example random temp files from other tools) stay in `$XDG_STATE_HOME/si/`. Pruning them is not specified (P4).
- An editor for AI agents ("sima", *SIM for Agents*) — considered and not built (P4). Agents edit files through their own tools and need no screen; their writes already reach si as changes on disk, merged and marked without touching unsaved work (PROP-2), and `:ref` hands them locations (AI-1). Reviewing an agent's edit before it lands is what the agent's own harness does (Claude Code's edit approval). If that ever falls short, the smallest step is a convention — an agent leaves a proposed version beside the file and the open si shows it as Proposals — not a new editor.
