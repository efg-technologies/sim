//! UI-10: the status line of the interface: its sections and colors (pure), what it says about the
//! file, the mode, the branch and the changes, what takes it over, and what gives way when the
//! screen is narrow.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::bar::{Bar, Block, render};
use sim::color::{Accent, Syntax, Tones};
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, apca, palette};
use unicode_width::UnicodeWidthStr;

const BG: Rgb = (0x1e, 0x1e, 0x1e);
/// VIS-4's backgrounds.
const BGS: [Rgb; 8] = [
    (0x00, 0x00, 0x00),
    (0x1e, 0x1e, 0x1e),
    (0x28, 0x2c, 0x34),
    (0x00, 0x2b, 0x36),
    (0xff, 0xff, 0xff),
    (0xfa, 0xfa, 0xfa),
    (0xfd, 0xf6, 0xe3),
    (0xef, 0xf1, 0xf5),
];

fn bar(icons: bool) -> Bar<'static> {
    Bar {
        icons,
        mode: "NORMAL",
        block: Block::Accent,
        branch: Some("main"),
        counts: Some((18, 3, 2)),
        icon: Some('\u{e7a8}'),
        path: Some("src/layout.rs"),
        modified: true,
        notes: "2 changes on disk",
        slot: Some(("rust-analyzer · sandboxed", false)),
        encoding: Some("not utf-8"),
        eol: Some("CRLF"),
        bom: Some("BOM"),
        ftype: Some("python"),
        progress: Some("42%"),
        pos: "12:3",
    }
}

#[test]
fn ui_10_the_bar_has_lualine_s_sections_with_powerline_separators_when_icons_are_on() {
    let t = Tones::new(&palette(BG));
    let line = render(140, &bar(true), &t).unwrap();
    let text = &line.row.text;
    assert_eq!(UnicodeWidthStr::width(text.as_str()), 140, "one bar across the width");
    for want in [
        " NORMAL ",
        "\u{e0a0} main",
        "+18",
        "~3",
        "-2",
        "src/layout.rs",
        "●",
        "2 changes on disk",
        "rust-analyzer · sandboxed",
        "not utf-8",
        "CRLF",
        "BOM",
        "python",
        "42%",
        " 12:3 ",
    ] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    // The order of lualine's: a mode, b branch and changes, c file ... x, y progress, z position.
    let at = |s: &str| text.find(s).unwrap();
    assert!(at("NORMAL") < at("main") && at("main") < at("+18") && at("+18") < at("src/layout.rs"));
    assert!(
        at("not utf-8") < at("CRLF")
            && at("CRLF") < at("BOM")
            && at("BOM") < at("python")
            && at("python") < at("42%")
            && at("42%") < at("12:3")
    );
    assert!(at("src/layout.rs") < at("not utf-8"));
    // Separators: `` after each section on the left, `` before each on the right.
    assert_eq!(text.matches('\u{e0b0}').count(), 3, "{text:?}");
    assert_eq!(text.matches('\u{e0b2}').count(), 3, "{text:?}");
    // The mode block and the position are in the mode's color, the file's section on the bar.
    let bg = |col: u16| line.paints.iter().find(|(c, _)| c.contains(&col)).and_then(|p| p.1.bg);
    assert_eq!(bg(2), Some(t.block(Accent::Blue)));
    assert_eq!(bg(139 - 1), Some(t.block(Accent::Blue)));
    let col = |s: &str| text[..text.find(s).unwrap()].chars().count() as u16;
    assert_eq!(bg(col("src/layout.rs")), Some(t.bar), "the file's section is on the bar");
    assert_eq!(bg(col("main")), Some(t.card), "the branch's on the lighter surface");
    assert_eq!(bg(col("42%")), Some(t.card));
}

#[test]
fn ui_10_without_icons_there_are_no_separators_and_no_glyphs() {
    let t = Tones::new(&palette(BG));
    let line = render(120, &bar(false), &t).unwrap();
    let text = &line.row.text;
    assert!(!text.chars().any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c)), "{text:?}");
    assert_eq!(UnicodeWidthStr::width(text.as_str()), 120);
    assert!(text.contains("main") && text.contains("NORMAL") && text.contains("12:3"));
}

