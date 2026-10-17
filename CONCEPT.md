# CONCEPT — sim

> 隠れたものまで見える、セキュアな Rust 製 Vim
> The next-generation Vim that shows even what is hidden.

sim is the next-generation Vim (新時代の Vim): it shows even what is hidden, it is secure, and it is written in Rust.

Files that AI and other processes rewrite: a human reads them as fast as possible, fixes them, and saves them without losing anything. And a place to live in: a project's files a few keys away, several of them open at once, and help with reading code that never hides what changed. It is meant to be `$EDITOR` too (git commit, Claude Code's prompt editing, quick fixes), over ssh and as root, where it runs nothing of its own.

The name: sim is *Secure vIM*. It began as si, *Secure vI*, a minimal editor of its own; si is only the core of sim now, an internal part and no longer a program.

## Two parts, one program

**The core** (`sim-editor-core`, in `core/`) is what keeps text from being lost: the buffer and vi's grammar, the atomic save, snapshots, the merge with what other processes write, the terminal. It has its own principles, its own cap on lines and its own list of the crates it may use. **The editor** (`sim-editor`, in `sim/`) is the program, the binary `sim`: it runs the core's editor once for each open file and adds the tree, finding, search, marks, syntax, the looks and the mouse.

`sim --plain` is the core alone: the calm look, one file, no panel, and no program of its own run (no git, no ripgrep, no language server, no filter, no hook); syntax colors stay. It is what sim is when it is root and when it is the editor of `sudoedit`, because there a file's owner is more privileged than the person at the keys. Every promise of the core holds in sim as a whole; plain mode is where they are the only ones.

## The core's principles

Lower numbers take priority. When two principles conflict, the lower number wins.

| ID | Principle | Meaning |
|---|---|---|
| P1 | Safety | Without an explicit action, nobody's text is lost (yours or another process's) |
| P2 | Understanding | Make what changed obvious at a glance. Color is used only for change |
| P3 | Speed | 5 ms to start, 1 ms per key. Anything that would make you wait runs asynchronously |
| P4 | Minimal | core/src stays within 3,200 lines. Every feature is judged by "can we cut it?" |
| P5 | Keys | The best key for each job, not vi for its own sake: keep the vi grammar the hands already know, and depart from it wherever vi is worse than modern editors. |

### External changes

- **A change on disk is a fact.** The parts that don't conflict with your edits are merged automatically and marked where they changed, word by word, until you have looked at them
- **A conflict is never merged away.** Your text stays, marked where the disk differs, and the file on disk is not written over: both versions are kept until you choose. `:w!` keeps yours and `:e!` takes the disk's, and each copies the version it gives up into the state directory first, so the choice itself loses nothing. Once the choice is made the marks go: mine is the file, or the disk's is the buffer, and the screen no longer says there is a difference

### Why it looks the way it does

