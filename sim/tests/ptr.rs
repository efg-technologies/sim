//! PTR-1, PTR-2: the mouse through sim's state: events in, the cursor, the focus, the scroll and the
//! selection out. The mouse is at (column, row) of the screen, 0-based; with the panel closed the
//! gutter is 5 columns wide (a symbol, three digits, a blank), so the text starts at column 5.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use crossterm::event::{MouseButton as B, MouseEventKind as K};
use si::core::app::Effect;
use si::core::vi::Mode;
use sim::paint::{MOUSE_OFF, MOUSE_ON};
use sim::sim::{SimEffect, SimEvent};

/// The column of the text.
const T: u16 = 5;

fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}\n")).collect()
}

/// The first line number shown on the screen, and the cursor's (both 1-based).
fn view(r: &Rig) -> (usize, usize) {
    let (pw, _) = r.sim.widths();
    let off = if pw > 0 { usize::from(pw) + 1 } else { 0 };
    let first = r.screen.rows[0].text.chars().skip(off + 1).take(3).collect::<String>();
    let line = r.status().split("  ").nth(1).unwrap().split(':').next().unwrap().to_string();
    (first.trim().parse().unwrap(), line.parse().unwrap())
}

fn cursor(r: &Rig) -> usize {
    r.app().buf.cursor
}

// ---- the panel ----

#[test]
fn ptr_1_a_click_in_the_panel_moves_the_panel_cursor_there() {
    let mut r = rig("ptr-panel", &["a/x.rs", "b.txt", "c.txt"], &["b.txt"]);
    r.keys("<C-n><Esc>"); // the panel is open, the text has the focus
    assert!(r.sim.panel.open && !r.sim.panel.focus);
    assert_eq!(r.panel(), ["▸ a/", "• b.txt", "  c.txt"], "b.txt is open: the likeliest");
    r.click(3, 0); // a directory: the cursor goes there, the panel has the focus
    assert!(r.sim.panel.focus);
    assert_eq!(r.sim.panel.cur, 0);
    assert_eq!(r.selected().as_deref(), Some("▾ a/"));
    // the cursor follows a click on any row, with the focus
    r.keys("<Esc>");
    assert!(!r.sim.panel.focus);
    r.click(3, 1);
    assert_eq!(r.sim.panel.cur, 1);
}

#[test]
fn ptr_1_a_click_on_a_file_opens_it_and_a_click_on_a_directory_toggles_it() {
    let mut r = rig("ptr-open", &["a/x.rs", "b.txt", "c.txt"], &[]);
    assert_eq!(r.panel(), ["▸ a/", "  b.txt", "  c.txt"]);
    r.click(2, 0);
    assert_eq!(r.panel(), ["▾ a/", "    x.rs", "  b.txt", "  c.txt"], "opened");
    assert!(r.sim.panel.focus && r.sim.bufs.is_empty(), "a directory opens no file");
    r.click(2, 0);
    assert_eq!(r.panel(), ["▸ a/", "  b.txt", "  c.txt"], "and closed again");
    r.click(2, 0);
    r.click(6, 1); // a/x.rs
    assert_eq!(r.open_files(), ["a/x.rs"]);
    assert!(!r.sim.panel.focus, "the focus is in the text now");
    assert_eq!(r.sim.panel.cur, 1, "the panel's cursor is where the click was");
    r.click(6, 3); // c.txt
    assert_eq!(r.open_files(), ["a/x.rs", "c.txt"]);
    r.click(6, 3); // the same file again: it is shown, not opened twice
    assert_eq!((r.open_files().len(), r.sim.cur), (2, 1));
    r.click(6, 1);
    assert_eq!(r.sim.cur, 0);
}

#[test]
fn ptr_1_a_click_outside_the_rows_of_the_panel_does_nothing() {
    let mut r = rig("ptr-none", &["a.txt", "b.txt"], &[]);
    let before = r.screen.clone();
    r.click(2, 5); // below the last row
    r.click(26, 0); // the blank column between the panel and the text
    r.click(2, 23); // the status line
    assert_eq!(r.screen, before);
    assert!(r.sim.bufs.is_empty());
}

