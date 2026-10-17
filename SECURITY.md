# Security policy

sim is an editor for files you did not write, so a report that a file, a folder or a repository can do something to its reader that sim promised it cannot do is exactly what this policy is for.

## Reporting a vulnerability

Report it privately, through GitHub's private vulnerability reporting: on the repository's **Security** tab, choose **Report a vulnerability** ([direct link](https://github.com/efg-technologies/sim/security/advisories/new)).

**Do not open a public issue, a pull request or a discussion for a vulnerability**, and do not post it elsewhere before a fix is out.

Please include:

- the output of `sim --version`, your operating system and your terminal (and tmux, if you use it);
- the smallest file or repository that shows the problem, and the keys you pressed, if any;
- what you expected, what happened, and which promise below it breaks.

The report, the fix and the advisory are handled in the private advisory thread. There is no response-time guarantee: sim is a young project.

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x | yes |

Nothing earlier was released.

## What counts as a vulnerability

sim's threat model: you open files and repositories you did not write (a clone, an unpacked archive, a pull request checkout, a file another process writes). The attacker controls their content, not your machine. sim tries to make sure that content cannot hide from you, cannot reconfigure the editor, and cannot make the editor run code. In scope:

- **Content that hides.** A bidirectional control, a zero-width character, a stray byte-order mark, a control character or an invalid byte that is not drawn visibly, in any state a cell can be in (SAFE-8, UI-15, UI-6).
- **Terminal escapes.** Anything from a file's content, or from a file name, that reaches the terminal as an escape sequence instead of being drawn as a visible character (SAFE-8, TREE-1, GREP-7).
- **Settings read from content.** A modeline, or any configuration, command or option that sim reads from a file's content or from a project's directory (SAFE-9).
- **A repository that makes sim run code.** git run where the repository's own config can hold a command (`core.fsmonitor`, a filter driver, and the like), a hook, or ripgrep run with a config file or `--pre` (MARK-1, GREP-7, GREP-8).
- **Links.** A search or a preview that reads a file outside the project root through a symlink, or a save that writes through a link planted by someone who is neither you nor root (SAFE-1, GREP-7).
- **Lost text.** A save, a merge, a crash, `kill -9` or a closed terminal that loses text without an explicit action of yours; a save that overwrites a file that changed on disk since it was read; a snapshot directory that others can read or write (SAFE-1 to SAFE-7, DISK-1 to DISK-6).
- **Non-text files.** Binary or non-UTF-8 content that sim opens for writing, or that a save corrupts (SAFE-4).
- **The network.** Any network access by sim at all (SAFE-9).

The full model, one threat at a time, with what sim does about each: <https://sim.efg-technologies.com/security/>. The requirement IDs refer to [SPEC.md](SPEC.md), where every requirement has tests.

## What does not

- sim is **not a sandbox**. It runs with your privileges. Showing you a hidden character does not stop you from running the code it hides.
- Look-alike letters from other scripts (homoglyphs) are not detected; sim draws a fixed, documented list of characters and is not a check of all Unicode confusables.
- A bug in your terminal, your terminal multiplexer, git or ripgrep. If sim could have kept it from mattering, that is in scope; tell us either way.
- An attacker who can already change sim itself, your home directory, your global git configuration or your terminal (the assumptions of the model).
- Language servers: sim has none yet. A sandboxed client is planned.
- A vulnerability in a dependency: please report it to that project too, and tell us if sim is affected.

sim's security claims come from the project's own specification and tests. No independent audit has been done. A gap between a claim and what sim does is a vulnerability report we want.
