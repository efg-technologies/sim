//! UI-2, UI-3: the tree of the interface: the header, the icons and `zi`, the git status of each
//! entry with its letter and color, the dot of a directory, the dimmed dotfiles and ignored
//! entries, the open file's row, and the guides. The marks and the icons are pure; the rest is
//! driven through sim's state on a real repository.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use si::core::view::Style;
use sim::color::{Accent, Syntax, Tones};
use sim::icons;
use sim::layout::Skin;
use sim::marks::{Git, Marks};
use sim::paint::{Look, Paint};
use sim::sim::{SimEffect, SimEvent, tilde};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

fn marks(status: &str, prefix: &str) -> Marks {
    Marks::new(status.replace('|', "\0").as_bytes(), prefix, &HashSet::new())
}

#[test]
fn ui_3_the_git_status_of_a_path_is_its_conflict_else_the_work_tree_s_letter_else_the_index_s() {
    let m = marks(
        " M a| D b|A  c|AM d|R  e|old|?? f|UU g|AA h|DD i|M  j| T k|C  l|!! x.log|!! target/|!! z",
        "",
    );
    let at = |p: &str| m.git(Path::new(p));
    assert_eq!(at("a"), Some(Git::Modified));
    assert_eq!(at("b"), Some(Git::Deleted));
    assert_eq!(at("c"), Some(Git::Added));
    assert_eq!(at("d"), Some(Git::Modified), "added, then modified: the work tree's letter");
    assert_eq!(at("e"), Some(Git::Renamed), "the new name");
    assert_eq!(at("old"), None, "the old name is not an entry");
    assert_eq!(at("f"), Some(Git::Untracked));
    for c in ["g", "h", "i"] {
        assert_eq!(at(c), Some(Git::Conflict), "{c}");
    }
    assert_eq!(at("j"), Some(Git::Modified));
    assert_eq!(at("k"), Some(Git::Modified), "a type change is a change");
    assert_eq!(at("l"), Some(Git::Added), "a copy is added");
    for p in ["x.log", "target", "z"] {
        assert_eq!(at(p), None, "{p}: ignored entries have no status");
    }
    let letters: String =
        [Git::Untracked, Git::Added, Git::Renamed, Git::Modified, Git::Deleted, Git::Conflict]
            .iter()
            .map(|g| g.letter())
            .collect();
    assert_eq!(letters, "UARMD!");
}

#[test]
fn ui_3_a_directory_shows_the_strongest_status_below_it_and_ignored_entries_cover_their_directories()
 {
    let m = marks(
        "?? a/b/new|A  a/c/added| M a/d/mod|R  n/new|o/old| D gone/x|!! target/|!! f.log",
        "",
    );
    let below = |p: &str| m.below(Path::new(p));
    assert_eq!(below("a/b"), Some(Git::Untracked));
    assert_eq!(below("a/c"), Some(Git::Added));
    assert_eq!(below("a"), Some(Git::Modified), "the strongest of untracked, added and modified");
    assert_eq!(below("n"), Some(Git::Renamed));
    assert_eq!(below("o"), Some(Git::Renamed), "the directory the rename left");
    assert_eq!(
        below("gone"),
        Some(Git::Deleted),
        "a deleted file is not in the tree, its directory says so"
    );
    assert_eq!(below("target"), None);
    assert!(
        m.ignored(Path::new("target"))
            && m.ignored(Path::new("target/debug/x.o"))
            && m.ignored(Path::new("f.log"))
    );
    assert!(
        !m.ignored(Path::new("targets")) && !m.ignored(Path::new("a")),
        "a name that only starts alike is not"
    );
    // Conflict outranks everything, and a prefix of the root is left out.
    let c = marks("UU sub/x/c| M sub/y| M other/z", "sub/");
    assert_eq!(c.below(Path::new("x")), Some(Git::Conflict));
    assert_eq!(c.git(Path::new("y")), Some(Git::Modified));
    assert_eq!(c.below(Path::new("other")), None, "outside the root");
}