#[test]
fn ptr_1_a_click_during_a_search_ends_the_typing_and_opens_what_it_hits() {
    let mut r = rig("ptr-search", &["src/main.rs", "src/lib.rs", "README.md"], &[]);
    r.keys("/ma");
    assert!(r.status().starts_with("/ma"));
    assert_eq!(r.panel(), ["▾ src/", "    lib.rs", "    main.rs", "  README.md"]);
    r.click(8, 1); // lib.rs, which is not the best match
    assert_eq!(r.open_files(), ["src/lib.rs"]);
    assert!(r.status().starts_with("src/lib.rs  1:1"), "the search is over: {}", r.status());
}

// ---- the text ----

#[test]
fn ptr_1_a_click_in_the_text_moves_the_cursor_to_that_char() {
    let mut r = rig("ptr-text", &["f.txt=alpha beta\ngamma\n\nδ\n"], &["f.txt"]);
    r.click(T + 6, 0);
    assert_eq!(r.status(), "f.txt  1:7  NORMAL");
    r.click(T + 1, 1);
    assert_eq!(r.status(), "f.txt  2:2  NORMAL");
    r.click(T + 3, 2); // an empty line
    assert_eq!(r.status(), "f.txt  3:1  NORMAL");
    r.click(0, 3); // the gutter: the start of the line
    assert_eq!(r.status(), "f.txt  4:1  NORMAL");
    // the wanted column starts again from here (j keeps the column of the click)
    r.click(T + 4, 0);
    r.keys("j");
    assert_eq!(r.status(), "f.txt  2:5  NORMAL");
}

#[test]
fn ptr_1_a_click_gives_the_focus_to_the_text() {
    let mut r = rig("ptr-focus", &["a.txt=abc\n", "b.txt"], &[]);
    r.keys("<CR>"); // a.txt is open; the focus moved to the text
    r.keys("<C-n>");
    assert!(r.sim.panel.focus);
    let off = r.sim.widths().0 + 1;
    r.click(off + T + 2, 0);
    assert!(
        r.sim.panel.open && !r.sim.panel.focus,
        "the panel stays open, the focus is in the text"
    );
    assert_eq!(r.status(), "a.txt  1:3  NORMAL");
    r.keys("h");
    assert_eq!(r.status(), "a.txt  1:2  NORMAL", "keys go to the text");
}

#[test]
fn ptr_1_a_click_lands_on_the_char_shown_under_the_pointer() {
    // tabs, wide chars, a control char and a bidirectional control (written as tokens, SAFE-8),
    // an emoji sequence, and a line that wraps: every cell of the text maps to the char drawn there
    let text = "\tab\n日本語x\n\u{1}z\u{202e}y\n\u{2764}\u{fe0f}q\n\
                aaaa bbbb cccc dddd eeee ffff gggg hhhh iiii\nlast\n";
    let mut r = rig_sized("ptr-map", &[&format!("f.txt={text}")], &["f.txt"], (30, 14));
    let rows = r.screen.rows.clone();
    let buf = r.app().buf.text.clone();
    for (y, row) in rows.iter().enumerate().take(13) {
        let (mut col, cs) = (0u16, row.text.chars().collect::<Vec<_>>());
        for (i, &drawn) in cs.iter().enumerate() {
            let w = si::core::view::width(&cs, i, usize::from(col)) as u16;
            if col >= T && w > 0 {
                // the char drawn under the pointer is the char under the cursor, but for what is
                // drawn for another: blanks for a tab, `^A` or `<U+202E>` for a control char
                r.click(col, y as u16);
                let under = buf.char(cursor(&r));
                let token = under.is_control() || under == '\u{202e}';
                let ok = under == drawn
                    || under == '\t' && drawn == ' '
                    || token && "^<U+0123456789ABCDEF>".contains(drawn);
                assert!(ok, "cell {col},{y} shows {drawn:?}, the cursor is on {under:?}");
            }
            col += w;
        }
    }
    // explicit cells: the second column of a wide char and of a tab are that char
    r.click(T + 3, 1); // 本 is columns 2-3
    assert_eq!(r.status(), "f.txt  2:2  NORMAL");
    r.click(T + 2, 0); // inside the tab (columns 0-3)
    assert_eq!(r.status(), "f.txt  1:1  NORMAL");
    r.click(T + 4, 0); // `a` after the tab
    assert_eq!(r.status(), "f.txt  1:2  NORMAL");
    r.click(T + 1, 2); // the second column of `^A`
    assert_eq!(r.status(), "f.txt  3:1  NORMAL");
    r.click(T + 2, 2); // z
    assert_eq!(r.status(), "f.txt  3:2  NORMAL");
    r.click(T + 5, 2); // inside `<U+202E>`
    assert_eq!(r.status(), "f.txt  3:3  NORMAL");
    r.click(T + 5, 4);
    assert_eq!(r.app().buf.ch(cursor(&r)), 'b', "{:?}", r.screen.rows[4].text);
    r.click(T + 5, 5); // the second row of the wrapped line: no number, and the line goes on
    assert_eq!(r.app().buf.ch(cursor(&r)), 'g', "{:?}", r.screen.rows[5].text);
    assert_eq!(r.status(), "f.txt  5:31  NORMAL");
}

