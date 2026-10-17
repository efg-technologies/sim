//! TREE-1, TREE-2, TREE-4, TREE-5: sim's state driven by events, as the shell would drive it:
//! how it starts, the panel and its keys, and directories whose listing arrives later.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use proptest::prelude::*;
use sim::sim::start;
use std::path::Path;

const FILES: [&str; 5] = ["src/main.rs", "src/lib.rs", "docs/a.md", "README.md", "Cargo.toml"];
const TOP: [&str; 4] = ["▸ docs/", "▸ src/", "  Cargo.toml", "  README.md"];

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(ToString::to_string).collect()
}

#[test]
fn tree_1_without_arguments_the_tree_of_the_current_directory_is_open_and_focused() {
    let r = rig("t1-none", &FILES, &[]);
    assert!(r.sim.panel.open && r.sim.panel.focus && r.sim.bufs.is_empty());
    assert_eq!(r.panel(), TOP);
    assert_eq!(r.selected().as_deref(), Some("▸ docs/"));
    assert_eq!(r.sim.root, r.cwd);
}

#[test]
fn tree_1_a_directory_argument_is_the_root_and_a_file_argument_closes_the_panel() {
    let r = rig("t1-dir", &FILES, &["src"]);
    assert_eq!(r.sim.root, r.cwd.join("src"));
    assert!(r.sim.panel.open && r.sim.panel.focus && r.sim.bufs.is_empty());
    assert_eq!(r.panel(), ["  lib.rs", "  main.rs"]);

    let r = rig("t1-file", &["f.txt=one\ntwo\nthree\n", "g.txt"], &["f.txt"]);
    assert!(!r.sim.panel.open && !r.sim.panel.focus);
    assert_eq!(r.open_files(), ["f.txt"]);
    assert_eq!(r.sim.root, r.cwd, "the tree of the current directory, for Ctrl-n later");
    assert_eq!(r.editor(), ["   1 one", "   2 two", "   3 three"]);
    assert_eq!(r.status(), "f.txt  1:1  NORMAL");
}

#[test]
fn tree_1_file_line_col_and_a_plus_command_go_to_the_place() {
    let r = rig("t1-pos", &["f.txt=one\ntwo\nthree\n"], &["f.txt:2:3"]);
    assert_eq!(r.status(), "f.txt  2:3  NORMAL");
    let r = rig("t1-cmd", &["f.txt=one\ntwo\nthree\n"], &["+3", "f.txt"]);
    assert_eq!(r.status(), "f.txt  3:1  NORMAL");
    // A file that does not exist yet is a new file, as in si.
    let r = rig("t1-new", &["g.txt"], &["new.txt"]);
    assert_eq!(r.open_files(), ["new.txt"]);
    assert_eq!(r.text(), "");
}

#[test]
fn tree_1_usage_errors_are_none() {
    let cwd = Path::new("/");
    let go = |a: &[&str]| start(&args(a), cwd, |_| false, |_| false).is_some();
    assert!(go(&[]) && go(&["f"]) && go(&["+q", "f"]) && go(&["--", "-x"]));
    assert!(!go(&["-x"]) && !go(&["a", "b"]) && !go(&["+"]));
}

#[test]
fn tree_1_the_panel_is_a_third_of_the_screen_at_most_32_and_one_column_apart() {
    for (w, pw) in [(36, 12), (80, 26), (96, 32), (120, 32), (240, 32)] {
        let mut r = rig_sized("t1-width", &["f.txt"], &["f.txt"], (w, 12));
        r.keys("<C-n>");
        assert_eq!(r.sim.widths(), (pw, w - pw - 1), "at {w} columns");
        let row = &r.screen.rows[0];
        assert_eq!(row.text.chars().nth(usize::from(pw)), Some(' '), "the gap");
        assert_eq!(row.text.chars().nth(usize::from(pw) + 1), Some(' '), "the gutter of the text");
        assert_eq!(
            r.app().size,
            (w - pw - 1, 12),
            "the file is laid out at the width that is left"
        );
        r.keys("<C-n>");
        assert_eq!(r.sim.widths(), (0, w), "closed: the whole screen");
        assert_eq!(r.app().size, (w, 12));
    }
}

