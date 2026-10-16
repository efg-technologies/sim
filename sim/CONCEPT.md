# CONCEPT — sim

> si, at home: get to the file, read it, see what changed.

sim is *SI iMproved*, as vim is *Vi IMproved*.

sim is si for your own machine. It keeps everything si guarantees and adds what makes an editor comfortable to live in: a way to reach files, several files open at once, and help with reading code.
si must work anywhere (over ssh, in any terminal, without relying on colors or plugins). sim may assume your own machine: a modern terminal with truecolor and italics, a local file system, git, and a mouse. And it uses color where color helps, as long as the change stays the easiest thing to find (P3).

## Principles

Lower numbers take priority. When two principles conflict, the lower number wins.

| ID | Principle | Meaning |
|---|---|---|
| P1 | Safety | Everything si guarantees (its P1 and every SAFE requirement) holds for every file open in sim |
| P2 | Reach | The fewest steps from "I want that file" to reading it, and from "what changed?" to the change |
| P3 | Signal | The change color stays the easiest thing to find: it is the only chromatic background in the text, and no text color shares its hue. Everything else may use color |
| P4 | Speed | si's targets hold: 1 ms per key. Anything slow (walking a tree, asking git, parsing for highlighting) runs asynchronously |
| P5 | Keys | si's keys, plus the keys the hands know from nvim and VS Code for what si doesn't have |

## Why it works the way it does

| # | Finding | How it shapes the design |
|---|---|---|
| 1 | People retrieve their files mostly by going where they put them (56–68% of retrievals) and search for only 4–15%, and better search engines barely change that (Bergman et al. 2008, ACM TOIS 26(4)) | The way in is a tree of the project, not a search box |
| 2 | Stable layouts let people find things by spatial memory instead of reading (Scarr, Cockburn & Gutwin 2013, FnT HCI 6(1); Scarr et al. 2012, CHI) | The tree never reorders: directories first, then names, and expansion stays as you left it |
| 3 | Marking likely items in place, and search that lights up the path through the hierarchy, cut retrieval time (Fitchett, Cockburn & Gutwin 2013, CHI, best paper) | Typing marks matches and the directories leading to them, in place; nothing moves to a separate list |
| 4 | Recency, frequency and timing together predict the next item better than recency alone, and give more stable lists (Fitchett & Cockburn 2012, AccessRank, CHI) | Likely files are ranked from your own history of opening them, and marked where they are |
| 5 | Narrowing views to the current task made 16 professional programmers measurably more productive (Kersten & Murphy 2006, FSE; Mylyn) | One key narrows the tree to the files that changed or that you opened recently |
| 6 | Developers spend about a third of their time on the mechanics of navigating between relevant code (Ko et al. 2006, IEEE TSE 32(12)) | Reach (P2) ranks right after safety |
| 7 | Syntax highlighting showed no measurable effect on comprehension (Hannebauer et al. 2018, EMSE, n = 390; Beelders & du Plessis 2016, JEMR) and only a small speed gain for novices that fades with experience (Sarkar 2015, PPIG, n = 10) | Highlighting is a convenience, not a claim: on by default, one key turns it off |
| 8 | A target with one unique feature pops out; the more similar and varied the distractors, the slower the search (Treisman & Gelade 1980; Duncan & Humphreys 1989) | Syntax colors are text colors from the terminal's own palette and never the change hue, so the change (a background) stays unique in both dimension and hue (P3) |

No study compares fuzzy finders with trees, or shows that marking changed files helps. Those choices are hypotheses, like si's readability claim, and can be measured the same way. The search of contents (Ctrl-f) is the author's habit from nvim (Telescope's live_grep), not a finding: it is the way in for the 4–15% of retrievals that search (finding 1), and it keeps the tree's promise that nothing moves: the results replace the panel while the search is on, and Esc puts back what was there.

## What sim adds to si

A tree panel of the project, typing to light up matches in it, searching the contents of the files with the results in the panel and a preview in the editor, changed and likely files marked in it, several files open at once (each one a complete si), syntax highlighting in color, the mouse, and three looks.

The looks (`zl` goes round them, and the choice is remembered) differ in how much of the screen is drawn to help the eye find the change. **Calm** is si's own: four styles, one chromatic background (blue, hue 250). **Flat** shows a diff the way GitHub, VS Code and delta do: what comes is on green and what goes on red, and every row of a change has a faint tint behind it (green for a line that gained or changed text, red for a deletion point, a neutral gray for a hunk, which holds both), so that a change is a band across the screen and not only a patch of words. **Depth**, the default, adds btop-like boxes around the tree and the text, an overview ruler on the text's right border, and a gradient: the text fades into the page by its distance from the cursor, the tree by its distance from the open file, and in REVIEW the hunk being decided is raised as a neutral card with its own shadow while whatever is no change fades by its distance from the card. The owner chose red and green here and set color vision aside for these two looks, so P3's single chromatic hue holds in calm only; calm is the look for anyone who cannot tell red from green. Without a palette or truecolor flat is calm and depth is calm in its boxes. Like the rest of sim's choices of look, these are hypotheses: no study compares them with calm, and the M6-style benchmark (time and accuracy to find a change) would.

## What we don't build

LSP, completion, plugins, config files, split windows (tmux does that), a terminal inside the editor, anything that only the mouse can do.

Features we find ourselves wanting are only written down in NOTES.md.

## How big

sim/src was limited to 3,000 lines and is limited to 4,200. The search of contents (GREP-1 to GREP-8) took about 750 of them: a literal smart-case reader of files, a thread that runs ripgrep or reads the files itself and sends what it finds in batches that a newer query cancels, the results in the panel with a preview in the editor pane, and the checks that keep a repository from making that search run its code (ripgrep's config and `--pre`) or lead it out of the tree by a link. That is what the author asked for by name, a key he uses all day in nvim, and the safety checks (P1) are part of the cost, not a thing to cut. The limit is a pressure to keep asking "can we cut it?", not a race, and the author's measure is that the editors stay significantly smaller than Neovim: si and sim together are at most 7,800 lines, about 2% of Neovim's C (about 380,000 lines by GitHub's count). The three looks (LOOK-1 to LOOK-9) took about 660 more: two hues found the way si finds its change color, the tints and the fade in OKLab, the boxes with their titles, badges and keys, the overview ruler, the raised card, the gradients of the text and of the tree, and the Painter's side of all of it. The author asked for each of them, they serve P3 (the change is the easiest thing to find) and P2 (what changed, at a glance), which rank above the size, and nothing of them reaches si's `src/`, which is at its own cap (only the visibility of `App::current` changed).