#[test]
fn ui_3_a_change_on_disk_is_not_a_git_status_and_git_s_changes_are_not_on_disk() {
    let disk: HashSet<PathBuf> = [PathBuf::from("a/b.rs")].into();
    let m = Marks::new(b" M c.rs\0", "", &disk);
    assert!(m.on_disk(Path::new("a/b.rs")) && !m.on_disk(Path::new("c.rs")));
    assert!(
        m.changed(Path::new("c.rs")) && m.changed(Path::new("a/b.rs")),
        "MARK-1's set has both"
    );
    assert_eq!(m.git(Path::new("a/b.rs")), None);
}

// ---- UI-2 ----

#[test]
fn ui_2_the_icons_cover_the_names_and_extensions_of_the_table_and_chezmoi_s_names() {
    let default = icons::of("whatever.zzz", false, false);
    let table = [
        "a.rs",
        "a.md",
        "a.toml",
        "a.json",
        "a.yaml",
        "a.yml",
        "a.sh",
        "a.zsh",
        "a.bash",
        "a.lua",
        "a.py",
        "a.js",
        "a.ts",
        "a.tsx",
        "a.jsx",
        "a.go",
        "a.c",
        "a.h",
        "a.cpp",
        "a.hpp",
        "a.html",
        "a.css",
        "a.scss",
        "a.sql",
        "a.png",
        "a.jpg",
        "a.svg",
        "a.pdf",
        "Dockerfile",
        "Makefile",
        ".gitignore",
        ".gitattributes",
        "LICENSE",
        "LICENSE-MIT",
        "README",
        "README.md",
        "Cargo.lock",
    ];
    for name in table {
        let glyph = icons::of(name, false, false);
        assert_ne!(glyph, default, "{name} has an icon of its own");
        assert!(
            ('\u{e000}'..='\u{f8ff}').contains(&glyph),
            "{name}: {glyph:?} is a Nerd Font glyph"
        );
    }
    // Case is ignored, and the file's own name wins over its extension.
    assert_eq!(icons::of("MAKEFILE", false, false), icons::of("Makefile", false, false));
    assert_ne!(icons::of("Cargo.lock", false, false), icons::of("a.toml", false, false));
    // chezmoi's decorations are taken off: `dot_zshrc.tmpl` is `.zshrc`, `run_x.sh.tmpl` is a script.
    assert_eq!(icons::of("dot_zshrc.tmpl", false, false), icons::of(".zshrc", false, false));
    assert_eq!(
        icons::of("private_dot_gitignore", false, false),
        icons::of(".gitignore", false, false)
    );
    assert_eq!(icons::of("executable_x.sh", false, false), icons::of("a.sh", false, false));
    assert_ne!(icons::of("dot_zshrc.tmpl", false, false), default);
    // Folders: open and closed.
    let (closed, open) = (icons::of("src", true, false), icons::of("src", true, true));
    assert_ne!(closed, open);
    assert_ne!(closed, default);
}

#[test]
fn ui_2_the_root_is_shown_below_the_home_directory_as_tilde() {
    let home = Path::new("/home/me");
    assert_eq!(
        tilde(Path::new("/home/me/.local/share/chezmoi"), Some(home)),
        "~/.local/share/chezmoi"
    );
    assert_eq!(tilde(Path::new("/home/me"), Some(home)), "~");
    assert_eq!(
        tilde(Path::new("/home/me2/x"), Some(home)),
        "/home/me2/x",
        "only the same directory"
    );
    assert_eq!(tilde(Path::new("/srv/x"), Some(home)), "/srv/x");
    assert_eq!(tilde(Path::new("/home/me/x"), None), "/home/me/x", "no home: as it is");
}

#[test]
fn ui_2_the_choice_of_icons_is_remembered_in_the_state_directory() {
    let dir = temp_dir("ui2-iconfile");
    assert!(sim::io::load_icons(&dir.0), "on at the first run");
    sim::io::save_icons(&dir.0, false).unwrap();
    assert!(!sim::io::load_icons(&dir.0));
    sim::io::save_icons(&dir.0, true).unwrap();
    assert!(sim::io::load_icons(&dir.0));
    for junk in ["", "maybe\n", "OFF\n"] {
        std::fs::write(dir.0.join("icons"), junk).unwrap();
        assert!(sim::io::load_icons(&dir.0), "{junk:?}: only `off` turns them off");
    }
}