#[test]
fn tree_1_names_that_would_do_harm_in_a_terminal_are_never_sent_raw() {
    let bidi = "\u{61c}\u{200b}\u{200e}\u{200f}\u{202a}\u{202e}\u{2066}\u{2069}\u{feff}";
    let names = ["a\x1b[2Jb", "c\u{202e}d", "e\nf", "日本語.md", &format!("g{bidi}h")];
    let r = rig("t1-safe", &names, &[]);
    let all = r.screen.rows.iter().map(|r| r.text.as_str()).collect::<String>();
    assert!(!all.chars().any(|c| c.is_control() || bidi.contains(c)), "{all:?}");
    assert_eq!(r.panel().len(), 5);
    assert!(r.panel().contains(&"  日本語.md".to_string()));
}

#[test]
fn tree_1_whatever_a_name_holds_the_bytes_written_to_the_terminal_are_sims_own() {
    // Names with an escape sequence (clear the screen, set the title, a CSI), a line break, a C1
    // control, the bidirectional controls: through the panel, the search, the status line, a
    // file opened by name, an error that names a directory, and the narrowed tree.
    let names = [
        "esc\x1b[2Jx",
        "nl\nx",
        "osc\x1b]0;pwned\x07x",
        "c1\u{9b}31mx",
        "rlo\u{202e}txt",
        "del\x7fx",
        "zw\u{200b}x",
        "bom\u{feff}x",
        "csi\x1b[31m",
        "日本語\x1b",
        "cr\r\ny",
        "tab\tx",
        "dir\x1b[2J/f",
    ];
    let files: Vec<String> = names.iter().map(|n| format!("{n}.txt")).collect();
    let mut r =
        rig_sized("t1-bytes", &files.iter().map(String::as_str).collect::<Vec<_>>(), &[], (60, 12));
    let mut painter = sim::paint::Painter::new(Vec::<u8>::new());
    let mut check = |r: &Rig, what: &str| {
        painter.draw(r.shot.clone()).unwrap();
        let bytes = std::mem::take(&mut painter.out);
        assert_eq!(foreign(&bytes), Vec::<String>::new(), "after {what}");
    };
    check(&r, "the start");
    for step in [
        "j", "j", "j", "l", "jjjjj", "<CR>", "<C-n>", "/esc", "<CR>", "<C-p>nl", "<Down>", ":w\n",
        ":e esc\n", "<Esc>", "<C-n>.", "-", "<Esc>gg", "<Tab>",
    ] {
        r.keys(step);
        check(&r, step);
    }
    // A directory that cannot be read is named in the status line.
    r.keys("<C-n>");
    std::fs::remove_dir_all(r.cwd.join("dir\x1b[2J")).unwrap();
    r.keys("<C-n><C-n>");
    r.keys("kkkkkkkkkkkkkkkk<CR>");
    check(&r, "an error that names a directory");
    // The checker itself sees what it is for.
    assert!(
        !foreign(b"\x1b[2Jtext\x1b]0;x\x07").is_empty()
            && foreign(b"\x1b[2;3Hok\x1b[0m").is_empty()
    );
}

#[test]
fn tree_1_the_panel_is_as_wide_as_its_columns_even_with_wide_characters() {
    let r = rig_sized("t1-wide", &["日本語のとても長いファイル名.md", "x.rs"], &[], (60, 8));
    let width = |s: &str| unicode_width::UnicodeWidthStr::width(s);
    // The help starts right after the panel (20 columns) and the gap, whatever the panel holds.
    for row in &r.screen.rows[..5] {
        let (help, _) = row.spans.last().unwrap();
        assert_eq!(width(&row.text), usize::from(help.end), "{:?}", row.text);
        assert_eq!(help.start, 21);
    }
    assert_eq!(width(&r.screen.rows[5].text), 21, "a row with no help ends with the gap");
}

