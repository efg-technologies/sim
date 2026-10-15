# CONCEPT — sim

> si, at home: get to the file, read it, see what changed.

sim is si for your own machine. It keeps everything si guarantees and adds what makes an editor comfortable to live in: a way to reach files, several files open at once, and help with reading code.
si must work anywhere (over ssh, in any terminal, without relying on colors or plugins). sim may assume your own machine: a modern terminal with truecolor and italics, a local file system, git, and a mouse.

## Principles

Lower numbers take priority. When two principles conflict, the lower number wins.

| ID | Principle | Meaning |
|---|---|---|
| P1 | Safety | Everything si guarantees (its P1 and every SAFE requirement) holds for every file open in sim |
| P2 | Reach | The fewest steps from "I want that file" to reading it, and from "what changed?" to the change |
| P3 | Signal | The change color stays the only saturated hue on screen. Everything else uses lightness and type |
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
| 7 | Syntax highlighting showed no measurable effect on comprehension (Hannebauer et al. 2018, EMSE, n = 390; Beelders & du Plessis 2016, JEMR) and only a small speed gain for novices that fades with experience (Sarkar 2015, PPIG, n = 10) | Highlighting is a convenience, not a claim: typographic only, on by default, one key turns it off |
| 8 | A target with one unique feature pops out; the more similar and varied the distractors, the slower the search (Treisman & Gelade 1980; Duncan & Humphreys 1989) | No syntax hues. Comments are dimmed, keywords bold, strings italic; the change color stays unique (P3) |

No study compares fuzzy finders with trees, or shows that marking changed files helps. Those choices are hypotheses, like si's readability claim, and can be measured the same way.

## What sim adds to si

A tree panel of the project, typing to light up matches in it, changed and likely files marked in it, several files open at once (each one a complete si), typographic syntax highlighting, and the mouse.

## What we don't build

LSP, completion, plugins, config files, split windows (tmux does that), a terminal inside the editor, anything that only the mouse can do.

Features we find ourselves wanting are only written down in NOTES.md.