#[test]
fn ui_10_what_does_not_fit_is_dropped_from_the_right_least_important_first() {
    let t = Tones::new(&palette(BG));
    let words =
        ["rust-analyzer", "CRLF", "not utf-8", "python", "42%", "+18", "main", "changes on disk"];
    let mut gone = vec![];
    for w in (24..=130).rev() {
        let line = render(w, &bar(true), &t).unwrap();
        let text = &line.row.text;
        assert_eq!(UnicodeWidthStr::width(text.as_str()), usize::from(w), "{w}: {text:?}");
        for must in ["NORMAL", "12:3"] {
            assert!(text.contains(must), "{w}: {must} stays: {text:?}");
        }
        assert!(
            text.contains("layout.rs") || text.contains('…'),
            "{w}: the path stays, cut if need be: {text:?}"
        );
        let now: Vec<&str> = words.iter().copied().filter(|x| !text.contains(x)).collect();
        // Once something is dropped it stays dropped as the screen narrows, and in the order of the words.
        assert!(gone.iter().all(|g| now.contains(g)), "{w}: {now:?} after {gone:?}");
        let k = now.len();
        assert_eq!(now, words[..k], "{w}: dropped in order: {text:?}");
        assert!(
            text.contains("CRLF") || !text.contains("BOM"),
            "{w}: BOM goes with CRLF: {text:?}"
        );
        gone = now;
    }
    assert_eq!(
        gone.len(),
        words.len(),
        "at 24 columns only the mode, the file and the position are left"
    );
    assert!(render(23, &bar(true), &t).is_none(), "under 24 columns it is si's text");
}

#[test]
fn ui_12_the_text_of_the_bar_reads_on_its_sections_on_every_background() {
    let mut worst = f64::INFINITY;
    for bg in BGS {
        let t = Tones::new(&palette(bg));
        let line = render(120, &bar(true), &t).unwrap();
        for (range, p) in &line.paints {
            let (Some(fg), Some(back)) = (p.fg, p.bg) else { continue };
            let text: String = line
                .row
                .text
                .chars()
                .skip(usize::from(range.start))
                .take(usize::from(range.end - range.start))
                .collect();
            if text.trim().is_empty()
                || text.trim().chars().all(|c| ('\u{e000}'..='\u{f8ff}').contains(&c))
            {
                continue; // blanks, separators and icons are not text
            }
            let lc = apca(fg, back).abs();
            assert!(lc >= 75.0, "{bg:?}: {text:?} |Lc| {lc:.1} ({fg:?} on {back:?})");
            worst = worst.min(lc);
        }
    }
    eprintln!("UI-12 lowest |Lc| of the status line's text on its sections: {worst:.1}");
}

// ---- in sim's state ----