// ---- the tree on screen ----

const FILES: [&str; 8] = [
    "src/main.rs",
    "src/lib.rs",
    "docs/guide.md",
    ".github/ci.yml",
    "README.md",
    ".gitignore=target/\n*.log\n",
    "Cargo.lock",
    "old/gone.txt",
];

/// A repository of `FILES` with: `src/main.rs` modified, `docs/new.md` untracked, `src/added.rs`
/// added to the index, `old/gone.txt` deleted, `Cargo.lock` renamed to `Cargo.lock2`, ignored `x.log`
/// and `target/out`.
fn git_rig(name: &str, skin: Skin, size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, &FILES, &[], size, |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        std::fs::write(e.cwd.join("src/main.rs"), "changed\n").unwrap();
        make(&e.cwd, &["docs/new.md", "x.log", "target/out"]);
        std::fs::write(e.cwd.join("src/added.rs"), "new\n").unwrap();
        git(&e.cwd, &["add", "src/added.rs"]);
        std::fs::remove_file(e.cwd.join("old/gone.txt")).unwrap();
        git(&e.cwd, &["mv", "Cargo.lock", "Cargo.lock2"]);
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
        e.icons = true;
        e.home = e.cwd.parent().map(Path::to_path_buf);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

fn tones() -> Tones {
    Tones::new(&palette(BG))
}

/// The row of the screen whose text has `what`.
fn row(r: &Rig, what: &str) -> usize {
    let y = r.shot.frame.rows.iter().position(|x| x.text.contains(what));
    assert!(y.is_some(), "no row with {what:?}:\n{}", dump_screen(&r.shot));
    y.unwrap()
}

fn col(r: &Rig, y: usize, what: &str) -> u16 {
    let byte = r.shot.frame.rows[y].text.find(what).unwrap();
    r.shot.frame.rows[y].text[..byte]
        .chars()
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u16)
        .sum()
}

/// What a cell is painted with, as the Painter merges the ranges that hold it: each field comes
/// from the first range that has one.
fn paint(r: &Rig, y: usize, x: u16) -> Paint {
    let on = r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
    on.fold(Paint::default(), |a, p| Paint {
        fg: a.fg.or(p.fg),
        bg: a.bg.or(p.bg),
        fade: if a.fade > 0 { a.fade } else { p.fade },
        bold: a.bold || p.bold,
        soft: a.soft || p.soft,
        sealed: a.sealed || p.sealed,
    })
}

fn style(r: &Rig, y: usize, x: u16) -> Option<Style> {
    r.shot.frame.rows[y].spans.iter().find(|(c, _)| c.contains(&x)).map(|p| p.1)
}

fn look(r: &Rig, y: usize, x: u16) -> Option<Look> {
    r.shot.looks[y].iter().find(|(c, _)| c.contains(&x)).map(|p| p.1)
}

/// Opens every directory of the tree, one at a time from the top (`l` on a directory), and goes
/// back to the top.
fn expand(r: &mut Rig) {
    for _ in 0..20 {
        let g = r.sim.geo();
        let rows = usize::from(g.y0)..usize::from(g.y0 + g.rows);
        let closed = rows.clone().find(|&y| r.shot.frame.rows[y].text.contains(icons::CLOSED));
        let Some(y) = closed else { break };
        r.keys("gg");
        r.keys(&"j".repeat(y - usize::from(g.y0)));
        r.keys("l");
    }
    r.keys("gg");
}

/// Puts the panel's cursor on the row that shows `name`.
fn goto(r: &mut Rig, name: &str) {
    let g = r.sim.geo();
    let y = row(r, name);
    r.keys("gg");
    r.keys(&"j".repeat(y - usize::from(g.y0)));
}

