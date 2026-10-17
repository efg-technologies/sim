//! CLI-5: `sim --plain` without a file shows an intro until the first key, drawn by the view and never
//! part of the buffer.
mod common;
use common::*;
use sim_core::core::app::{App, Event};
use sim_core::core::view::Style;
use unicode_width::UnicodeWidthStr;

fn bare(size: (u16, u16)) -> App {
    app_with(None, b"", size)
}

#[test]
fn cli_5_without_a_file_the_intro_shows_until_the_first_key() {
    let mut a = bare((80, 24));
    let f = screen(&mut a);
    // The golden holds the version of Cargo.toml: a new version blesses it again (SIM_BLESS=1).
    golden("cli_5_intro", &dump(&f));
    let shown: String = f.rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");
    assert!(shown.contains(&format!("sim {}", env!("CARGO_PKG_VERSION"))));
    let concept = include_str!("../../CONCEPT.md");
    let tagline = "The next-generation Vim that shows even what is hidden.";
    assert!(concept.contains(tagline) && shown.contains(tagline), "one line from CONCEPT");
    for key in ["i / Esc", "Ctrl-s", "ZZ", "ZQ", "]c / [c", "type", "save", "quit", "change"] {
        assert!(shown.contains(key), "{key}");
    }
    assert!(shown.lines().filter(|l| l.contains("quit")).count() >= 2 && shown.contains(" q "));
    // It is not text: the buffer is empty and unmodified, and the cursor is where typing starts.
    assert_eq!((text(&a).as_str(), a.buf.dirty(), f.cursor), ("", false, (5, 0)));
    assert_eq!(status(&f), "[no name]  1:1  NORMAL");
    // The first key removes it, whatever the key is.
    feed(&mut a, "l");
    let f = screen(&mut a);
    assert!(f.rows[1..22].iter().all(|r| r.text.is_empty()), "{:?}", f.rows);
    // And it does not come back.
    feed(&mut a, "dd");
    assert!(screen(&mut a).rows[1..22].iter().all(|r| r.text.is_empty()));
}

#[test]
fn cli_5_the_intro_is_plain_text_in_primary_and_secondary_and_at_most_12_rows() {
    for size in [(80, 24), (120, 50), (40, 24), (200, 100)] {
        let f = screen(&mut bare(size));
        let used: Vec<_> =
            f.rows[1..f.rows.len() - 1].iter().filter(|r| !r.text.is_empty()).collect();
        assert!(!used.is_empty() && used.len() <= 12, "{size:?}: {} rows", used.len());
        for r in &used {
            assert!(
                r.spans.iter().all(|s| s.1 == Style::Secondary) && r.underline.is_empty(),
                "{r:?}"
            );
            assert!(r.text.is_ascii() && r.text.width() <= usize::from(size.0));
        }
        // Centered: the same space on both sides of the widest row, give or take one column.
        let widest = used.iter().map(|r| r.text.len()).max().unwrap();
        let left = used.iter().map(|r| r.text.len() - r.text.trim_start().len()).min().unwrap();
        let right = usize::from(size.0) - widest;
        assert!(left.abs_diff(right) <= 1, "{size:?}: {left} {right}");
    }
}

#[test]
fn cli_5_short_and_narrow_screens_drop_the_lines_that_do_not_fit() {
    let lines = |f: &sim_core::core::view::Frame| -> Vec<String> {
        let inner = &f.rows[1..f.rows.len() - 1];
        inner.iter().map(|r| r.text.trim().to_string()).filter(|t| !t.is_empty()).collect()
    };
    let all = lines(&screen(&mut bare((80, 24))));
    assert_eq!(all.len(), 8, "the title, the tagline and six keys (the blank line is not text)");
    for h in 2..=24u16 {
        let f = screen(&mut bare((80, h)));
        assert_eq!(f.rows.len(), usize::from(h));
        let shown = lines(&f);
        assert!(shown.len() <= usize::from(h) - 2, "{h}: {shown:?}");
        assert_eq!(shown, all[..shown.len()], "{h}: the first lines stay, in order");
        if h >= 12 {
            assert_eq!(shown.len(), all.len(), "{h}: everything fits");
        }
    }
    for w in 1..=80u16 {
        let f = screen(&mut bare((w, 24)));
        assert!(f.rows.iter().all(|r| r.text.width() <= usize::from(w)), "{w}");
    }
}

#[test]
fn cli_5_no_intro_with_a_file_or_text_in_the_buffer() {
    let intro = |a: &mut App| screen(a).rows.iter().any(|r| r.text.contains("type / stop typing"));
    assert!(!intro(&mut app_with(Some("f.txt"), b"", (80, 24))), "a file that is empty");
    assert!(!intro(&mut app_with(Some("new.txt"), b"x\n", (80, 24))));
    assert!(!intro(&mut app_with(None, b"hello\n", (80, 24))), "text in an unnamed buffer");
    // Text that comes in later (a recovered snapshot) takes it away.
    let mut a = bare((80, 24));
    assert!(intro(&mut a));
    a.update(Event::Recover(b"".to_vec(), b"recovered\n".to_vec(), 5000, "snap".into(), far()));
    assert!(text(&a).contains("recovered") && !intro(&mut a));
    // A paste is text too.
    let mut a = bare((80, 24));
    a.update(Event::Paste("pasted".into()));
    assert!(!intro(&mut a));
    // The window changing size does not remove it.
    let mut a = bare((80, 24));
    a.update(Event::Resize(100, 30));
    assert!(intro(&mut a));
    // Opening a file by `:e` is a key first.
    let mut a = bare((80, 24));
    feed(&mut a, ":e x\n");
    assert!(!intro(&mut a));
}