/// `a.rs` of ten lines committed on `main` and then changed (one line changed, one added, one deleted).
fn rig(name: &str, skin: Skin, size: (u16, u16), icons: bool) -> Rig {
    let text: String = (1..=10).map(|i| format!("l{i}\n")).collect();
    let mut r = rig_env(name, &[&format!("a.rs={text}"), "b.rs"], &["a.rs"], size, |e| {
        git(&e.cwd, &["init", "-q", "-b", "main"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        std::fs::write(e.cwd.join("a.rs"), "l1\nL2\nl3\nl4\nl5\nnew\nl6\nl7\nl9\nl10\n").unwrap();
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
        e.icons = icons;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

fn last(r: &Rig) -> String {
    r.shot.frame.rows.last().unwrap().text.clone()
}

#[test]
fn ui_10_the_bar_says_the_mode_the_branch_the_changes_and_the_position_and_no_fact_that_is_elsewhere()
 {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = rig("ui10-bar", skin, (120, 24), true);
        let text = last(&r);
        for want in ["NORMAL", "main", "+1", "~1", "-1", "1:1"] {
            assert!(text.contains(want), "{skin:?}: {want:?} in {text:?}");
        }
        // the file is in the tab and the breadcrumb, how a file is usually coded says nothing,
        // the type is in the name, and the scrollbar shows where the cursor is
        for gone in ["a.rs", "utf-8", "LF", "rust", "Top", "%", "●"] {
            assert!(!text.contains(gone), "{skin:?}: {gone:?} in {text:?}");
        }
        r.keys("3jlix");
        let text = last(&r);
        assert!(text.contains("INSERT") && text.contains("4:3"), "{skin:?}: {text:?}");
        assert!(!text.contains('●') && !text.contains('%'), "{skin:?}: {text:?}");
        let tab = r.shot.frame.rows.iter().any(|x| x.text.contains("a.rs") && x.text.contains('●'));
        assert!(tab, "{skin:?}: the tab says it is modified: {}", dump(&r.shot.frame));
        r.keys("<Esc>G");
        let text = last(&r);
        assert!(text.contains("NORMAL") && !text.contains("Bot"), "{skin:?}: {text}");
        r.keys("v");
        assert!(last(&r).contains("VISUAL"), "{skin:?}");
        // The mode block is the accent's in any mode but Normal, where it is neutral.
        let t = Tones::new(&palette(BG));
        let at = r.shot.paints.last().unwrap().iter().find(|(c, _)| c.contains(&3)).unwrap().1.bg;
        assert_eq!(at, Some(t.accent_block().0), "{skin:?}: Visual mode's block is the accent's");
    }
}

#[test]
fn ui_10_where_there_is_no_tab_bar_the_bar_keeps_the_file_and_where_there_is_no_scrollbar_the_progress()
 {
    // under 10 rows there is no header, so no tab bar: the bar names the file and says it is modified
    let mut r = rig("ui10-short", Skin::Flat, (100, 9), true);
    assert!(last(&r).contains("a.rs"), "{}", last(&r));
    r.keys("x");
    assert!(last(&r).contains("a.rs") && last(&r).contains('●'), "{}", last(&r));
    // a pane too narrow for the scrollbar (flat's column needs 26 columns of text): the bar has the progress
    let flat = |w: u16| {
        let mut r = rig_env("ui10-pane", &["a.rs=l1\nl2\n"], &["a.rs"], (w, 12), |e| {
            e.skin = Skin::Flat;
            e.syn = Syntax::new(true);
            e.chrome = true;
        });
        r.send(SimEvent::Term(Event::Bg(BG)));
        last(&r)
    };
    assert!(flat(26).contains("Top"), "{}", flat(26));
    assert!(!flat(60).contains("Top"), "the scrollbar says it: {}", flat(60));
}

#[test]
fn ui_10_calm_and_a_screen_too_narrow_keep_si_s_status_line() {
    let calm = rig("ui10-calm", Skin::Calm, (120, 24), true);
    assert_eq!(last(&calm), "a.rs  1:1  NORMAL");
    let narrow = rig("ui10-narrow", Skin::Flat, (23, 24), true);
    assert!(last(&narrow).contains("a.rs"), "{}", last(&narrow));
    assert!(!last(&narrow).contains("main"));
}

#[test]
fn ui_10_a_message_the_command_line_and_a_search_take_the_row_over() {
    let mut r = rig("ui10-over", Skin::Flat, (120, 24), true);
    r.keys("zi");
    assert_eq!(r.status(), "icons off", "a message");
    r.keys("j");
    assert!(
        r.status().contains("NORMAL") && r.status().contains("main"),
        "and the bar is back: {}",
        r.status()
    );
    r.keys(":");
    assert_eq!(r.status(), ":", "the command line");
    r.keys("<Esc>");
    assert!(r.status().contains("NORMAL"));
    r.keys("<C-n>/");
    assert!(r.status().starts_with('/'), "a search being typed: {}", r.status());
}

#[test]
fn ui_10_si_s_state_words_are_in_the_bar_and_a_second_file_is_in_the_tabs() {
    let mut r = rig("ui10-notes", Skin::Flat, (140, 24), true);
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"l1\nL2\nl3\nl4\nl5\nnew\nl6\nl7\nl9\nl10\nl11\n")));
    let text = last(&r);
    assert!(text.contains("1 change on disk"), "{text}");
    r.keys("x");
    let text = last(&r);
    assert!(text.contains("1 change on disk") && !text.contains("modified"), "{text}");
    r.keys(":e b.rs\n");
    let text = last(&r);
    assert!(!text.contains("2/2"), "the tab bar says where you are, the bar does not: {text}");
    assert!(!text.contains("b.rs"), "nor which file: the tab does: {text}");
    assert!(r.shot.frame.rows[0].text.contains("b.rs"), "{}", r.shot.frame.rows[0].text);
}

#[test]
fn ui_10_the_panel_s_focus_has_a_tree_block_the_branch_and_the_selected_path() {
    let mut r = rig("ui10-tree", Skin::Flat, (120, 24), true);
    r.keys("<C-n>");
    let text = last(&r);
    for want in ["TREE", "main", "a.rs"] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    assert!(!text.contains("NORMAL"));
}

#[test]
fn ui_10_the_security_slot_shows_the_servers_state_and_the_trust_of_the_root_and_nothing_else() {
    let mut r = rig("ui10-slot", Skin::Flat, (140, 24), true);
    assert!(
        !last(&r).contains("trusted") && !last(&r).contains("sandboxed"),
        "nobody has said anything: {}",
        last(&r)
    );
    r.sim.trust = Some(false);
    r.keys("j");
    assert!(last(&r).contains("untrusted"), "{}", last(&r));
    r.sim.trust = Some(true);
    r.keys("j");
    assert!(last(&r).contains(" trusted"), "{}", last(&r));
    r.sim.bufs[0].lsp = Some(" rust-analyzer · sandboxed".into());
    r.keys("j");
    assert!(last(&r).contains("rust-analyzer · sandboxed"), "{}", last(&r));
}

/// The bar of a file `name` with `bytes` in it, open in flat, 120 columns wide.
fn bar_of(name: &str, bytes: &[u8]) -> String {
    let mut r = rig_env("ui10-file", &[name], &[name], (120, 24), |e| {
        std::fs::write(e.cwd.join(name), bytes).unwrap();
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    last(&r)
}

#[test]
fn ui_10_what_is_not_as_usual_is_said_at_the_right_and_nothing_else() {
    let plain = bar_of("a.zzz", b"one\ntwo\n");
    for gone in ["utf-8", "LF", "CRLF", "BOM", "a.zzz"] {
        assert!(!plain.contains(gone), "{gone}: {plain}");
    }
    let crlf = bar_of("a.zzz", b"one\r\ntwo\r\n");
    assert!(crlf.contains("CRLF") && !crlf.contains(" LF") && !crlf.contains("utf-8"), "{crlf}");
    let bom = bar_of("a.zzz", b"\xEF\xBB\xBFone\ntwo\n");
    assert!(bom.contains("BOM") && !bom.contains("CRLF"), "{bom}");
    let both = bar_of("a.zzz", b"\xEF\xBB\xBFone\r\ntwo\r\n");
    assert!(both.contains("CRLF") && both.contains("BOM"), "{both}");
    // the core's own words for them are not said twice
    assert_eq!(both.matches("CRLF").count(), 1, "{both}");
    assert_eq!(both.matches("BOM").count(), 1, "{both}");
    // a file that is not UTF-8: the middle says the state (read-only), the right the format (not
    // utf-8), and neither says the other's
    let latin = bar_of("a.zzz", b"caf\xe9\n");
    assert!(latin.contains("not utf-8") && latin.contains("read-only"), "{latin}");
    assert!(!latin.contains("not valid UTF-8") && !latin.contains("read-only:"), "{latin}");
    assert_eq!(latin.matches("read-only").count(), 1, "{latin}");
    assert_eq!(
        latin.to_lowercase().matches("utf").count(),
        1,
        "the encoding is said once: {latin}"
    );
    // no word of the middle is said again at the right
    for (name, bytes, word) in [
        ("a.zzz", &b"one\r\n"[..], "CRLF"),
        ("a.zzz", b"\xEF\xBB\xBFone\n", "BOM"),
        ("run", b"#!/bin/sh\necho\n", "sh"),
    ] {
        let bar = bar_of(name, bytes);
        assert_eq!(bar.matches(word).count(), 1, "{word} is said once: {bar}");
    }
    // a binary file says why it is read-only and nothing about an encoding
    let binary = bar_of("a.zzz", b"a\0b\n");
    assert!(binary.contains("read-only: binary file") && !binary.contains("not utf-8"), "{binary}");
}

#[test]
fn ui_10_the_file_type_is_shown_only_for_a_file_whose_name_does_not_tell_it() {
    // a script with no extension: its first line says what it is
    let sh = bar_of("run", b"#!/bin/sh\necho hi\n");
    assert!(sh.contains(" sh"), "{sh}");
    let py = bar_of("tool", b"#!/usr/bin/env python3\nprint(1)\n");
    assert!(py.contains("python"), "{py}");
    // a name that tells it, even over a first line that says another thing, says nothing
    let rs = bar_of("a.rs", b"#!/bin/sh\nfn a() {}\n");
    assert!(!rs.contains("rust") && !rs.contains(" sh"), "{rs}");
    let none = bar_of("notes", b"just words\n");
    assert!(!none.contains("text") && !none.contains("notes"), "{none}");
}

#[test]
fn ui_10_a_shebang_is_read_by_the_interpreter_and_not_by_its_path_or_version() {
    use sim::icons::shebang;
    for (line, ft) in [
        ("#!/bin/sh", "sh"),
        ("#!/usr/bin/env bash", "bash"),
        ("#!/usr/bin/env -S python3 -u", "python"),
        ("#!/usr/bin/python3.11", "python"),
        ("#! /usr/bin/node", "javascript"),
        ("#!/usr/bin/env ruby", "ruby"),
    ] {
        assert_eq!(shebang(line), Some(ft), "{line}");
    }
    for line in ["", "#!", "# !/bin/sh", "#!/usr/bin/env", "#!/opt/weird/thing", "fn a() {}"] {
        assert_eq!(shebang(line), None, "{line:?}");
    }
}

#[test]
fn ui_10_the_file_type_is_what_neovim_calls_it_and_chezmoi_s_names_are_read_as_what_they_stand_for()
{
    use sim::icons::filetype;
    for (name, ft) in [
        ("a.rs", "rust"),
        ("README.md", "markdown"),
        ("src/x.tsx", "typescriptreact"),
        ("Dockerfile", "dockerfile"),
        ("Makefile", "make"),
        ("dot_zshrc", "zsh"),
        ("dot_gitconfig.tmpl", "gitconfig"),
        ("dot_tmux.conf.tmpl", "conf"),
        ("private_dot_config/nvim/init.lua.tmpl", "lua"),
        ("run_onchange_setup.sh", "sh"),
    ] {
        assert_eq!(filetype(name), Some(ft), "{name}");
    }
    assert_eq!(filetype("a.zzz"), None);
    assert_eq!(filetype("noextension"), None);
}

#[test]
fn ui_10_the_core_s_own_line_keeps_its_whole_reason_where_there_is_no_right_side() {
    // calm has no bar: the core's words, the reason of the read-only included
    let mut r = rig_env("ui10-calm-latin", &["a.zzz"], &["a.zzz"], (100, 12), |e| {
        std::fs::write(e.cwd.join("a.zzz"), b"caf\xe9\n").unwrap();
        e.skin = Skin::Calm;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    assert!(last(&r).contains("read-only: not valid UTF-8"), "{}", last(&r));
    let plain = rig_env("ui10-plain-latin", &["a.zzz"], &["--plain", "a.zzz"], (100, 12), |e| {
        std::fs::write(e.cwd.join("a.zzz"), b"caf\xe9\n").unwrap();
    });
    assert!(plain.status().contains("read-only: not valid UTF-8"), "{}", plain.status());
}
