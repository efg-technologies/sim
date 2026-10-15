# SPEC — si

Format: `- **ID** [principle][milestone] requirement — verification`

Kinds of verification: U = unit/property test, I = integration test (feed an event sequence to App), E = terminal E2E (tmux), B = benchmark.
Test names start with the ID in lowercase (SAFE-1 → `safe_1_...`). scripts/trace.sh checks the mapping.

Sub-items that start with **Decided** record what was settled while implementing, with the principle that settled it.
They bind the implementation the same way as the line above them.

## Startup and exit

- **CLI-1** [P5][M1] `si [+cmd] [file]`. `+N` goes to line N, `+/re` searches, and any other `+cmd` runs as an ex command. It runs after the first render — I
  - **Decided** (P5): `--` ends the options, so a file named `+x` can be opened. Any other argument that starts with `-`, or a second file, prints usage and exits with 2. Without a file, si opens an unnamed buffer, and `:w {file}` gives it that name.
- **CLI-2** [P3][M1] `file:line[:col]` is accepted. But if a file whose name is exactly that string exists, that file is opened — U
- **CLI-3** [P5][M2] The exit code is 0 for `:q` `:wq` `:x`, and 1 for `:cq` (used to abort git commit) — I
  - **Decided** (P5): `:q!`, `ZZ`, `ZQ` and `q` exit with 0 as well, so `:cq` is the only way to fail on purpose. `git commit` with `GIT_EDITOR=si` is checked end to end: `ZZ` commits and `:cq` aborts.
- **CLI-4** [P1][M1] Every way out, including panic, SIGTERM and SIGHUP, restores the terminal state — E
  - **Decided** (P1): SIGTERM and SIGHUP reach App as an event. Unsaved changes are snapshotted (SAFE-6) before exiting with 128 + the signal number.
  - **Decided** (P1): a failed write to the terminal counts as a hangup: it reaches App as SIGHUP, so the unsaved text is snapshotted and si exits with 129, instead of leaving at once with 1.

## Safety

- **SAFE-1** [P1][M1] Saving creates a temporary file in the same directory with 0600 and O_EXCL → writes it → fsync → rename → fsync of the directory, in this order. Whichever step fails, either the old or the new version remains intact — U (fault injection at each step)
  - **Decided** (P1): before the rename, the temporary file gets the original's mode and group. If the group cannot be set, the in-place path of SAFE-2 is used instead.
  - **Decided** (P1): a symlink is saved through to its target, and the link itself stays.
  - **Decided** (P1, P5): a file that does not exist yet gets the mode any new file would get, so its temporary file is created with 0666 reduced by the umask instead of 0600. This exposes nothing the final file doesn't.
  - **Decided** (P3): saving runs on a background thread, and editing continues meanwhile. `:wq` and `:x` quit only after the save has succeeded.
  - **Decided** (P1): once the rename has happened the save counts as done, even if syncing the directory then fails (some systems refuse it); the status line says so as a warning.
- **SAFE-2** [P1][M1] If the file is owned by someone else, has two or more hard links, or its directory is not writable, save in place. Before that, write a recovery copy and fsync it; delete it once the save has succeeded — U
  - **Decided** (P1): the recovery copy goes to `$XDG_STATE_HOME/si/` (0700), because the file's own directory may not be writable. If the in-place write fails, the status line names the recovery copy.
- **SAFE-3** [P1][M1] Opening and saving without editing gives byte-identical output (line endings, whether there is a final newline, BOM, permissions) — U (property test)
  - **Decided** (P1): if every line break is CRLF, the buffer holds LF and saving writes CRLF. Any other mix is kept exactly as it is (`\r` shows as `^M`, SAFE-8).
  - **Decided** (P1, P2): an empty or new file gets a final newline once it has text, since a text file ends with one (git and POSIX tools expect it). An existing file keeps whatever it had.
- **SAFE-4** [P1][M1] A file that is not valid UTF-8, or has a NUL in its first 8 KB, opens read-only and shows why — U
  - **Decided** (P1): invalid bytes are shown as U+FFFD. Edits and every kind of write are refused, because the text on screen is not the file. A file that cannot be read (no permission, a directory) opens the same way with an empty buffer.
- **SAFE-5** [P1][M3] Right before saving, the hash of the content on disk is compared with the hash at load (the base). If they differ, nothing is overwritten and the disk content goes to the merge of PROP-2 — I
  - **Decided** (P1): the check runs on the save thread right before the rename (compare-and-swap). After the merge, nothing is saved automatically; the status line says the file was not saved, and `:w` saves.
