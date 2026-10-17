# CONCEPT — si

> A vi for reading, where only the changes light up.

Files that AI and other processes rewrite: a human reads them as fast as possible, fixes them, and saves them without losing anything.
It is meant to be `$EDITOR` (git commit, Claude Code's prompt editing, quick fixes). Open it for a moment, then close it.

The name: si is short for *SIght*, as vi is for *VIsual* — and 視 (shi), to look closely, in Japanese. sim is *SI iMproved*, as vim is *Vi IMproved*.

## Principles

Lower numbers take priority. When two principles conflict, the lower number wins.

| ID | Principle | Meaning |
|---|---|---|
| P1 | Safety | Without an explicit action, nobody's text is lost (yours or another process's) |
| P2 | Understanding | Make what changed obvious at a glance. Color is used only for change |
| P3 | Speed | 5 ms to start, 1 ms per key. Anything that would make you wait runs asynchronously |
| P4 | Minimal | src/ stays within 3,425 lines. Every feature is judged by "can we cut it?" |
| P5 | Keys | The best key for each job, not vi for its own sake: keep the vi grammar the hands already know, and depart from it wherever vi is worse than modern editors. |

## External changes

- **A change on disk is a fact.** The parts that don't conflict with your edits are merged automatically and marked where they changed, word by word, until you have looked at them
- **A conflict is never merged away.** Your text stays, marked where the disk differs, and the file on disk is not written over: both versions are kept until you choose. `:w!` keeps yours and `:e!` takes the disk's, and each copies the version it gives up into the state directory first, so the choice itself loses nothing

## Why it looks the way it does

| # | Finding | How it shapes the design |
|---|---|---|
| 1 | Syntax highlighting does not improve comprehension (Hannebauer et al. 2018, 390 students; Beelders & du Plessis 2016, eye tracking) | No syntax highlighting |
| 2 | An item that differs by a single color is found instantly, regardless of how many items there are (Treisman & Gelade 1980) | Exactly one chromatic color, reserved for "change" |
| 3 | Without a cue, people fail to notice changes on screen (change blindness; Rensink et al. 1997) | Marks for external changes stay until you have looked at them |
| 4 | About 5% of Japanese men have a color vision deficiency (Japan Ophthalmologists Association) | Besides color, gutter symbols and strikethrough also show change |
| 5 | Legibility is determined by perceived contrast (APCA) | Body text at Lc 90 and secondary text at Lc 60, computed from the terminal's background |
| 6 | Lines of 45–75 characters are the most readable for Latin text (Bringhurst) | Prose wraps at 72 columns |
| 7 | Indentation of 2–4 columns gives the best comprehension (Miara et al. 1983) | Tabs are displayed as 4 columns |
| 8 | Light backgrounds favor proofreading accuracy, especially with small text (Piepenbrock et al. 2014) | The background follows the terminal, and both light and dark meet the targets |

"More readable than existing editors" is a hypothesis, not a claim. The M6 perception benchmark (time and accuracy to find a change) tests it.

## What we don't build

A GUI (Ghostty draws), split windows (tmux does that), config files, plugins, syntax highlighting, LSP, completion, a file tree, multiple buffers, macros, the mouse, networking.

Features we find ourselves wanting are only written down in NOTES.md.

## si and sim

si is the editor that works anywhere: over ssh, in any terminal, without relying on colors or plugins.
Comfort on your own machine (a way to find files, syntax highlighting, and the like) belongs to `sim`, a separate command built on si's core with its own CONCEPT and SPEC (in `sim/`). si's limits, such as the 3,425 lines of src/, apply to si alone.

The limit was 3,000 until M3. It was raised to 3,300 because what pushed past it was safety (P1: races between saves, reloads and the watcher, a private state directory, compare-and-swap saves) and correctness (P2: widths of CJK and emoji, lines taller than the screen), both of which rank above P4. For scale: Neovim's C alone is about 11.5 MB (GitHub's count, roughly 380,000 lines), so si stays near 1% of it. The number is a pressure to keep asking "can we cut it?", not a race. After M4 it was raised to 3,600 by the author's own measure, "significantly smaller than Neovim": filters, reopening where you left off and incremental search came from the author's requests and nvim habits, and 3,600 lines is still under 1% of Neovim's C. On 2026-10-17 it moved by three lines, to 3,603, with the owner's agreement that limits move for a stated reason and are not to be overdone: a char that is shown in another form (SAFE-8) has a style of its own, `Style::Hidden`, so that sim can draw it in the alert's color and a file that holds the literal text `<U+202E>` no longer looks like the real control char (P1). The same day the owner took the AI-only features out (the proposals, REVIEW mode, `:ref`, `contrib/ai`): a conflict with the disk is now kept as it is, with both versions recoverable, and the limit came down to what was left, 3,425 lines (SAFE-7). 178 lines went: the proposals, their word diffs and their weave into the view, REVIEW's keys and status, and `:ref` freed about 330; the copies of SAFE-7 (`Effect::Keep`, `Event::Kept`, the journal's `keep`, the reload that waits for its copy) and a filter's result that is merged in place took about 150.