#[test]
fn tree_1_without_a_file_the_editor_shows_a_help_in_the_secondary_gray() {
    let r = rig_sized("t1-help", &FILES, &[], (60, 10));
    golden("tree_1_help", &dump(&r.screen));
    assert_eq!(r.status(), "docs/  TREE");
}

#[test]
fn tree_1_a_name_that_is_not_utf8_is_listed_but_never_opened_as_another_file() {
    use std::os::unix::ffi::OsStrExt;
    // (Some file systems, macOS's among them, refuse such a name: there is nothing to test.)
    let mut r = rig("t1-nonutf8", &["b.txt"], &[]);
    let name = std::ffi::OsStr::from_bytes(b"a\xff.txt");
    if std::fs::write(r.cwd.join(name), "mine\n").is_err() {
        return;
    }
    r.keys("<C-n><C-n>"); // (the panel is shut and opened again: the root is read again)
    assert_eq!(r.panel(), ["  a\u{fffd}.txt", "  b.txt"]);
    r.keys("k<CR>"); // (the cursor stayed on b.txt)
    // Every path in sim is text, so the name that was listed is not the file's: opening it would
    // be a new empty file that a save then creates under a name that is not the file's.
    assert_eq!(r.open_files(), ["a\u{fffd}.txt"]);
    assert!(r.app().buf.readonly.is_some(), "so it is read-only, and says why: {}", r.status());
    assert!(r.status().contains("not valid UTF-8"), "{}", r.status());
    r.keys("ix<Esc>:w\n");
    assert!(!r.cwd.join("a\u{fffd}.txt").exists(), "nothing is created under another name");
    assert_eq!(std::fs::read(r.cwd.join(name)).unwrap(), b"mine\n");
}

#[test]
fn tree_1_an_argument_that_is_not_utf8_is_a_usage_error_and_not_a_panic() {
    use std::os::unix::ffi::OsStrExt;
    let arg = std::ffi::OsStr::from_bytes(b"a\xff.txt");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sim")).arg(arg).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("not valid UTF-8") && !err.contains("panicked"), "{err}");
    assert_eq!(out.status.code(), Some(2), "a usage error, as for any argument sim cannot use");
}

/// A name of 60 chars, to build paths that are longer than the screen.
fn long(n: char) -> String {
    format!("{}{n}", "d".repeat(59))
}

#[test]
fn tree_1_a_long_path_is_cut_from_the_left_and_the_word_tree_stays() {
    let (a, b) = (long('1'), long('2'));
    let mut r = rig("t1-long", &[&format!("{a}/{b}/file.txt")], &[]);
    r.keys("ljl"); // the first directory, the second, open
    assert_eq!(r.status(), format!("…/{b}/  TREE"), "a directory");
    r.keys("j");
    assert_eq!(r.status(), format!("…/{b}/file.txt  TREE"));
    assert!(width(r.status()) <= 80, "{}", r.status());
    let narrow = rig_sized("t1-narrow", &[&format!("{a}/file.txt")], &[], (30, 6));
    assert_eq!(narrow.status(), format!("…{}/  TREE", &a[a.len() - 22..]), "cut inside a name");
    assert_eq!(width(narrow.status()), 30, "{}", narrow.status());
}

#[test]
fn tree_1_with_no_file_open_and_no_focus_the_root_is_cut_from_the_left() {
    let mut r = rig_sized("t1-root", &["a"], &[], (20, 6));
    r.keys("<Esc>");
    let root = r.sim.root.to_string_lossy().into_owned();
    let status = r.status().to_string();
    assert!(root.len() > 20 && status.starts_with('…') && width(&status) <= 20, "{status}");
    assert!(root.ends_with(&status['…'.len_utf8()..]), "{status} is the end of {root}");
}

#[test]
fn tree_1_the_status_line_is_not_cut_at_the_width_of_the_editor() {
    let mut r = rig_sized("t1-status", &["f.txt"], &["f.txt"], (40, 8));
    r.keys("<C-n><Esc>:nosuchcommand<CR>");
    assert_eq!(r.sim.widths(), (13, 26));
    assert_eq!(r.status(), "not an editor command: nosuchcommand");
    r.keys(":abcdefghijklmnopqrstuvwxyz0123456789");
    assert_eq!(r.status(), ":abcdefghijklmnopqrstuvwxyz0123456789");
    assert_eq!(r.screen.cursor, (37, 7), "the cursor of the command line is on the status row");
}