- **SAFE-6** [P1][M2] Unsaved changes are snapshotted at most once per second into `$XDG_STATE_HOME/si/` (0700). Opening the same file after an abnormal exit offers to restore them. At most one second of work is lost — I
  - **Decided** (P1): a snapshot holds the path, the base text and the buffer text, in a file (0600) whose name includes the process id, so two sessions on one file don't overwrite each other's snapshots. A background thread writes it (P3). It is deleted after a successful save and on quitting with `:q`, `:q!` or `:cq`; it stays after a crash or a signal. A snapshot whose process is still running is left alone.
  - **Decided** (P5): if the disk still matches the snapshot's base, restoring applies the snapshot text as one undo step, and the status line says how old they are, e.g. `recovered unsaved changes from 3 minutes ago · u to undo` (an age needs no time zone, which si cannot read without libc). If the disk has changed since, the snapshot goes through the merge of PROP-2 (from M3; before M3 it is kept, not applied, and the status line names its file).
  - **Decided** (P1): the shell sends a tick at least every second, from a timer thread, so continuous typing is snapshotted too. On a tick, if the buffer is modified, its version has changed since the last snapshot and 1000 ms of tick time have passed since the last one, the App asks for a snapshot (`Effect::Snapshot`, holding the path and ropes, which are O(1) clones; the journal thread turns them into bytes). Tick times are 1000 ms apart by construction, so at most one second of work is lost. The core never reads a clock; the journal thread stamps the snapshot's time, and the shell passes the age of a recovered one in ms.
  - **Decided** (P1): the App asks the journal to forget (`Effect::Unsnapshot`) whenever the buffer becomes clean (saved, or undone to the saved text, or `:e!`) and before every `Quit` that a key or a save causes: `:q`, `:q!`, `:cq`, `ZQ`, `q`, and a successful `:wq` `:x` `ZZ`. A signal asks for a snapshot of the unsaved text, whatever the clock says, and keeps it.
  - **Decided** (P1): an unnamed buffer is protected too. Its journal key is the empty path, so `si` without a file offers the unsaved text of a crashed unnamed session. The key of a file is its canonical path, or the absolute path for a file that does not exist yet (symlinks are followed, as in SAFE-1). The App sends the path with every snapshot, because only it knows whether `:w {file}` has named the buffer or a `:e` was refused.
  - **Decided** (P3): one journal thread runs snapshots and removals in order, so a key never waits for a disk. Before si exits (`Quit`, signal, SIGHUP) the shell closes its queue and waits for that thread for at most 2 seconds, so a SIGTERM right after an edit still leaves the last snapshot.
  - **Decided** (P1): a recovered snapshot belongs to a dead process. It is deleted only once the buffer's own snapshot (which then holds the same text or newer) is on disk, or when the buffer is clean again or we quit. A snapshot that was not applied is never deleted by si.
  - **Decided** (P1): recovery runs at startup before a `+cmd`, so that command works on the recovered text (and, as with any key, clears the status message). It is not applied to a read-only buffer.
  - **Decided** (P1): a snapshot whose process is gone but whose base differs from the disk is kept and left alone. Until M3 merges it, every open of that file says `kept unsaved changes from {age}, not applied: {file}`, with the snapshot's own file name.
- **SAFE-7** [P1][M3] While an external conflict is unresolved, `:w` is refused (`:w!` saves my version). While a proposal is unresolved, `:q` is refused (`:q!` discards) — I
  - **Decided** (P1): `:w!` drops the unresolved conflicts from disk, since the disk then holds my version. Proposals from filters stay.
- **SAFE-8** [P1][M1] Control characters (C0, C1, DEL) are shown in `^[` form. Bidirectional controls (U+200E–200F, U+202A–202E, U+2066–2069), U+200B, and U+FEFF anywhere but the start are shown in `<U+202E>` form. None of them is ever sent raw to the terminal (against Trojan Source) — U
  - **Decided** (P2): caret notation has no form for C1, so C1 (U+0080–U+009F) uses the `<U+0085>` form. A tab is shown as spaces (VIS-8).
- **SAFE-9** [P1][M1] Settings and commands are never read from file contents (no modelines). No network access. Tests check that src/ has no `std::net` and that the dependencies match the allowlist — U
  - **Decided** (P1): `signal-hook` is on the allowlist. CLI-4 and SAFE-6 need SIGTERM and SIGHUP without `unsafe`, and crossterm already depends on it, so the binary doesn't grow.