| # | Finding | How it shapes the design |
|---|---|---|
| 1 | Syntax highlighting does not improve comprehension (Hannebauer et al. 2018, 390 students; Beelders & du Plessis 2016, eye tracking) | The core has no syntax highlighting (the editor's is a convenience, and off with one key) |
| 2 | An item that differs by a single color is found instantly, regardless of how many items there are (Treisman & Gelade 1980) | Exactly one chromatic color, reserved for "change" |
| 3 | Without a cue, people fail to notice changes on screen (change blindness; Rensink et al. 1997) | Marks for external changes stay until you have looked at them |
| 4 | About 5% of Japanese men have a color vision deficiency (Japan Ophthalmologists Association) | Besides color, gutter symbols and strikethrough also show change; with no color at all (`NO_COLOR`) every one of these is still told apart (COLOR-2, COLOR-3) |
| 5 | Legibility is determined by perceived contrast (APCA) | Body text at Lc 90 and secondary text at Lc 60, computed from the terminal's background |
| 6 | Lines of 45–75 characters are the most readable for Latin text (Bringhurst) | Prose wraps at 72 columns |
| 7 | Indentation of 2–4 columns gives the best comprehension (Miara et al. 1983) | Tabs are displayed as 4 columns |
| 8 | Light backgrounds favor proofreading accuracy, especially with small text (Piepenbrock et al. 2014) | The background follows the terminal, and both light and dark meet the targets |

"More readable than existing editors" is a hypothesis, not a claim. The M6 perception benchmark (time and accuracy to find a change) tests it.

## The editor's principles

Lower numbers take priority. When two principles conflict, the lower number wins. They rank below the core's: the editor adds, and takes nothing away.

| ID | Principle | Meaning |
|---|---|---|
| P1 | Safety | Everything the core guarantees (its P1 and every SAFE requirement) holds for every file open in sim |
| P2 | Reach | The fewest steps from "I want that file" to reading it, and from "what changed?" to the change |
| P3 | Signal | The change color stays the easiest thing to find: it is the only chromatic background in the text, and no text color shares its hue. Everything else may use color |
| P4 | Speed | The core's targets hold: 1 ms per key. Anything slow (walking a tree, asking git, parsing for highlighting) runs asynchronously |
| P5 | Keys | The core's keys, plus the keys the hands know from nvim and VS Code for what the core doesn't have |

In SPEC the tag `[P2]` of an item in part one is the core's principle, in part two the editor's.

### Why it works the way it does

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

No study compares fuzzy finders with trees, or shows that marking changed files helps. Those choices are hypotheses, like the core's readability claim, and can be measured the same way. The search of contents (Ctrl-f) is the author's habit from nvim (Telescope's live_grep), not a finding: it is the way in for the 4–15% of retrievals that search (finding 1), and it keeps the tree's promise that nothing moves: the results are in a window that floats over the screen with a preview of the selected file (the picker, PICK-1), the panel and the files behind it are not touched, and Esc closes it and everything is as it was.

## What the editor adds to the core

A tree panel of the project, typing to light up matches in it, finding files and searching their contents in a picker that floats over the screen with a preview, changed and likely files marked in it, several files open at once (each one a complete core editor), syntax highlighting in color, the mouse, and three looks.

The looks (`zl` goes round them, and the choice is remembered) differ in how much of the screen is drawn to help the eye find the change. **Calm** is the core's own: four styles, one chromatic background (blue, hue 250). **Flat** shows a diff the way GitHub, VS Code and delta do: what comes is on green and what goes on red, and every row of a change has a faint tint behind it (green for a line that gained or changed text, red for a deletion point), so that a change is a band across the screen and not only a patch of words. **Depth**, the default, adds btop-like boxes around the tree and the text, an overview ruler on the text's right border, and a gradient: the text fades into the page by its distance from the cursor, the tree by its distance from the open file, and whatever is no change fades toward the page. The owner chose red and green here and set color vision aside for these two looks, so P3's single chromatic hue holds in calm only; calm is the look for anyone who cannot tell red from green. Without a palette or truecolor flat is calm and depth is calm in its boxes. Like the rest of the editor's choices of look, these are hypotheses: no study compares them with calm, and the M6-style benchmark (time and accuracy to find a change) would.

Beyond the looks, the interface's color has one meaning for each hue (COLOR-1): grays and weight are structure; green, amber and red are changes and nothing else, so that a change is the one thing that pops out among plain distractors (Treisman & Gelade 1980; Duncan & Humphreys 1989, finding 8); blue is where you act (the mode, the shown tab, a match, a link); cyan is what a heading is; magenta is only what is hidden or dangerous, a char shown in another form included, and nothing else in the text may be magenta (numbers and constants are the strings' green). Syntax keeps its hues inside the text. This is the owner's rule, made after the first interface used amber for a modified file, for the number of the cursor's line and for a heading, side by side, and completed on 2026-10-17 when a heading looked like bold text: what a thing is is told by hue, how much or what rank by lightness and weight (Mackinlay 1986: hue is a nominal channel, lightness an ordinal one), so a heading's level is weight and band, a task's being done is a gray, and the fog of distance has a floor of |Lc| 60 for text.

Color is a help and never the only carrier of a meaning (COLOR-2, COLOR-3). With `NO_COLOR` set, or on a terminal that cannot do 8 colors, sim writes no color at all and does not ask the terminal for any; weight, underline and inverse video say what the colors said. A changed line, an added one and a deleted one are three shapes in the sign column, a folder says the strongest git status below it in a letter, a tab says "changed on disk" with `↯`. Icons are the same kind of help (ICON-1): they need a Nerd Font, which a terminal does not announce, so they start on only where the terminal is known to bring the symbols (Ghostty, kitty, WezTerm) and off, with a hint, elsewhere; with them off, no private-use code point is drawn.

## What we don't build

LSP, completion, plugins, config files, split windows (tmux does that), a terminal inside the editor, a GUI (Ghostty draws), macros, networking, anything that only the mouse can do. The core builds none of the editor's additions (a file tree, several buffers, finding, marks, the looks beyond calm); they are in the editor, and `sim --plain` has none. Syntax colors are in-process and stay in it (`zh` turns them off); what plain mode takes away is every program.

Features we find ourselves wanting are only written down in NOTES.md.

## How big

The core (`core/src`) is limited to 3,200 lines and the editor (`sim/src`) to 7,980; together they are about 11,150, about 2.9% of Neovim's C (about 380,000 lines by GitHub's count). The limits are a pressure to keep asking "can we cut it?", not a race, and the author's measure is that the editor stays significantly smaller than Neovim. They move only with a reason stated here.