#[test]
fn tree_1_a_command_line_longer_than_the_screen_scrolls_at_the_width_of_the_screen() {
    // VI-12: the App renders its own, narrower frame at every update and scrolls the line to it;
    // the line shown across the whole screen must scroll as one on a screen of that width.
    let mut r = rig_sized("t1-scroll", &["f.txt"], &["f.txt"], (40, 8));
    r.keys("<C-n><Esc>");
    let line: String = ('a'..='z').cycle().take(60).collect();
    let width = |s: &str| unicode_width::UnicodeWidthStr::width(s);
    r.keys(":");
    for c in line.chars() {
        r.keys(&c.to_string());
        let (status, (x, y)) = (r.status().to_string(), r.screen.cursor);
        assert!(width(&status) <= 40 && y == 7 && x < 40, "{status:?} {x},{y}");
        assert!(width(&status) > 26 || width(&status) == status.chars().count(), "{status:?}");
    }
    let (shown, x) = (r.status().to_string(), r.screen.cursor.0);
    assert!(width(&shown) >= 39 && x >= 38, "the line fills the screen: {shown:?} cursor {x}");
    assert!(format!(":{line}").ends_with(&shown), "its tail: {shown:?}");
    r.keys("<Home>");
    assert_eq!((r.status().chars().next(), r.screen.cursor.0), (Some(':'), 1), "the start again");
}

#[test]
fn tree_2_ctrl_n_opens_and_focuses_the_panel_and_closes_it_when_it_has_focus() {
    let mut r = rig("t2-toggle", &["f.txt"], &["f.txt"]);
    assert!(!r.sim.panel.open);
    r.keys("<C-n>");
    assert!(r.sim.panel.open && r.sim.panel.focus);
    assert_eq!(r.panel(), ["• f.txt"], "opened: likely (MARK-2)");
    r.keys("<C-n>");
    assert!(!r.sim.panel.open && !r.sim.panel.focus);
    assert!(r.panel().is_empty());
}

#[test]
fn tree_2_esc_gives_the_focus_back_and_leaves_the_panel_open() {
    let mut r = rig("t2-esc", &["f.txt=one\n"], &["f.txt"]);
    r.keys("<C-n><Esc>");
    assert!(r.sim.panel.open && !r.sim.panel.focus);
    assert!(r.selected().is_none(), "no cursor row while the focus is in the text");
    r.keys("x"); // now the text gets the keys
    assert_eq!(r.text(), "ne");
    // Ctrl-n from an open panel that has no focus focuses it; it does not close it.
    r.keys("<C-n>");
    assert!(r.sim.panel.open && r.sim.panel.focus);
    r.keys("x"); // the panel gets the keys, and has no `x`
    assert_eq!(r.text(), "ne");
}

#[test]
fn tree_2_ctrl_n_works_in_every_mode_of_the_text() {
    let mut r = rig("t2-modes", &["f.txt=one\n"], &["f.txt"]);
    r.keys("ixy<C-n>");
    assert!(r.sim.panel.focus, "from Insert mode");
    r.keys("<Esc>");
    r.keys("xyz"); // back in the text, still in Insert mode
    assert_eq!(r.text(), "xyxyzone");
    r.keys("<Esc>:w<C-n><C-n>");
    assert!(!r.sim.panel.open, "from the command line, and closed again");
}

