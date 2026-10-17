//! GUI-14, GUI-15
//! A neutral demo project for the offscreen renders and the tests: a small made-up project ("lantern")
//! in a git repository whose working tree differs from HEAD, so the signs, the git letters in the
//! tree and the counts in the status line have something to show. Nothing of the owner's is read.
use std::io;
use std::path::Path;
use std::process::Command;

const README: &str = "\
# Lantern

A tiny made-up project that **sim** opens in a window, to show what is hidden.

## Usage

Run `lantern build` to compile the notes, then `lantern serve`.

- [x] read the notes
- [ ] write the tests
- [ ] ship it

## Layout

| path | what it is |
| --- | --- |
| src/main.rs | the entry point |
| src/notes.rs | the notes parser |

> Depth is not decoration: it shows what is hidden.

```rust
fn main() {
    println!(\"hello, lantern\");
}
```

See the [notes](docs/notes.md) and the [license](LICENSE) for more.
";

/// README.md as committed: the version above with three lines different, so the sign column shows a
/// change, an addition and a deletion.
fn readme_head() -> String {
    README
        .replace("compile the notes", "compile your notes")
        .replace("- [ ] ship it\n", "- [ ] ship it\n- [ ] celebrate\n")
        .replace("## Usage\n\n", "## Usage\n\nThis line was deleted later.\n\n")
}

const MAIN_RS: &str = "\
//! lantern: turns notes into a lit page.
use std::collections::HashMap;
use std::env;

mod notes;

/// How many notes a page holds before it is split.
const PAGE: usize = 24;

// TODO: read the page size from the environment
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for word in args.iter().map(String::as_str) {
        *counts.entry(word).or_insert(0) += 1;
    }
    let lit = notes::light(&counts, PAGE);
    println!(\"{} words, {} lit\", counts.len(), lit);
}
";

fn main_head() -> String {
    MAIN_RS.replace("let lit = notes::light(&counts, PAGE);", "let lit = notes::light(&counts, 8);")
}

const NOTES_RS: &str = "\
//! The notes parser.
use std::collections::HashMap;

/// A note is lit when it was said more than once.
pub fn light(counts: &HashMap<&str, usize>, page: usize) -> usize {
    counts.values().filter(|&&n| n > 1).take(page).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_14_a_word_said_twice_is_lit() {
        let mut m = HashMap::new();
        m.insert(\"lamp\", 2);
        m.insert(\"moth\", 1);
        assert_eq!(light(&m, 10), 1);
    }
}
";

const NOTES_MD: &str = "\
# Notes

Lanterns are lit at dusk. Each note is a line; a note said twice is lit.

## Format

One note per line, no markup.
";

/// A Markdown file in Japanese: wide chars, kinsoku, the font's fallback, and the place an input
/// method's text goes.
pub const NIHONGO: &str = "\
# ランタン

夕暮れに灯りがともる、小さな架空のプロジェクトです。**sim** は見えないものを見えるようにします。

## 使い方

`lantern build` でメモをまとめ、`lantern serve` で公開します。

- [x] メモを読む
- [ ] テストを書く
- [ ] 公開する

> 奥行きは飾りではなく、隠れているものを見せるためにあります。
";

/// A long source file, for a screen with a lot of text on it.
fn dense_rs() -> String {
    let mut s = String::from(
        "//! A lot of lines, so that a screen is full of text.\nuse std::collections::HashMap;\n\n",
    );
    for i in 0..220 {
        s.push_str(&format!(
            "/// Lamp number {i}: lit when it was said more than {} times.\npub fn lamp_{i}(counts: &HashMap<&str, usize>, page: usize) -> usize {{\n    counts.values().filter(|&&n| n > {}).take(page).count() + {i}\n}}\n\n",
            i % 7,
            i % 7
        ));
    }
    s
}

const CARGO: &str = "\
[package]
name = \"lantern\"
version = \"0.1.0\"
edition = \"2024\"
";

const BUILD_SH: &str = "#!/usr/bin/env bash\nset -eu\ncargo build --release\n";

/// A file that holds a real U+202E (right-to-left override) next to the literal text `<U+202E>`,
/// and a real escape char next to the literal `^[`: the first of each is hidden in a terminal and
/// is shown here in another form; the second of each is only ASCII (UI-15).
pub const HIDDEN: &str = "\
invoice_final\u{202e}fdp.exe
the literal text:  <U+202E>  and  ^[  (only ASCII)
the real chars:   [\u{202e}]  and  [\u{1b}]  (shown in another form)
if level == \"user\u{202e} \u{2066}admin\u{2069} \u{2066}\" { grant(); }
a zero\u{200b}width space hides inside this word
";

/// Runs git in `dir` with none of the user's configuration and a fixed identity.
fn git(dir: &Path, args: &[&str]) -> io::Result<()> {
    let out = Command::new("git")
        .args(["-c", "user.name=demo", "-c", "user.email=demo@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("XDG_CONFIG_HOME", "/nonexistent")
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr))))
    }
}