#[test]
fn ptr_1_a_click_past_the_end_of_a_line_goes_to_its_last_char_and_below_the_text_to_the_last_line()
{
    let mut r = rig("ptr-past", &["f.txt=short\nlonger line\n"], &["f.txt"]);
    r.click(T + 30, 0);
    assert_eq!(r.status(), "f.txt  1:5  NORMAL", "the last char of the line, not after it");
    r.click(40, 10); // the blank rows under the text
    assert_eq!(r.status(), "f.txt  2:11  NORMAL");
    // a wrapped line: past the end of a row that is not the last is that row's last char
    let mut r = rig_sized("ptr-past2", &["f.txt=aaaa bbbb cccc dddd eeee\n"], &["f.txt"], (16, 8));
    r.click(15, 0);
    assert_eq!(r.app().buf.ch(cursor(&r)), ' ', "{:?}", r.screen.rows[0].text);
}

#[test]
fn ptr_1_a_click_in_insert_mode_moves_the_cursor_and_the_mode_stays() {
    let mut r = rig("ptr-ins", &["f.txt=alpha beta\n"], &["f.txt"]);
    r.keys("i");
    r.click(T + 2, 0);
    assert_eq!(r.status(), "f.txt  1:3  INSERT");
    r.keys("X");
    assert_eq!(r.text(), "alXpha beta");
    r.click(T + 30, 0);
    assert_eq!(
        r.status(),
        "f.txt  1:12  INSERT  modified",
        "after the last char: Insert mode may be there"
    );
}

#[test]
fn ptr_1_the_panel_beside_the_text_moves_the_columns_of_a_click() {
    let mut r = rig("ptr-off", &["f.txt=alpha beta\n"], &["f.txt"]);
    r.keys("<C-n><Esc>");
    let (pw, _) = r.sim.widths();
    let off = pw + 1;
    r.click(off + T + 6, 0);
    assert_eq!(r.status(), "f.txt  1:7  NORMAL");
}

#[test]
fn ptr_1_the_wheel_scrolls_the_text_three_lines_a_notch_and_the_cursor_keeps_its_context() {
    let mut r = rig("ptr-wheel", &[&format!("f.txt={}", lines(100))], &["f.txt"]);
    assert_eq!(view(&r), (1, 1));
    r.wheel(false, 30, 5);
    // 3 lines on, and the cursor at least 8 lines in (VIS-12)
    assert_eq!(view(&r), (4, 12));
    r.wheel(false, 30, 5).wheel(false, 30, 5);
    assert_eq!(view(&r), (10, 18));
    r.wheel(true, 30, 5);
    assert_eq!(view(&r), (7, 18), "the cursor stays while it is in the context");
    for _ in 0..5 {
        r.wheel(true, 30, 5);
    }
    assert_eq!(view(&r).0, 1, "the top");
    let (_, line) = view(&r);
    assert!(line <= 1 + 14, "the cursor kept its 8 lines below: {line}");
    assert_eq!(r.text(), lines(100).trim_end(), "scrolling edits nothing");
    for _ in 0..60 {
        r.wheel(false, 30, 5);
    }
    let (top, line) = view(&r);
    assert_eq!(line, 100, "at the end the cursor is on the last line");
    assert!(top > 80 && top <= 92, "the last lines stay on the screen: {top}");
}