#[test]
fn tree_2_opening_the_panel_reads_every_open_directory_again() {
    let mut r = rig("t2-refresh", &FILES, &[]);
    r.keys("jl"); // src
    assert_eq!(
        r.panel(),
        ["▸ docs/", "▾ src/", "    lib.rs", "    main.rs", "  Cargo.toml", "  README.md"]
    );
    std::fs::write(r.cwd.join("src/new.rs"), "").unwrap();
    std::fs::write(r.cwd.join("zzz.md"), "").unwrap();
    r.keys("<Esc><C-n>");
    assert_eq!(
        r.panel(),
        [
            "▸ docs/",
            "▾ src/",
            "    lib.rs",
            "    main.rs",
            "    new.rs",
            "  Cargo.toml",
            "  README.md",
            "  zzz.md"
        ]
    );
    assert_eq!(r.selected().as_deref(), Some("▾ src/"), "the cursor stays on its entry");
}

#[test]
fn tree_4_j_k_and_the_arrows_move_and_stop_at_the_ends() {
    let mut r = rig("t4-move", &FILES, &[]);
    r.keys("k");
    assert_eq!(r.selected().as_deref(), Some("▸ docs/"));
    r.keys("jj");
    assert_eq!(r.selected().as_deref(), Some("  Cargo.toml"));
    r.keys("<Down><Down><Down><Down>");
    assert_eq!(r.selected().as_deref(), Some("  README.md"));
    r.keys("<Up>k");
    assert_eq!(r.selected().as_deref(), Some("▸ src/"));
}

#[test]
fn tree_4_l_right_and_enter_expand_a_directory_and_open_a_file() {
    for key in ["l", "<Right>", "<CR>"] {
        let mut r = rig("t4-expand", &["src/main.rs=fn main() {}\n", "a.txt"], &[]);
        r.keys(key);
        assert_eq!(r.panel(), ["▾ src/", "    main.rs", "  a.txt"], "{key}");
        r.keys("j");
        r.keys(key);
        assert_eq!(
            r.open_files(),
            ["src/main.rs"],
            "{key}: a file opens, shown relative to the cwd"
        );
        assert!(!r.sim.panel.focus && r.sim.panel.open, "{key}: the focus is in the text");
        assert_eq!(r.text(), "fn main() {}");
    }
}

#[test]
fn tree_4_h_and_left_collapse_a_directory_or_go_to_its_parent() {
    let mut r = rig("t4-back", &FILES, &[]);
    r.keys("jljj"); // src expanded, cursor on main.rs
    assert_eq!(r.selected().as_deref(), Some("    main.rs"));
    r.keys("h");
    assert_eq!(r.selected().as_deref(), Some("▾ src/"), "a file goes to its parent");
    r.keys("h");
    assert_eq!(r.panel(), TOP, "an open directory collapses");
    assert_eq!(r.selected().as_deref(), Some("▸ src/"));
    r.keys("<Left>");
    assert_eq!(r.selected().as_deref(), Some("▸ src/"), "at the top level there is no parent");
    r.keys("l<Down><Left>");
    assert_eq!(
        r.panel(),
        ["▸ docs/", "▾ src/", "    lib.rs", "    main.rs", "  Cargo.toml", "  README.md"]
    );
    assert_eq!(r.selected().as_deref(), Some("▾ src/"));
}

#[test]
fn tree_4_gg_g_ctrl_d_ctrl_u_pageup_and_pagedown_move_as_in_si() {
    let names: Vec<String> = (0..60).map(|i| format!("f{i:02}.txt")).collect();
    let files: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut r = rig_sized("t4-pages", &files, &[], (60, 11)); // 10 rows of panel
    let at = |r: &Rig| r.selected().unwrap().trim().to_string();
    r.keys("G");
    assert_eq!(at(&r), "f59.txt");
    r.keys("gg");
    assert_eq!(at(&r), "f00.txt");
    r.keys("g");
    r.keys("j");
    assert_eq!(
        at(&r),
        "f01.txt",
        "a lone g waits for a second one and is forgotten by the next key"
    );
    r.keys("<C-d>");
    assert_eq!(at(&r), "f06.txt", "half a page: 5 rows");
    r.keys("<PageDown>");
    assert_eq!(at(&r), "f16.txt", "a page: 10 rows");
    r.keys("<C-u>");
    assert_eq!(at(&r), "f11.txt");
    r.keys("<PageUp><PageUp><PageUp>");
    assert_eq!(at(&r), "f00.txt");
    r.keys("<PageDown><PageDown><PageDown><PageDown><PageDown><PageDown><PageDown>");
    assert_eq!(at(&r), "f59.txt");
    assert_eq!(r.panel().len(), 10, "the panel scrolls with the cursor");
    assert_eq!(r.panel().last().unwrap().trim(), "f59.txt");
}