fn write(dir: &Path, name: &str, text: &str) -> io::Result<()> {
    let at = dir.join(name);
    if let Some(parent) = at.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(at, text)
}

/// Makes the demo project in `dir` (which should be empty or absent): a repository with one commit,
/// then a working tree that differs from it.
pub fn create(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    git(dir, &["init", "-q"])?;
    write(dir, "README.md", &readme_head())?;
    write(dir, "Cargo.toml", CARGO)?;
    write(dir, "LICENSE", "MIT License (a made-up demo)\n")?;
    write(dir, "src/main.rs", &main_head())?;
    write(dir, "src/notes.rs", NOTES_RS)?;
    write(dir, "docs/notes.md", NOTES_MD)?;
    write(dir, "docs/nihongo.md", NIHONGO)?;
    write(dir, "src/dense.rs", &dense_rs())?;
    write(dir, "scripts/build.sh", BUILD_SH)?;
    write(dir, "hidden.txt", HIDDEN)?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", "first light"])?;
    // the working tree differs from HEAD
    write(dir, "README.md", README)?;
    write(dir, "src/main.rs", MAIN_RS)?;
    write(dir, "src/scratch.rs", "// an untracked file\npub fn scratch() {}\n")?;
    Ok(())
}

/// A file the working tree has and HEAD does not, whose name is longer than the tree's panel is wide: its
/// row is cut with a `…` in the changes view.
pub const LONG_NAME: &str = "docs/a-note-with-a-name-too-long-for-the-panel-to-show.md";

/// More that differs from HEAD, for the changes view (DIFF-1): a committed file deleted
/// (`scripts/build.sh`), one moved with a line added to it (`docs/notes.md`), and an untracked one with a
/// long name. Made after `create`, so that the other scenes have the project as it was.
pub fn diverge(dir: &Path) -> io::Result<()> {
    std::fs::remove_file(dir.join("scripts/build.sh"))?;
    git(dir, &["mv", "docs/notes.md", "docs/lantern-notes.md"])?;
    let moved = dir.join("docs/lantern-notes.md");
    let mut text = std::fs::read_to_string(&moved)?;
    text.push_str("\nA line added after the move.\n");
    std::fs::write(moved, text)?;
    write(dir, LONG_NAME, "# a note\n\nthe panel cannot show all of its name\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_14_the_hidden_file_has_the_real_char_and_the_literal_text() {
        assert!(HIDDEN.contains('\u{202e}'));
        assert!(HIDDEN.contains("<U+202E>"));
        assert_eq!(HIDDEN.matches('\u{202e}').count(), 3);
    }

    #[test]
    fn gui_14_a_demo_project_can_diverge_from_head_by_a_deletion_a_move_and_a_long_name() {
        let dir = crate::safe_io::PrivateDir::new("sim-gui-demo-test-").unwrap();
        let at = dir.path().join("lantern");
        create(&at).unwrap();
        diverge(&at).unwrap();
        assert!(!at.join("scripts/build.sh").exists());
        assert!(!at.join("docs/notes.md").exists());
        assert!(at.join("docs/lantern-notes.md").exists());
        assert!(at.join(LONG_NAME).exists() && LONG_NAME.len() > 40);
        // and HEAD still has what was taken away
        let out = git_out(&at, &["status", "--porcelain"]);
        assert!(out.contains(" D scripts/build.sh"), "{out}");
        assert!(out.contains("RM docs/notes.md -> docs/lantern-notes.md"), "{out}");
    }

    fn git_out(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    fn gui_14_the_head_differs_from_the_working_tree() {
        assert_ne!(readme_head(), README);
        assert_ne!(main_head(), MAIN_RS);
    }
}