## External changes and Proposals

- **PROP-1** [P1,P2][M3] Watch the parent directory of the open file and take in changes on disk within 100 ms (including saves by rename, deletion and re-creation). Events caused by my own saves are recognized by their hash and ignored — I
  - **Decided** (P1): when the file is deleted, the buffer keeps its text and counts as modified, and the status line says `deleted on disk`.
- **PROP-2** [P1,P2][M3] If the buffer is clean, reload it and mark what changed. If it is dirty, run a 3-way merge of base, mine and disk: take in the changes that don't conflict and mark them, and turn the conflicting parts into Proposals. The cursor keeps pointing at the same text — I
  - **Decided** (P1): a reload or a merge is one undo step. Afterwards, the disk content is the new base.
- **PROP-3** [P1][M3] The 3-way merge is our own, built from two `similar` line diffs. Conflict-marker strings are neither generated nor parsed — U
- **PROP-4** [P1][M3] Property: rejecting every hunk gives my version, and accepting every hunk brings in every change from the disk side. No character from either side is lost — U (property test, 10,000 cases)
  - **Decided** (P1): precisely, for a merge of base, mine and theirs: rejecting every hunk gives mine plus the theirs changes that did not conflict (just mine when every theirs change is a proposal, as with filters); accepting every hunk gives the same with theirs winning every conflict (just theirs when mine equals base); until a hunk is resolved, every line of mine and of theirs is either in the buffer or in that hunk.
- **PROP-5** [P3][M4] Filters (`:{range}!cmd`, `!` in Visual mode) run asynchronously with `sh -c`. You can keep working while one runs, and the status line shows the elapsed seconds. Ctrl-c cancels it — I
  - **Decided** (P4): one filter at a time. Ctrl-c stops the filter's whole process group.
- **PROP-6** [P1,P2][M4] When the filter ends, run a 3-way merge with the buffer at its start as the base, and turn every change from its output into a Proposal (the edits I made meanwhile are kept). With a nonzero exit code, the result is discarded and stderr is shown — I
  - **Decided** (P2): the status line shows the first line of stderr.
- **PROP-7** [P3][M3] Diffing gives up after 50 ms (the deadline of similar). Even on a 100,000-line file, taking in a change finishes within 50 ms — B
- **PROP-8** [P2,P5][M3] `]c` / `[c` move to the next/previous mark or Proposal. On a Proposal, REVIEW mode starts, and the status line keeps showing `REVIEW 2/5  y accept · n reject · Y all · N none · Esc back`. `y` accepts and `n` rejects, and both move on to the next hunk. `Y` accepts all, `N` rejects all, `Esc` goes back. Each accept is one undo unit — I
  - **Decided** (P1): REVIEW starts only when `]c` or `[c` lands on a Proposal, so `y` and `n` never change meaning just because the cursor moved onto a hunk. Inside REVIEW, `]c` and `[c` move on without deciding. Any other key leaves REVIEW and works as in Normal mode. REVIEW ends when no proposal is left.
  - **Decided** (P1): a reject is one undo unit too, so undo brings a rejected hunk back. `Y` and `N` are one undo unit each.
- **PROP-9** [P2][M3] A mark stays until the cursor enters its line. `:seen` clears all of them — I

## Look

- **VIS-1** [P2][M1] No syntax highlighting. Text uses only two achromatic levels: primary (|Lc| 90) and secondary (|Lc| 60: line numbers, and unchanged lines during REVIEW) — U
- **VIS-2** [P2][M3] The single chromatic color, "change" (OKLCH h=250), is used only as a background. Changes are painted word by word (a run of letters, digits and `_` is one word; so is each CJK character, and each symbol) — U
- **VIS-3** [P2][M3] Changes are shown without color as well: the gutter shows `+` (added), `~` (changed) and `-` (where something was deleted). During REVIEW, words that would be deleted are struck through in the secondary color — U (golden)
  - **Decided** (P2): outside REVIEW, a Proposal shows its gutter symbols and puts the change background on the words it would replace. In REVIEW, every hunk is shown inline: inserted words on the change background, deleted words struck through.