#[test]
fn ui_2_the_first_row_of_the_panel_is_the_root_below_home_in_bold_with_the_open_folder() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = git_rig("ui2-head", skin, (100, 30));
        let head = row(&r, "~/");
        assert_eq!(
            head,
            usize::from(skin == Skin::Depth),
            "{skin:?}: the first row inside the box"
        );
        let root = format!("~/sim-it-{}-ui2-head", std::process::id());
        let text = &r.shot.frame.rows[head].text;
        assert!(text.contains(&root), "{skin:?}: {text}");
        let x = col(&r, head, "~/");
        assert!(paint(&r, head, x).bold, "{skin:?}: bold");
        assert_eq!(paint(&r, head, x).bg, Some(tones().bar), "{skin:?}: on the bar's surface");
        let folder = col(&r, head, &icons::OPEN.to_string());
        assert_eq!(paint(&r, head, folder).fg, Some(tones().focus), "{skin:?}: the open folder");
        // The tree starts under it, and the mouse, the cursor and the pages count from there.
        let g = r.sim.geo();
        assert_eq!((g.head, usize::from(g.y0)), (1, head + 1), "{skin:?}");
        assert!(
            r.shot.frame.rows[head + 1].text.contains(".github"),
            "{skin:?}: {}",
            r.shot.frame.rows[head + 1].text
        );
    }
}

#[test]
fn ui_1_calm_and_a_screen_too_short_for_it_draw_none_of_the_interface() {
    let r = git_rig("ui1-calm", Skin::Calm, (100, 30));
    let g = r.sim.geo();
    assert_eq!(g.head, 0);
    assert!(r.shot.frame.rows[0].text.starts_with("▸ .github/"), "{}", r.shot.frame.rows[0].text);
    assert!(r.shot.paints.iter().all(Vec::is_empty), "calm draws no paint");
    assert!(
        !r.shot.frame.rows.iter().any(|x| x.text.contains('●') || x.text.contains(icons::CLOSED))
    );
    // Without the palette (LOOK-8), or on a screen under 10 rows, there is no header either.
    let short = git_rig("ui1-short", Skin::Flat, (100, 9));
    assert_eq!(short.sim.geo().head, 0);
}

#[test]
fn ui_2_folders_and_files_have_their_icons_and_zi_turns_them_off_and_on() {
    let mut r = git_rig("ui2-icons", Skin::Depth, (100, 30));
    expand(&mut r);
    let t = tones();
    let dir = row(&r, ".github");
    let line = &r.shot.frame.rows[dir].text;
    assert!(line.contains(&format!("{} .github", icons::of(".github", true, true))), "{line}");
    let main = row(&r, "main.rs");
    let glyph = icons::of("main.rs", false, false);
    let x = col(&r, main, &glyph.to_string());
    assert_eq!(paint(&r, main, x).fg, Some(t.focus), "icons are monochrome: the secondary gray");
    let folder = col(&r, dir, &icons::of(".github", true, true).to_string());
    assert_eq!(paint(&r, dir, folder).fg, Some(t.focus), "folders too");
    assert!(
        !r.shot.frame.rows[dir].text.contains(".github/"),
        "no slash after a directory with an icon"
    );
    // `zi` is typed in the text, like `zl`; the App gets both keys.
    goto(&mut r, "README.md");
    r.keys("<CR>");
    r.keys("zi");
    assert_eq!(r.status(), "icons off");
    assert!(r.log.iter().any(|e| matches!(e, SimEffect::Icons(false))), "asked to be remembered");
    let (dir, file) = (row(&r, ".github"), row(&r, "ci.yml"));
    assert!(
        r.shot.frame.rows[dir].text.contains("▾ .github/"),
        "today's ▾: {}",
        r.shot.frame.rows[dir].text
    );
    assert!(
        !r.shot.frame.rows[file].text.contains(icons::of("ci.yml", false, false)),
        "no icon on a file"
    );
    r.keys("zi");
    assert_eq!(r.status(), "icons on");
    assert!(r.shot.frame.rows[row(&r, ".github")].text.contains(icons::OPEN));
    assert!(r.log.iter().any(|e| matches!(e, SimEffect::Icons(true))));
}