#[test]
fn tree_4_dash_collapses_everything_and_keeps_the_cursor_on_its_top_level_entry() {
    let mut r = rig("t4-dash", &["a/b/c.txt", "a/d.txt", "z.txt"], &[]);
    r.keys("ljlj"); // a open, b open, the file in b under the cursor
    assert_eq!(r.panel(), ["▾ a/", "  ▾ b/", "      c.txt", "    d.txt", "  z.txt"]);
    r.keys("-");
    assert_eq!(r.panel(), ["▸ a/", "  z.txt"]);
    assert_eq!(r.selected().as_deref(), Some("▸ a/"));
    r.keys("l"); // what was loaded is kept: it opens at once, as it was left
    assert_eq!(r.panel(), ["▾ a/", "  ▸ b/", "    d.txt", "  z.txt"]);
}

#[test]
fn tree_4_q_closes_the_panel_and_gives_the_text_the_keys() {
    let mut r = rig("t4-q", &["f.txt=one\n"], &["f.txt"]);
    r.keys("<C-n>q");
    assert!(!r.sim.panel.open && !r.sim.panel.focus);
    assert_eq!(r.open_files(), ["f.txt"], "q closed the panel, not the file");
}

#[test]
fn tree_5_a_listing_that_arrives_later_moves_nothing_above_it() {
    let mut r = rig("t5-late", &FILES, &[]);
    r.defer = true;
    r.keys("jl");
    let before = r.panel();
    assert_eq!(
        before,
        ["▸ docs/", "▾ src/", "  Cargo.toml", "  README.md"],
        "nothing below src yet"
    );
    assert_eq!(r.waiting, [r.cwd.join("src")]);
    r.deliver();
    let after = r.panel();
    assert_eq!(
        after,
        ["▸ docs/", "▾ src/", "    lib.rs", "    main.rs", "  Cargo.toml", "  README.md"]
    );
    assert_eq!(after[..2], before[..2], "everything above and including src stayed");
    assert_eq!(r.selected().as_deref(), Some("▾ src/"));
}

#[test]
fn tree_5_the_cursor_stays_on_its_entry_when_rows_arrive_above_it() {
    let mut r = rig_sized("t5-cursor", &FILES, &[], (60, 5)); // 4 rows of panel
    r.defer = true;
    r.keys("jlj"); // src is open and empty for now; the cursor goes on to Cargo.toml
    assert_eq!(r.selected().as_deref(), Some("  Cargo.toml"));
    r.deliver();
    assert_eq!(r.selected().as_deref(), Some("  Cargo.toml"), "two rows came in above it");
    r.keys("k");
    assert_eq!(r.selected().as_deref(), Some("    main.rs"));
}

#[test]
fn tree_5_a_directory_that_cannot_be_read_says_why_and_stays_empty() {
    let mut r = rig("t5-fail", &["gone/x", "f"], &[]);
    std::fs::remove_dir_all(r.cwd.join("gone")).unwrap();
    r.keys("l");
    assert_eq!(r.panel(), ["▾ gone/", "  f"]);
    // The name is the panel's (below the root), so a long temp directory (macOS's is 50
    // characters) never cuts the reason off the status line.
    assert_eq!(r.status(), "cannot read gone: No such file or directory");
}