- **VIS-4** [P2][M5] Colors are computed with APCA-W3 0.0.98G-4g from the terminal's background, obtained with OSC 11 (waiting at most 3 ms). Primary text at |Lc| 90, secondary at |Lc| 60, the change background against the normal background at |Lc| ≥ 15, and text on the change background at |Lc| ≥ 75. With no reply, reverse (SGR 7) and faint (SGR 2) stand in; if the reply arrives late, redraw. Checked on every background of `#000000 #1e1e1e #282c34 #002b36 #ffffff #fafafa #fdf6e3 #eff1f5` — U
  - **Decided** (P2): primary and secondary are grays within ±1 of their target Lc. The Lc between the two backgrounds is computed with the change background as the text color.
  - **Decided** (P2): the three numbers cannot hold with one primary gray: 15 + 75 already uses up the 90 between primary and the background once APCA's offset is paid twice (measured: at most 68–72 on the eight backgrounds). So text on the change background gets its own gray, the one closest to primary that keeps |Lc| ≥ 75 there. It stays achromatic (VIS-1): the primary level adjusted to its background, not a new level of meaning.
- **VIS-5** [P2][M5] Even after the color vision deficiency simulation of Machado et al. (2009) for protan, deutan and tritan (severity 1.0), the change background still differs from the normal background by |Lc| ≥ 15 — U
- **VIS-6** [P2][M1] `.md`, `.txt`, `COMMIT_EDITMSG` and files without an extension wrap at 72 columns. Other files wrap at the screen width, and continuation rows show no line number — U
  - **Decided** (P2): 72 columns of text, not counting the gutter; a narrower screen wraps at its own width. A row breaks after a space when it can (and between CJK characters); a word longer than a row is cut at the edge.
- **VIS-7** [P2][M5] Wrapping applies kinsoku: `、。，．）」』】〕〉》ー…‥・！？` never start a row, and `（「『【〔〈《` never end one. A full-width character counts as 2 columns (unicode-width) — U (property test)
  - **Decided** (P2): when a row cannot satisfy kinsoku (a row of nothing but `、`), it breaks at the edge anyway.
- **VIS-8** [P2][M1] A tab is displayed as 4 columns — U
  - **Decided** (P2): tab stops are every 4 columns, so text aligned with tabs stays aligned.
- **VIS-9** [P2][M1] State is shown in words. The status line has the form `path  line:col  MODE  modified · 3 changes on disk · 2 proposals` — U (golden)
  - **Decided** (P2): MODE is NORMAL, INSERT, VISUAL, V-LINE or REVIEW, and col counts characters from 1. Parts that don't apply are left out, and a count of one is singular. Other states (read-only with its reason, `deleted on disk`, a running filter) are added the same way. A message (an error, `search wrapped`) replaces the status line until the next key. While typing after `:`, `/` or `?`, the status line is the command line. An unnamed buffer shows `[no name]`.
- **VIS-10** [P2][M1] The cursor shape also shows the mode (Normal: block, Insert: bar, REVIEW: underline; none of them blink). Checked through the DECSCUSR output — U
  - **Decided** (P5): Visual mode uses the block, and the command line uses the bar.
- **VIS-11** [P4][M1] No decoration: no icons, no box drawing, no `~` past the end of the file, no highlighting of the cursor line — U (golden)
- **VIS-12** [P2][M1] The eyes find their place again: `n N * # ]c [c Ctrl-d Ctrl-u`, `:N`, `+N` and `file:line` put the cursor line in the middle of the screen, and moving the cursor keeps at least 8 lines of context above and below it (half the text area on a short screen) — U
  - **Decided** (P2): centering applies near the end of the file too; rows past the last line stay blank.

## Keys

The vi grammar the hands already know, departing from vi wherever vi is worse than modern editors (P5).

- **VI-1** [P5][M1] Motions: `h j k l w b e W B E 0 ^ $ gg G f F t T ; , % Ctrl-d Ctrl-u zz`. A count can be given — U
  - **Decided** (P4): `%`, `n`/`N` and text objects ignore counts; `%` matches `()[]{}` only. `n N * #` are motions, so `dn` works, but an operator followed by `/` is cancelled (no `d/re`).