#[test]
fn ui_3_a_file_has_its_git_letter_at_the_right_edge_and_its_name_in_the_color_of_its_status() {
    let mut r = git_rig("ui3-letters", Skin::Depth, (100, 30));
    expand(&mut r);
    let (t, pw) = (tones(), r.sim.widths().0);
    let px = r.sim.geo().px;
    let end = px + pw - 1; // the last column of the panel
    for (name, letter, accent) in [
        ("main.rs", 'M', Accent::Amber),
        ("added.rs", 'A', Accent::Green),
        ("new.md", 'U', Accent::Green),
        ("Cargo.lock2", 'R', Accent::Amber), // a rename is told by its letter, in the modification's color
    ] {
        let y = row(&r, name);
        let text: Vec<char> = r.shot.frame.rows[y].text.chars().collect();
        assert_eq!(text[usize::from(end)], letter, "{name}: {}", r.shot.frame.rows[y].text);
        assert_eq!(paint(&r, y, end).fg, Some(t.fg(accent)), "{name}: the letter's color");
        let x = col(&r, y, name);
        assert_eq!(paint(&r, y, x).fg, Some(t.fg(accent)), "{name}: the name's color");
        assert_eq!(
            look(&r, y, x) == Some(Look::Em),
            letter == 'U',
            "{name}: only an untracked name is italic"
        );
    }
    // A file with no status has no letter and no color of its own.
    let y = row(&r, "lib.rs");
    assert_eq!(r.shot.frame.rows[y].text.chars().nth(usize::from(end)), Some(' '));
    assert_eq!(paint(&r, y, col(&r, y, "lib.rs")).fg, None);
    // The change style of MARK-1 is for what changed on disk, not for git's changes.
    let y = row(&r, "main.rs");
    assert_eq!(style(&r, y, col(&r, y, "main.rs")), None, "git's change shows by its letter");
}