**The core.** The limit was 3,000 until M3. It was raised to 3,300 because what pushed past it was safety (P1: races between saves, reloads and the watcher, a private state directory, compare-and-swap saves) and correctness (P2: widths of CJK and emoji, lines taller than the screen), both of which rank above P4. After M4 it was raised to 3,600 by the author's own measure, "significantly smaller than Neovim": filters, reopening where you left off and incremental search came from the author's requests and nvim habits. On 2026-10-17 it moved by three lines, to 3,603, with the owner's agreement that limits move for a stated reason and are not to be overdone: a char that is shown in another form (SAFE-8) has a style of its own, `Style::Hidden`, so that the editor can draw it in the alert's color and a file that holds the literal text `<U+202E>` no longer looks like the real control char (P1). The same day the owner took the AI-only features out (the proposals, REVIEW mode, `:ref`, `contrib/ai`): a conflict with the disk is now kept as it is, with both versions recoverable, and the limit came down to what was left, 3,425 lines (SAFE-7). 178 lines went: the proposals, their word diffs and their weave into the view, REVIEW's keys and status, and `:ref` freed about 330; the copies of SAFE-7 (`Effect::Keep`, `Event::Kept`, the journal's `keep`, the reload that waits for its copy) and a filter's result that is merged in place took about 150. Then the owner made sim one program, and the core became a library: its program (`src/main.rs`, 267 lines: the event loop, the signals, the threads) went, because the editor's event loop does the same for many files, and the limit came down to 3,200 for what is left, 3,191 lines. They took 33: plain mode (PLAIN-1: the switch that no program is run, the filter's refusal, the hook's guard) and the copy of the old state directory (STATE-1), less the 3 that reading our uid from the system (`geteuid`) saved in `file.rs`. The core's dependencies are an allowlist too (ropey, crossterm, notify, rustix, similar, unicode-width, regex-lite; `scripts/check.sh`, `tests/safe_9.rs`), and the editor's are the editor's own.