#[test]
fn ptr_1_the_wheel_scrolls_what_is_under_the_pointer() {
    let names: Vec<String> = (0..40).map(|i| format!("file{i:02}.txt")).collect();
    let mut files: Vec<&str> = names.iter().map(String::as_str).collect();
    let long = format!("long.txt={}", lines(100));
    files.push(&long);
    let mut r = rig_sized("ptr-under", &files, &["long.txt"], (80, 12));
    r.keys("<C-n><Esc>");
    let before = view(&r);
    assert_eq!(r.sim.panel.top, 0);
    r.wheel(false, 3, 4); // over the panel
    assert_eq!(r.sim.panel.top, 3, "3 rows a notch");
    assert_eq!(view(&r), before, "the text did not move");
    r.wheel(false, 3, 4);
    assert_eq!(r.sim.panel.top, 6);
    assert!(r.sim.panel.cur >= 6, "the panel's cursor stays in view");
    assert!(!r.sim.panel.focus, "scrolling gives no focus");
    r.wheel(true, 3, 4);
    assert_eq!(r.sim.panel.top, 3);
    r.wheel(false, 50, 4); // over the text
    assert_eq!(r.sim.panel.top, 3, "the panel did not move");
    assert!(view(&r).0 > before.0);
    // the status line and the blank column scroll nothing
    let (p, v) = (r.sim.panel.top, view(&r));
    r.wheel(false, 50, 11).wheel(false, 26, 4);
    assert_eq!((r.sim.panel.top, view(&r)), (p, v));
    // the panel stops at its last row
    for _ in 0..40 {
        r.wheel(false, 3, 4);
    }
    assert_eq!(r.sim.panel.top, 41 - 11, "the last row is the last row of the screen");
}

#[test]
fn ptr_1_dragging_in_the_text_selects_charwise_as_visual_does_and_release_keeps_it() {
    let mut r = rig("ptr-drag", &["f.txt=alpha beta\ngamma delta\n"], &["f.txt"]);
    r.mouse(K::Down(B::Left), T + 2, 0);
    assert_eq!(
        (r.app().vi.mode, r.app().selection()),
        (Mode::Normal, None),
        "a press selects nothing"
    );
    r.mouse(K::Drag(B::Left), T + 6, 0);
    assert_eq!(r.app().vi.mode, Mode::Visual);
    assert_eq!(r.app().selection(), Some((2, 7)), "from the press to the pointer, both chars in");
    r.mouse(K::Drag(B::Left), T + 3, 1); // down a line
    assert_eq!(r.app().selection(), Some((2, 15)));
    r.mouse(K::Up(B::Left), T + 3, 1);
    assert_eq!(r.app().vi.mode, Mode::Visual, "released: the selection stays");
    assert_eq!(r.app().selection(), Some((2, 15)));
    // it is Visual mode's selection: its keys work on it
    r.keys("d");
    assert_eq!(r.text(), "ala delta");
}

#[test]
fn ptr_1_a_drag_backwards_and_back_to_the_press_select_what_visual_would() {
    let mut r = rig("ptr-back", &["f.txt=alpha beta\n"], &["f.txt"]);
    r.mouse(K::Down(B::Left), T + 6, 0);
    r.mouse(K::Drag(B::Left), T + 2, 0);
    assert_eq!(r.app().selection(), Some((2, 7)), "the same chars from either end");
    r.mouse(K::Drag(B::Left), T + 6, 0);
    assert_eq!(r.app().selection(), Some((6, 7)), "back on the press: one char");
    r.mouse(K::Up(B::Left), T + 6, 0);
    // the keyboard makes the same selection
    let mut k = rig("ptr-back-k", &["f.txt=alpha beta\n"], &["f.txt"]);
    k.keys("llllllvhhhh");
    assert_eq!(k.app().selection(), Some((2, 7)));
}