#[test]
fn ui_3_a_conflict_is_bold_red_and_a_deleted_file_s_directory_has_the_red_dot() {
    let mut r = rig_env("ui3-conflict", &["a.rs", "b.rs", "d/x.rs"], &[], (80, 20), |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        git(&e.cwd, &["checkout", "-q", "-b", "other"]);
        std::fs::write(e.cwd.join("a.rs"), "theirs\n").unwrap();
        git(&e.cwd, &["commit", "-qam", "theirs"]);
        git(&e.cwd, &["checkout", "-q", "-"]);
        std::fs::write(e.cwd.join("a.rs"), "mine\n").unwrap();
        git(&e.cwd, &["commit", "-qam", "mine"]);
        let merge = std::process::Command::new("git")
            .args(["-c", "user.name=sim", "-c", "user.email=sim@example.com"])
            .args(["-c", "commit.gpgsign=false", "merge", "other"])
            .current_dir(&e.cwd)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(!merge.status.success(), "a conflict");
        std::fs::remove_file(e.cwd.join("d/x.rs")).unwrap();
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    let t = tones();
    let y = row(&r, "a.rs");
    let end = r.sim.widths().0 - 1;
    assert_eq!(r.shot.frame.rows[y].text.chars().nth(usize::from(end)), Some('!'));
    let (p, q) = (paint(&r, y, end), paint(&r, y, col(&r, y, "a.rs")));
    assert!(p.bold && q.bold, "bold");
    assert_eq!((p.fg, q.fg), (Some(t.fg(Accent::Red)), Some(t.fg(Accent::Red))));
    let y = row(&r, "d/");
    let dot = col(&r, y, "●");
    assert_eq!(paint(&r, y, dot).fg, Some(t.fg(Accent::Red)), "something below it was deleted");
}

#[test]
fn ui_3_a_directory_has_a_dot_in_the_color_of_the_strongest_status_below_it_closed_or_open() {
    let r = git_rig("ui3-dots", Skin::Depth, (100, 30));
    let (t, pw) = (tones(), r.sim.widths().0);
    let end = r.sim.geo().px + pw - 2; // the edge has a blank, then the letter: the dot is on the letter's cell
    let end = end + 1;
    for (dir, accent) in [("docs/", Accent::Green), ("src/", Accent::Amber), ("old/", Accent::Red)]
    {
        let _ = dir;
        let _ = accent;
    }
    // Closed: the directories, with the strongest of what is in them.
    for (name, accent) in [("docs", Accent::Green), ("src", Accent::Amber), ("old", Accent::Red)] {
        let y = row(&r, name);
        let text: Vec<char> = r.shot.frame.rows[y].text.chars().collect();
        assert_eq!(text[usize::from(end)], '●', "{name}: {}", r.shot.frame.rows[y].text);
        assert_eq!(paint(&r, y, end).fg, Some(t.fg(accent)), "{name}");
    }
    let y = row(&r, ".github");
    assert_eq!(
        r.shot.frame.rows[y].text.chars().nth(usize::from(end)),
        Some(' '),
        "a clean one has none"
    );
    // A dot is news: it never fades, and no `~` is drawn in the interface.
    assert!(paint(&r, row(&r, "src"), end).fade == 0);
    let head = usize::from(r.sim.geo().y0) - 1;
    let tildes =
        r.shot.frame.rows.iter().enumerate().filter(|(i, x)| *i != head && x.text.contains(" ~"));
    assert_eq!(tildes.count(), 0, "calm's `~` is not drawn");
}

#[test]
fn ui_3_dotfiles_and_ignored_entries_are_in_the_secondary_gray_and_a_changed_one_is_not() {
    let mut r = git_rig("ui3-dim", Skin::Depth, (100, 30));
    expand(&mut r);
    let secondary = |r: &Rig, name: &str| {
        let y = row(r, name);
        style(r, y, col(r, y, name)) == Some(Style::Secondary)
    };
    assert!(secondary(&r, ".github"), "a dotted directory");
    assert!(secondary(&r, ".gitignore"), "a dotfile");
    assert!(secondary(&r, "target"), "an ignored directory");
    assert!(secondary(&r, "x.log"), "an ignored file");
    assert!(secondary(&r, "out"), "what is inside an ignored directory");
    assert!(!secondary(&r, "ci.yml"), "a file in a dotted directory is not a dotfile");
    assert!(!secondary(&r, "docs") && !secondary(&r, "README.md") && !secondary(&r, "lib.rs"));
    assert!(!secondary(&r, "main.rs"), "a changed file has its status's color");
}

#[test]
fn ui_3_the_open_file_s_row_is_on_the_surface_and_the_indent_guides_are_in_the_lines_color() {
    let mut r = git_rig("ui3-open", Skin::Depth, (100, 30));
    expand(&mut r);
    goto(&mut r, "main.rs");
    r.keys("<CR>");
    let t = tones();
    let open = r.app().path.clone().unwrap();
    // (the title of the editor's box has the name too: look from the first row of the body)
    let name = Path::new(&open).file_name().unwrap().to_str().unwrap();
    let y = (usize::from(r.sim.geo().y0)..)
        .find(|&y| r.shot.frame.rows[y].text.contains(name))
        .unwrap();
    let px = r.sim.geo().px;
    assert_eq!(paint(&r, y, px + 5).bg, Some(t.card), "the open file's row");
    let others = (0..r.shot.frame.rows.len())
        .filter(|&i| i != y && paint(&r, i, px + 5).bg == Some(t.card))
        .count();
    assert_eq!(others, 0, "no other row");
    let main = row(&r, "lib.rs");
    let g = col(&r, main, "│");
    assert_eq!(paint(&r, main, g).fg, Some(t.line), "a guide for the level above");
    let top = row(&r, "README.md");
    assert!(
        !r.shot.frame.rows[top].text[..40].contains('│')
            || r.shot.frame.rows[top].text.starts_with('│'),
        "none for the top level"
    );
}

#[test]
fn ui_3_git_ignored_entries_come_from_a_second_status_and_the_listing_has_only_those_lines() {
    let r = git_rig("ui3-ignored", Skin::Depth, (100, 30));
    let status = sim::io::git_ignored(&r.sim.root).unwrap();
    let fields: Vec<&[u8]> = status.split(|&b| b == 0).filter(|f| !f.is_empty()).collect();
    assert!(fields.iter().all(|f| f.starts_with(b"!! ")), "{fields:?}");
    let names: Vec<String> =
        fields.iter().map(|f| String::from_utf8_lossy(&f[3..]).into_owned()).collect();
    assert!(
        names.contains(&"target/".to_string()) && names.contains(&"x.log".to_string()),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|n| n.contains("out")),
        "an ignored directory is one entry: {names:?}"
    );
    let all = sim::io::status(&r.sim.root).unwrap().0;
    assert!(all.windows(4).any(|w| w == b"!! t"), "the status carries them");
    assert!(all.windows(8).any(|w| w == b" M src/m"), "and keeps its own entries");
}
