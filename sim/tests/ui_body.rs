//! UI-9, UI-5: the body of the editor in flat and depth: wrapping at the pane's width, the line
//! numbers, the cursor's line, the sign column, driven through sim's state.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::layout::Skin;

const LINE: &str = "word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word.";

#[test]
fn ui_9_prose_wraps_at_the_panes_width_in_every_look_and_a_narrow_pane_wraps_at_its_own() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        for name in ["a.md", "a.txt", "README"] {
            let file = format!("{name}={LINE}\n");
            let wide = rig_env("ui9-wide", &[&file], &[name], (200, 12), |e| e.skin = skin);
            let texts = |r: &Rig| -> Vec<String> {
                let rows = r.screen.rows.iter().map(|x| x.text.clone());
                rows.filter(|t| t.contains("word")).collect()
            };
            // The line is 190 columns: one row in a pane of 190 or more, not two at 72.
            let one = texts(&wide);
            assert!(one.len() == 1 && one[0].contains(LINE), "{skin:?} {name}: {one:?}");
            let narrow = rig_env("ui9-narrow", &[&file], &[name], (60, 12), |e| e.skin = skin);
            let rows = texts(&narrow);
            assert!(rows.len() >= 4, "{skin:?} {name}: wraps at 60: {rows:?}");
            assert!(
                rows.iter().all(|r| unicode_width::UnicodeWidthStr::width(r.as_str()) <= 60),
                "{skin:?} {name}"
            );
        }
    }
}

#[test]
fn ui_9_si_keeps_its_72_columns() {
    use si::core::app::App;
    use si::core::view::render;
    let file = format!("{LINE}\n").into_bytes();
    let mut app = App::new(Some("a.md".into()), Ok(Some(file)), (200, 12));
    assert!(!app.wide, "si's default");
    let rows = render(&mut app).rows;
    assert!(rows[0].text.chars().count() <= 5 + 72, "{}", rows[0].text);
    app.wide = true;
    let rows = render(&mut app).rows;
    assert!(rows[0].text.contains(LINE), "wide: {}", rows[0].text);
}