- **VI-2** [P5][M1] Editing: `x r J p P u Ctrl-r . i a I A o O dd cc yy D C Y >> <<` — U
  - **Decided** (P5): `Y` yanks to the end of the line, consistent with `D` and `C` (Neovim's default; vi's `Y` duplicates `yy`). `U` redoes, like Ctrl-r (vi's line-undo `U` is dropped). `>>` and `<<` shift by the indent unit of VI-6.
  - **Decided** (P5): `.` replays the keys of the last Normal-mode change, a new count replacing the old one; Visual-mode operations are not repeated, and a replay stops at the first key that fails. A paste inside an insert session belongs to that session's undo unit. After undo the cursor goes back to where it was before the command.
- **VI-3** [P5][M1] The operators `d c y > < !` combine with a motion or a text object (`iw aw iW aW i" a" i' a' i( a( i[ a[ i{ a{ i< a<`) — U
  - **Decided** (P5): `!{motion}` opens the command line with `:{range}!`, as in vi.
- **VI-4** [P5][M1] Search: `/ ? n N * #`. Regular expressions use the syntax of regex-lite — U
  - **Decided** (P2, P5): a search wraps around the end of the file with the message `search wrapped`. It is smartcase: a pattern without an uppercase letter ignores case, one with an uppercase letter doesn't. `*` and `#` search for the whole word under the cursor, case-sensitively. Matches are not highlighted, since color is only for change.
  - **Decided** (P4): `*` and `#` put `\b` only at ASCII word edges (regex-lite's `\b` is ASCII-only). Matches never overlap.
- **VI-5** [P5][M1] Visual mode: `v V o`. It takes the same operators as VI-3 — U
  - **Decided** (P2): the selection is shown with an underline (SGR 4), because color is reserved for change and reverse video is the fallback for change (VIS-4). `:` in Visual mode starts the command line with `'<,'>`.
  - **Decided** (P4): Visual mode takes motions, text objects, `o`, the operators and `:`; other Normal-mode commands (`p`, `J`, `u`, `.`, `/`) do nothing there. A text object widens a selection longer than one char.
- **VI-6** [P5][M1] Insert mode: typing, BS, Enter (keeps the indent of the line above), Tab (inferred from the file: a tab if the first indented line uses a tab, that many spaces if it uses spaces, otherwise 4 spaces), Ctrl-w, Ctrl-u, Esc — U
  - **Decided** (P5): BS at the start of a line joins it to the line above.
  - **Decided** (P5): Ctrl-[ and Ctrl-c leave Insert mode like Esc, since the hands know both. Moving with the arrows inside one insert session keeps it one undo unit.
- **VI-7** [P5][M1] ex commands: `:w [file]` `:w!` `:q` `:q!` `:wq` `:x` `:cq` `:e {file}` `:e!` `:{N}` `:{range}s/re/rep/[g]` `:{range}!cmd` `:seen` `:ref`. Ranges are `N` `N,M` `%` `.` `$` `'<,'>` — U
  - **Decided** (P5): `:q` and `:e {file}` are refused while the buffer is modified (`:q!` and `:e!` discard). In the replacement of `:s`, `&` and `\1`–`\9` refer to the match, as in vi. `:x` writes only when the buffer is modified.
  - **Decided** (P1): a file arrives as an event, so the buffer may be edited between `:e` and the arrival. The App records the buffer's version when the command runs, and if it differs when the file arrives (an edit that was undone again gives the same version), it refuses the load with `not opened: edited meanwhile` instead of discarding those edits.
  - **Decided** (P4, P5): ranges have no `+N` offsets; `N,M` with M < N is swapped, numbers clamp to the file, and a bare range goes to its last line. `:e` without a file reloads (refused while modified); `:wq!` and `:x!` force the write. `:s` takes only the `g` flag, and `:s//rep/` reuses the last search pattern.
- **VI-8** [P5][M1] There is only one register, with no register names. Every yank also sends the text to the system clipboard through OSC 52; a delete or a change fills only the register — U
  - Changed from "a yank prefixed with `"+`": copying out to another program is the common case, and a prefix to remember costs keys without adding safety (P5). Deletes stay out of the clipboard so that `x` and `dd` never clobber what you copied.
  - **Decided** (P2): inside tmux (`$TMUX` set) the sequence is also sent wrapped in tmux's passthrough, because tmux's default `set-clipboard external` drops it.
- **VI-9** [P3][M1] The kitty keyboard protocol is enabled without waiting for a query. Esc is handled without delay. Bracketed paste is enabled, and one paste is one undo unit — U
  - **Decided** (P1): a paste is always inserted at the cursor as text and never run as keys, whatever the mode (Visual and REVIEW end first). On the command line, line breaks in a paste become spaces.
- **VI-10** [P2,P5][M4] In Normal mode, full-width ASCII (U+FF01–FF5E) is read as half-width (so you can keep operating with the IME on) — U
  - **Decided** (P5): this applies to command keys in Normal, Visual and REVIEW mode and after an operator, but not to the character after `f F t T r`, nor in Insert mode or on the command line.
- **VI-11** [P2][M4] If the environment variable `SI_ON_NORMAL` is set, it runs asynchronously with `sh -c` every time Normal mode is entered (e.g. `fcitx5-remote -c` to turn the IME off) — I
  - **Decided** (P2): it also runs once at startup, since si starts in Normal mode.
- **VI-12** [P5][M1] The keys every editor has work everywhere: the arrows, Home, End, PageUp, PageDown and Delete in Normal, Visual and Insert mode, and Left, Right, Home, End, BS and Delete on the command line — U
- **VI-13** [P5][M1] Finishing takes few keys: Ctrl-s saves in every mode (Insert mode stays Insert mode); `ZZ` saves if modified and quits, like `:x`; `ZQ` quits without saving, like `:q!`; `q` quits like `:q` (as in less; si has no macros). Ctrl-c with nothing to cancel says `q to quit` — U

## The AI era

- **AI-1** [P2][M4] `:ref` sends `path:start-end` (relative to the current directory) to the clipboard through OSC 52, to hand a location to an agent. In Visual mode it uses the selection; in Normal mode, the current line — I
  - **Decided** (P5): `:ref` takes a range like the other ex commands, so it is `:'<,'>ref` from Visual mode. A file outside the current directory is given by its absolute path.

## Performance (measured on a release build)

- **PERF-1** [P3][M1] `si +q` opens and closes a 10,000-line file within 5 ms (median of hyperfine) — B
  - **Decided** (P3): si needs a terminal, so `scripts/bench.sh` runs hyperfine inside a private tmux when it is installed. Otherwise `scripts/pty_bench.py` spawns the release binary in a pty; the median it reports includes starting the process from Python, and the test (`perf_1_bench_startup_under_5ms`) asserts that raw median, so it is stricter than hyperfine's.
- **PERF-2** [P3][M1] On a 100,000-line file, handling one key (motion, insert, delete, undo) and building the Frame take at most 1 ms at p99 — B
  - **Decided** (P3): the time of a key is `App::update` plus `Term::draw` into memory, i.e. everything before the write to the terminal, measured on 80x24 and 240x60 screens.
- **PERF-3** [P3][M1] After one character is inserted, only that line is sent to the terminal again — U (bytes written)
  - **Decided** (P2): plus the status line, whose `line:col` changed, and the cursor position.
  - **Decided** (P3): a frame that rewrites anything is wrapped in synchronized output (`CSI ? 2026 h` … `CSI ? 2026 l`), so a terminal that supports it never shows half of a frame; terminals that don't know it ignore it. A frame that only moves the cursor is sent as it is.
- **PERF-4** [P3][M1] On a 100,000-line file, a search that matches nothing (the worst case) finishes within 50 ms — B
  - **Decided** (P3): a line that lacks a literal the pattern requires (the longest plain run, e.g. `foo` in `foo\d+bar`) is skipped without running the regex, because regex-lite does about 20 ns a byte and a 100,000-line file is several MB.
  - **Decided** (P3): regex-lite stays, and the worst case is held by running it in parallel. A buffer of 512 KiB or more is searched in 8 chunks of lines (a fixed number, so the result never depends on the machine) on `std::thread::scope` threads that only read the text; the first chunk in search order that has a match wins, so the match is exactly the one a single thread finds. The first 256 lines run on one thread before any starts, so a match near the cursor costs no thread. 8 threads bring a regex with no useful literal on 7.65 MB from about 150 ms to about 24 ms, on a machine with 8 or more cores.
- **PERF-5** [P4][M1] The release binary is at most 2 MB (strip, LTO). With a 10 MB file open, RSS is at most 50 MB — B
  - **Decided** (P4): the test asserts the peak (VmHWM) after the first frame, which is never lower than the RSS at that moment. It runs on Linux only.

## Validation

- **VAL-1** [P2][M6] With tools/perception (not counted in src/), show 20 trials of a 60-line code screen in which exactly one word changed, and record the time to answer the number of the changed line and whether the answer was right. Condition A is si's display; condition B is syntax highlighting with a line-level diff background (close to the defaults of common editors). Output medians and 95% bootstrap confidence intervals — B
