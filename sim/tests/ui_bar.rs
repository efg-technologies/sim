//! UI-10: the status line of the interface: its sections and colors (pure), what it says about the
//! file, the mode, the branch and the changes, what takes it over, and what gives way when the
//! screen is narrow.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, apca, palette};
use sim::bar::{Bar, Block, render};
use sim::color::{Accent, Syntax, Tones};
use sim::layout::Skin;
use sim::sim::SimEvent;
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
        path: "src/layout.rs",
        modified: true,
        notes: "2 changes on disk",
        slot: Some(("rust-analyzer · sandboxed", false)),
        encoding: Some("utf-8"),
        eol: Some("LF"),
        ftype: Some("rust"),
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
        "utf-8",
        "LF",
        "rust",
        "42%",
        " 12:3 ",
    ] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    // The order of lualine's: a mode, b branch and changes, c file ... x, y progress, z position.
    let at = |s: &str| text.find(s).unwrap();
    assert!(at("NORMAL") < at("main") && at("main") < at("+18") && at("+18") < at("src/layout.rs"));
    let rust = text.rfind("rust").unwrap(); // (the file type: the slot has the word too)
    assert!(
        at("utf-8") < at("LF") && at("LF") < rust && rust < at("42%") && at("42%") < at("12:3")
    );
    assert!(at("src/layout.rs") < at("utf-8"));
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
    let words = ["rust-analyzer", "LF", "utf-8", "rust", "42%", "+18", "main", "changes on disk"];
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
fn ui_10_the_bar_says_the_mode_the_branch_the_changes_the_file_and_the_position() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = rig("ui10-bar", skin, (120, 24), true);
        let text = last(&r);
        for want in
            ["NORMAL", "main", "+1", "~1", "-1", "a.rs", "utf-8", "LF", "rust", "Top", "1:1"]
        {
            assert!(text.contains(want), "{skin:?}: {want:?} in {text:?}");
        }
        assert!(!text.contains('●'), "{skin:?}: not modified");
        r.keys("3jlix");
        let text = last(&r);
        for want in ["INSERT", "●", "4:3", "30%"] {
            assert!(text.contains(want), "{skin:?}: {want:?} in {text:?}");
        }
        r.keys("<Esc>G");
        let text = last(&r);
        assert!(text.contains("NORMAL") && text.contains("Bot"), "{skin:?}: {text}");
        r.keys("v");
        assert!(last(&r).contains("VISUAL"), "{skin:?}");
        // The mode block is the accent's in any mode but Normal, where it is neutral.
        let t = Tones::new(&palette(BG));
        let at = r.shot.paints.last().unwrap().iter().find(|(c, _)| c.contains(&3)).unwrap().1.bg;
        assert_eq!(at, Some(t.accent_block().0), "{skin:?}: Visual mode's block is the accent's");
    }
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
    assert!(
        text.contains('●') && text.contains("1 change on disk") && !text.contains("modified"),
        "{text}"
    );
    r.keys(":e b.rs\n");
    let text = last(&r);
    assert!(!text.contains("2/2"), "the tab bar says where you are, the bar does not: {text}");
    assert!(text.contains("b.rs"), "{text}");
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

#[test]
fn ui_10_a_crlf_file_says_so_and_a_file_of_no_known_type_says_no_type() {
    let mut r = rig_env("ui10-crlf", &["a.zzz=one\r\ntwo\r\n"], &["a.zzz"], (120, 24), |e| {
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    let text = last(&r);
    assert!(text.contains("CRLF") && !text.contains(" LF"), "{text}");
    assert!(text.contains("utf-8") && text.contains("a.zzz"), "{text}");
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