#[test]
fn ptr_1_a_drag_that_leaves_the_text_is_cut_at_its_edge() {
    let mut r = rig("ptr-edge", &["f.txt=alpha\nbeta\ngamma\n"], &["f.txt"]);
    r.mouse(K::Down(B::Left), T + 1, 0);
    r.mouse(K::Drag(B::Left), 200, 200);
    assert_eq!(r.app().vi.mode, Mode::Visual);
    let (s, e) = r.app().selection().unwrap();
    assert_eq!(s, 1);
    assert_eq!(e, r.app().buf.text.len_chars(), "to the last char of the last line");
    r.mouse(K::Drag(B::Left), 0, 0);
    assert_eq!(r.app().selection(), Some((0, 2)));
    r.mouse(K::Up(B::Left), 0, 0);
}

#[test]
fn ptr_1_a_click_ends_a_selection_and_starts_over() {
    let mut r = rig("ptr-reclick", &["f.txt=alpha beta\n"], &["f.txt"]);
    r.keys("vlll");
    assert_eq!(r.app().vi.mode, Mode::Visual);
    r.click(T + 8, 0);
    assert_eq!((r.app().vi.mode, r.app().selection()), (Mode::Normal, None));
    assert_eq!(r.status(), "f.txt  1:9  NORMAL");
    // a press that moves in Insert mode does not turn into Visual (v would be text)
    r.keys("i");
    r.mouse(K::Down(B::Left), T + 1, 0);
    r.mouse(K::Drag(B::Left), T + 5, 0);
    r.mouse(K::Up(B::Left), T + 5, 0);
    assert_eq!(r.app().vi.mode, Mode::Insert);
    assert_eq!(r.text(), "alpha beta");
}

#[test]
fn ptr_1_the_mouse_reporting_sequences_are_each_others_reverse() {
    let on = String::from_utf8(MOUSE_ON.to_vec()).unwrap();
    let off = String::from_utf8(MOUSE_OFF.to_vec()).unwrap();
    assert_eq!(on, "\x1b[?1000h\x1b[?1002h\x1b[?1006h", "buttons, drags, SGR coding");
    assert_eq!(off, on.replace('h', "l").split_inclusive('l').rev().collect::<String>());
    let mut out = Vec::new();
    sim::paint::leave(&mut out);
    let out = String::from_utf8(out).unwrap();
    assert!(out.starts_with(&off), "the mouse is let go first: {out:?}");
    assert!(out.ends_with(&String::from_utf8(si::io::term::LEAVE.to_vec()).unwrap()));
}

// ---- PTR-2: the mouse never edits ----

#[test]
fn ptr_2_a_click_only_moves_the_cursor_the_focus_the_scroll_or_the_selection() {
    let mut r = rig("ptr2-noedit", &["f.txt=alpha beta\ngamma\n", "g.txt"], &["f.txt"]);
    let (text, undo) = (r.text(), r.app().buf.version());
    r.keys("<C-n>");
    for (x, y) in [(3, 0), (3, 1), (T + 30, 0), (T + 40, 3), (200, 200), (0, 23), (30, 4)] {
        for kind in [
            K::Down(B::Left),
            K::Drag(B::Left),
            K::Up(B::Left),
            K::Down(B::Right),
            K::Up(B::Right),
            K::Down(B::Middle),
            K::Up(B::Middle),
            K::Moved,
            K::ScrollUp,
            K::ScrollDown,
            K::ScrollLeft,
            K::ScrollRight,
        ] {
            r.mouse(kind, x, y);
        }
    }
    assert_eq!(r.sim.bufs[0].app.buf.text.to_string(), text);
    assert_eq!(r.sim.bufs[0].app.buf.version(), undo, "no edit, not even an undo step");
    assert!(!r.sim.bufs[0].app.buf.dirty());
    let touched = |e: &SimEffect| {
        matches!(
            e,
            SimEffect::App(
                _,
                Effect::Write { .. }
                    | Effect::Snapshot { .. }
                    | Effect::Spawn { .. }
                    | Effect::Clipboard(_)
            )
        )
    };
    assert!(!r.log.iter().any(touched), "nothing saved, snapshotted, run or copied: {:?}", r.log);
}