**The editor.** It was limited to 3,000 lines and is limited to 7,980. The search of contents (GREP-1 to GREP-8) took about 750 of them: a literal smart-case reader of files, a thread that runs ripgrep or reads the files itself and sends what it finds in batches that a newer query cancels, the results (first in the panel with a preview in the editor pane, since PICK-1 in a picker of their own), and the checks that keep a repository from making that search run its code (ripgrep's config and `--pre`) or lead it out of the tree by a link. That is what the author asked for by name, a key he uses all day in nvim, and the safety checks (P1) are part of the cost, not a thing to cut. The three looks (LOOK-1 to LOOK-9) took about 660 more: two hues found the way the core finds its change color, the tints and the fade in OKLab, the boxes with their titles, badges and keys, the overview ruler, the raised card, the gradients of the text and of the tree, and the Painter's side of all of it. The author asked for each of them, they serve P3 (the change is the easiest thing to find) and P2 (what changed, at a glance), which rank above the size. The interface (UI-1 to UI-13 and COLOR-1) raised the limit from 4,200 to 6,400 and took about 2,000 lines of the 6,162 that sim/src had then. The picker (PICK-1 to PICK-5: the window, its keys, the list, the preview with the highlighter's colors, the count and the spinner; Ctrl-p's finder and Ctrl-f's search of contents are its two modes) and the lit matches of a search (UI-14) raised it from 6,400 to 7,000: the author found the results in the tree panel poor next to Telescope, and asked for the same window. The author put sim next to his own Neovim (Neo-tree, BufferLine, lualine, gitsigns, indent-blankline, todo-comments, render-markdown) and found the drop far too big, and wanted at least that environment surpassed: icons and git status in the tree, tabs, signs from a thread that asks git, guides and listchars, TODO badges, markdown from the highlighter's scopes, a status line with a slot for the language server. They serve P2 (what changed, at a glance) and the owner's measure, which rank above the size.

The fold into one program moved the editor's limit from 7,000 to 7,050. The editor ended at 7,019 lines, 188 more than the 6,831 it had, and the lines are: in `io.rs` 98 (the detection of `NO_COLOR` and of terminfo's `colors`, COLOR-2, 40; the icons' defaults and their hint, ICON-1, 38; the detection of root and `sudoedit` and the hook's two names, PLAIN-1, 11; the guards of git and ripgrep and the unnamed buffer, 9), in `paint.rs` 24 (the Painter without color and the cues, COLOR-2), in `sim.rs` 28 and `main.rs` 19 (plain mode, the page the interface is laid out on without color, the icons' hint, and the terminal asked for its colors only when the startup keys have not quit), 19 in `color.rs`, `layout.rs`, `pick.rs` and `bar.rs` (the cues and `Tones::mono`). The two parts together went down by 46 lines (10,256 before, 10,210 now), so the owner's measure, that together they stay small, was not touched.

Keys and help (KEY-1 to KEY-6, CLI-7, UI-16, UI-17) moved the limit from 7,050 to 7,600. The editor ended at 7,585 lines, 566 more than the 7,019 it had, and the lines are: `keys.rs` 178 (reading the key table, the keys written for people, the keys that can follow a prefix, the help's rows, and which-key's small window), `sim/guide.rs` 122 (the leader, the prefix that is typed, the pause, the letters on the tabs and the pick and close by letter, and the hint for Tab), 128 in `sim/live.rs` (the help in the picker: rows, filter, the preview with the long text wrapped, and that Enter runs nothing), 93 in `sim.rs` (the new events and effects, the window in the frame, the search count), 20 in `main.rs` (`--version` and `--help`, the timer of the pause, the count of uses of Tab), 17 in `tabs.rs` (the letters on the tabs), 11 in `io.rs` (the count of uses, kept like the icons' hint), and 6 in all in `color.rs`, `pick.rs` and `lib.rs` (F1, the enter key's word, the module), less the 9 that `layout.rs` lost with the numbers in the titles. The table itself (`keys.tsv`, 144 lines of data) is not counted, as the usage text (`help.txt`) is not: they are what the lines read, and the test that every key of Normal mode is in the table and every entry of it is handled keeps them true. They serve P2 (what is typed is shown, what a key does can be found without leaving the editor) and the owner's measure was kept: the two parts together are 10,779 lines, 2.8% of Neovim's C.

The git guard and the changes view (GIT-1 to GIT-4, DIFF-1 to DIFF-6) moved the limit from 7,600 to 7,980. The editor ended at 7,956 lines, 364 more than the 7,592 it had, and the lines are: in `io.rs` 128 (the one function that builds a git command line, with what it turns off, the filters by name, the exception for git-lfs with its list of harmless settings, and the private empty directory for hooks, 94 net of the two functions it replaced; and what the changes view asks of git, the numbers and HEAD's version with the two threads that do it, 34), `changes.rs` 70 (the numbers of each file and the sums of the folders, read from `git diff --numstat -z`), `sim.rs` 62 (the view's state, its two events and two effects, the rename's row, the buffer of HEAD's version), `layout.rs` 42 (the numbers at the row's edge in the interface and after the name in calm, 22; and the cut of a name with a `…`, which keeps a renamed file's new name whole, 20), `panel.rs` 25 (the key, the narrowed tree made of the files that differ, Enter on a deleted file), `marks.rs` 21 (the renames, the untracked files, the files that differ), `tabs.rs` 7 (the `HEAD:` of its tab), 9 in `main.rs`, `guide.rs` and `lib.rs`. The owner opens Neo-tree's `git_status` with `<leader>ge` in his Neovim, and deleted and renamed files never show in a normal tree: they are hidden changes, which is what sim is for (P3, P2), and the guard is P1, which ranks above the size. The two parts together are 11,150 lines, 2.9% of Neovim's C.