#[test]
fn tree_5_listings_come_back_in_any_order_and_for_unknown_directories_nothing_happens() {
    let mut r = rig("t5-order", &["a/x/y.txt", "b/z.txt"], &[]);
    r.defer = true;
    r.keys("jl"); // b
    r.keys("kl"); // a
    assert_eq!(r.waiting.len(), 2);
    r.waiting.reverse(); // b's listing first
    r.deliver();
    assert_eq!(r.panel(), ["▾ a/", "  ▸ x/", "▾ b/", "    z.txt"]);
    r.send(sim::sim::SimEvent::Listing(r.cwd.join("nowhere/deeper"), Ok(vec![])));
    r.send(sim::sim::SimEvent::Listing(std::path::PathBuf::from("/elsewhere"), Ok(vec![])));
    assert_eq!(r.panel().len(), 4);
}

#[test]
fn tree_1_a_new_size_lays_the_panel_and_the_file_out_again() {
    use sim::sim::SimEvent;
    use sim_core::core::app::Event;
    let mut r = rig_sized("t1-resize", &["f.txt=one\n"], &["f.txt"], (90, 12));
    r.keys("<C-n>");
    assert_eq!((r.sim.widths(), r.app().size), ((30, 59), (59, 12)));
    r.send(SimEvent::Term(Event::Resize(45, 8)));
    assert_eq!((r.sim.widths(), r.app().size), ((15, 29), (29, 8)));
    assert_eq!((r.screen.width, r.screen.height), (45, 8));
    r.send(SimEvent::Term(Event::Resize(9, 8)));
    assert_eq!(r.sim.widths(), (0, 9), "too narrow for a panel: the file has the screen");
}

#[test]
fn tree_2_a_paste_goes_to_the_text_and_is_ignored_by_the_panel() {
    use sim::sim::SimEvent;
    use sim_core::core::app::Event;
    let mut r = rig("t2-paste", &["f.txt=one\n"], &["f.txt"]);
    r.send(SimEvent::Term(Event::Paste("abc".into())));
    assert_eq!(r.text(), "abcone");
    r.keys("<C-n>");
    r.send(SimEvent::Term(Event::Paste("xyz".into())));
    assert_eq!(r.text(), "abcone", "the panel has the focus: nothing to paste into");
}

#[test]
fn tree_2_a_notice_from_a_thread_is_told_on_the_status_line_until_the_next_key() {
    use sim::sim::SimEvent;
    let mut r = rig("t2-notice", &["f.txt"], &[]);
    r.send(SimEvent::Notice("cannot write snapshots: disk full".into()));
    assert_eq!(r.status(), "cannot write snapshots: disk full");
    r.keys("j");
    assert_eq!(r.status(), "f.txt  TREE");
}

proptest! {
    #[test]
    fn tree_1_a_cut_text_fits_and_is_the_end_of_the_text(
        text in "[a-z/日本é\u{200d}\u{1f600}]{0,40}",
        max in 0..45usize,
    ) {
        let (cut, whole) = (sim::layout::tail(&text, max), width(&text));
        prop_assert!(width(&cut) <= max, "{cut:?} in {max}");
        if whole <= max {
            prop_assert_eq!(&cut, &text);
        } else if max > 0 {
            let rest = cut.strip_prefix('…').unwrap_or("");
            prop_assert!(cut.starts_with('…') && text.ends_with(rest), "{cut:?} of {text:?}");
        }
    }

    #[test]
    fn tree_1_a_path_made_to_fit_ends_as_the_path_does_and_the_state_words_stay(
        path in "[a-z/日本é\u{200d}\u{1f600}]{0,40}",
        max in 0..45usize,
    ) {
        // si's `shorten` keeps the last components (`…/dir/file`); a name wider than the room is cut.
        let short = sim::layout::short(&path, max);
        prop_assert!(width(&short) <= max, "{short:?} in {max}");
        let last = path.rsplit('/').find(|c| !c.is_empty()).unwrap_or("");
        if width(&path) > max && max > 3 + width(last) {
            prop_assert!(short.starts_with("…/") && short.ends_with(last), "{short:?} of {path:?}");
        }
        // The state words are never cut: `stated` keeps them whole while there is room for them.
        let line = sim::layout::stated(&path, "  TREE", max);
        prop_assert!(width(&line) <= max);
        if max >= 6 {
            prop_assert!(line.ends_with("  TREE"), "{line:?} in {max}");
        }
    }
}