#[test]
fn ptr_2_the_other_buttons_and_the_horizontal_wheel_do_nothing() {
    let mut r = rig("ptr2-buttons", &["f.txt=alpha beta\n"], &["f.txt"]);
    let before = r.screen.clone();
    for kind in [
        K::Down(B::Right),
        K::Up(B::Right),
        K::Drag(B::Right),
        K::Down(B::Middle),
        K::Up(B::Middle),
        K::Drag(B::Middle),
        K::Moved,
        K::ScrollLeft,
        K::ScrollRight,
    ] {
        r.mouse(kind, T + 3, 0);
    }
    assert_eq!(r.screen, before);
}

#[test]
fn ptr_2_a_click_in_the_status_line_does_nothing() {
    let mut r = rig("ptr2-status", &["f.txt=alpha beta\n"], &["f.txt"]);
    r.keys("lll");
    r.keys("v");
    let before = (r.screen.clone(), r.app().selection());
    for x in [0, 5, 20, 79] {
        r.click(x, 23);
        r.mouse(K::Down(B::Left), x, 23);
        r.mouse(K::Drag(B::Left), x, 23);
        r.mouse(K::Up(B::Left), x, 23);
    }
    assert_eq!(
        (r.screen.clone(), r.app().selection()),
        before,
        "not the cursor, not the mode, not the message"
    );
}

#[test]
fn ptr_2_a_click_forgets_a_half_typed_command_and_leaves_the_command_line_alone() {
    let mut r = rig("ptr2-pending", &["f.txt=alpha beta gamma\n"], &["f.txt"]);
    r.keys("d");
    r.click(T + 6, 0);
    r.keys("w"); // it would delete a word, had the d waited
    assert_eq!(r.text(), "alpha beta gamma");
    assert_eq!(r.status(), "f.txt  1:12  NORMAL");
    r.keys("3");
    r.click(T, 0);
    r.keys("x");
    assert_eq!(r.text(), "lpha beta gamma", "the 3 was forgotten too: one char, not three");
    r.keys("z");
    r.click(T + 1, 0);
    r.keys("h"); // `zh` (SYN-2) was not typed: the z was forgotten
    assert!(!r.status().starts_with("syntax colors"), "{}", r.status());
    // the command line: a click in the text does nothing to it
    r.keys(":set");
    let before = r.status().to_string();
    r.click(T + 2, 0);
    r.mouse(K::Down(B::Left), T + 2, 0);
    r.mouse(K::Drag(B::Left), T + 5, 0);
    r.wheel(false, T, 0);
    assert_eq!((r.app().vi.mode, r.status().to_string()), (Mode::Cmd, before));
    r.keys("<Esc>");
    assert_eq!(r.app().vi.mode, Mode::Normal);
}

#[test]
fn ptr_2_a_modified_file_stays_as_it_is_and_nothing_is_saved_by_the_mouse() {
    let mut r = rig("ptr2-mod", &["f.txt=alpha beta\n"], &["f.txt"]);
    r.keys("x");
    let text = r.text();
    for i in 0..30u16 {
        r.click(T + i % 12, 0);
        r.mouse(K::Down(B::Left), T + 1, 0);
        r.mouse(K::Drag(B::Left), T + i % 9, 0);
        r.mouse(K::Up(B::Left), T + i % 9, 0);
        r.wheel(i % 2 == 0, T, 0);
    }
    assert_eq!(r.text(), text);
    assert!(r.app().buf.dirty());
    assert!(!r.log.iter().any(|e| matches!(e, SimEffect::App(_, Effect::Write { .. }))));
    assert_eq!(std::fs::read_to_string(r.cwd.join("f.txt")).unwrap(), "alpha beta\n");
}

#[test]
fn ptr_2_any_mouse_event_at_any_place_in_any_mode_edits_nothing_and_never_panics() {
    // from a roomy screen to the smallest one: a panel too narrow to be drawn, two rows
    for (n, size) in [(50, (50, 14)), (30, (30, 3)), (20, (6, 2)), (10, (80, 24)), (11, (3, 5))] {
        fuzz(n, size, sim::layout::Skin::Calm);
        // The boxes (LOOK-3) move the text, the panel and the borders: the same, in depth.
        fuzz(n + 100, size, sim::layout::Skin::Depth);
    }
}

fn fuzz(seed: u64, size: (u16, u16), skin: sim::layout::Skin) {
    let big: String = lines(60);
    let files = [&format!("f.txt={big}"), "日本語.txt=日本語\n\ttab\n", "e.txt="];
    let mut r = rig_env(&format!("ptr2-fuzz{seed}"), &files, &["f.txt"], size, |e| {
        (e.skin, e.syn) = (skin, sim::color::Syntax::new(true));
    });
    r.send(SimEvent::Term(si::core::app::Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys("<C-n><Esc>:e 日本語.txt\n");
    let text = r.sim.bufs.iter().map(|b| b.app.buf.text.to_string()).collect::<Vec<_>>();
    let mut rng = Rng(0x2545_f491_4f6c_dd1d ^ seed);
    let kinds = [
        K::Down(B::Left),
        K::Up(B::Left),
        K::Drag(B::Left),
        K::Down(B::Right),
        K::Up(B::Middle),
        K::Moved,
        K::ScrollUp,
        K::ScrollDown,
        K::ScrollLeft,
        K::ScrollRight,
    ];
    let keys =
        ["<Esc>", "v", "V", "i", ":", "/", "y", "3", "z", "<C-n>", "<Tab>", "<C-p>", "<Esc>"];
    for _ in 0..3000 {
        let (x, y) =
            (rng.below(usize::from(size.0) + 8) as u16, rng.below(usize::from(size.1) + 6) as u16);
        r.mouse(kinds[rng.below(kinds.len())], x, y);
        if rng.below(7) == 0 {
            // Keys that change the mode, never the text: in Insert mode only Esc is typed
            let key = keys[rng.below(keys.len())];
            let typing = r.sim.bufs.iter().any(|b| b.app.vi.mode == Mode::Insert);
            r.keys(if typing { "<Esc>" } else { key });
        }
        assert!(r.sim.bufs.iter().all(|b| !b.app.buf.dirty()), "after {x},{y} at {size:?}");
    }
    let now = r.sim.bufs.iter().map(|b| b.app.buf.text.to_string()).collect::<Vec<_>>();
    // (a click on the panel's rows may have opened more files: they are unmodified, checked above)
    assert_eq!(now[..text.len()], text[..]);
    let touched = |e: &SimEffect| {
        matches!(e, SimEffect::App(_, Effect::Write { .. } | Effect::Snapshot { .. }))
    };
    assert!(!r.log.iter().any(touched));
    assert_eq!(r.quit, None);
}

#[test]
fn ptr_1_a_drag_is_drawn_as_the_selection_golden() {
    // With a palette the selection is the neutral gray (`g`), as with the keys (SYN-5); the
    // pointer's char is where the terminal's cursor is.
    let rust = "main.rs=fn main() {\n    let s = \"a b\";\n}\n";
    let mut r = rig_env("ptr-golden", &[rust], &["main.rs"], (30, 6), |e| {
        e.syn = sim::color::Syntax::new(true);
        e.syn.bg((0x1e, 0x1e, 0x1e));
    });
    r.mouse(K::Down(B::Left), T + 4, 1);
    r.mouse(K::Drag(B::Left), T + 9, 1);
    r.mouse(K::Drag(B::Left), T + 2, 2);
    r.mouse(K::Up(B::Left), T + 2, 2);
    assert_eq!(r.app().vi.mode, Mode::Visual);
    golden("ptr_1_drag_selection", &dump_screen(&r.shot));
}
