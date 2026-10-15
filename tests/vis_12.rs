//! VIS-12: the eyes find their place again. `n N * # Ctrl-d Ctrl-u PageDown PageUp :N +N
//! file:line` put the cursor line in the middle of the text area (also near the end of the
//! file), and every cursor movement keeps 8 rows of context (min(8, (rows - 1) / 2)).
mod common;
use common::*;
use si::core::app::App;
use si::core::cli::parse;

/// 80x24: 23 text rows, the middle one is row 11, and the context is 8 rows.
const MID: u16 = 11;

fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

fn big() -> App {
    app_with(Some("f.rs"), lines(200).as_bytes(), (80, 24))
}

/// (index of the cursor line, `top`, row of the cursor in the Frame) after typing `keys`.
fn view(a: &mut App, keys: &str) -> (usize, usize, u16) {
    feed(a, keys);
    let f = screen(a);
    (a.buf.line_of(a.buf.cursor), a.top, f.cursor.1)
}

#[test]
fn vis_12_the_jump_commands_put_the_cursor_line_in_the_middle() {
    for keys in [":100\n", "/line 100\n", "100Gzz"] {
        let mut a = big();
        assert_eq!(view(&mut a, keys), (99, 88, MID), "{keys:?}");
    }
    // `G` is not one of them: it scrolls just enough.
    let mut a = big();
    assert_eq!(view(&mut a, "50G"), (49, 49 - 14, 14));
    assert_eq!(view(&mut a, "zz"), (49, 49 - 11, MID));
}

#[test]
fn vis_12_n_and_n_and_star_and_hash_center_the_match() {
    let mut a = big();
    assert_eq!(view(&mut a, "/5$\n"), (4, 0, 4), "near the top there is nothing to scroll");
    assert_eq!(view(&mut a, "n"), (14, 3, MID));
    assert_eq!(view(&mut a, "n"), (24, 13, MID));
    assert_eq!(view(&mut a, "N"), (14, 3, MID));
    assert_eq!(view(&mut a, "N"), (4, 0, 4));
    let mut a = big();
    feed(&mut a, ":50\n");
    assert_eq!(
        view(&mut a, "*"),
        (50, 39, MID),
        "centered even though 1 line down needs no scroll"
    );
    assert_eq!(view(&mut a, "#"), (49, 38, MID));
    assert_eq!(view(&mut a, "#"), (48, 37, MID));
    assert_eq!(view(&mut a, "j"), (49, 37, MID + 1), "j does not center");
    assert_eq!(view(&mut a, "?line 120\n"), (119, 108, MID));
}

#[test]
fn vis_12_ctrl_d_ctrl_u_and_the_page_keys_center_the_cursor() {
    let mut a = big();
    assert_eq!(view(&mut a, "<C-d>"), (11, 0, 11));
    assert_eq!(view(&mut a, "<C-d>"), (22, 11, MID));
    assert_eq!(view(&mut a, "<C-u>"), (11, 0, 11));
    assert_eq!(view(&mut a, "<PageDown>"), (34, 23, MID));
    assert_eq!(view(&mut a, "<PageDown>"), (57, 46, MID));
    assert_eq!(view(&mut a, "<PageUp>"), (34, 23, MID));
}

#[test]
fn vis_12_plus_n_and_file_colon_line_at_startup_center_too() {
    for args in [vec!["+120", "f"], vec!["f:120"], vec!["f:120:3"]] {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let cli = parse(&args, |_| false).unwrap();
        let mut a = big();
        a.start(&cli.keys);
        let f = screen(&mut a);
        assert_eq!((a.buf.line_of(a.buf.cursor), a.top, f.cursor.1), (119, 108, MID), "{args:?}");
    }
}

#[test]
fn vis_12_near_the_end_of_the_file_the_centering_leaves_blank_rows() {
    let mut a = big();
    assert_eq!(view(&mut a, ":200\n"), (199, 188, MID));
    let f = screen(&mut a);
    assert_eq!(f.rows[MID as usize].text, " 200 line 200");
    assert!(
        f.rows[MID as usize + 1..23].iter().all(|r| r.text.is_empty()),
        "the rows past the end are blank"
    );
    assert_eq!(view(&mut a, "k"), (198, 188, MID - 1));
    assert_eq!(view(&mut a, "zz"), (198, 187, MID));
}

#[test]
fn vis_12_moving_keeps_eight_rows_of_context() {
    let mut a = big();
    for n in 1..=30usize {
        feed(&mut a, "j");
        let f = screen(&mut a);
        assert_eq!((a.top, f.cursor.1 as usize), (n.saturating_sub(14), n.min(14)), "after {n} j");
    }
    // Back up: nothing scrolls until fewer than 8 rows are left above.
    let mut a = big();
    feed(&mut a, ":100\n");
    for (keys, want) in [("kkk", (88, 8)), ("k", (87, 8)), ("kk", (85, 8))] {
        let (_, top, row) = view(&mut a, keys);
        assert_eq!((top, row as usize), want, "{keys}");
    }
    // At the top of the file there is no context to keep.
    let mut a = big();
    assert_eq!(view(&mut a, "3j2k"), (1, 0, 1));
    // The same at the bottom: `G` shows the last line on the last row.
    let mut a = big();
    assert_eq!(view(&mut a, "G"), (199, 177, 22));
    assert_eq!(view(&mut a, "14k"), (185, 177, 8));
    assert_eq!(view(&mut a, "k"), (184, 176, 8));
}

#[test]
fn vis_12_a_short_screen_keeps_half_the_text_area() {
    // 10 rows: 9 text rows, context min(8, 4) = 4.
    let mut a = app_with(Some("f.rs"), lines(100).as_bytes(), (40, 10));
    for n in 1..=12usize {
        feed(&mut a, "j");
        let f = screen(&mut a);
        assert_eq!((a.top, f.cursor.1 as usize), (n.saturating_sub(4), n.min(4)), "after {n} j");
    }
    feed(&mut a, ":50\n");
    assert_eq!((a.top, screen(&mut a).cursor.1), (49 - 4, 4), "the middle of 9 rows is row 4");
    let mut a = app_with(Some("f.rs"), lines(100).as_bytes(), (40, 2));
    feed(&mut a, "5j:50\n");
    assert_eq!(
        (a.top, screen(&mut a).cursor.1),
        (49, 0),
        "a one row text area: no context, no middle"
    );
}

#[test]
fn vis_12_context_is_measured_in_screen_rows_not_lines() {
    // Every line wraps into 3 rows at 72 columns.
    let line = "word ".repeat(30);
    let text = vec![line.trim_end(); 100].join("\n");
    let mut a = app_with(Some("a.md"), text.as_bytes(), (80, 24));
    assert_eq!(screen(&mut a).rows[3].text.get(..5), Some("   2 "));
    for n in 1..=12usize {
        feed(&mut a, "j");
        let f = screen(&mut a);
        let row = f.cursor.1 as usize;
        assert!(row <= 22 - 8, "after {n} j the cursor is on row {row}: 8 rows must stay below");
        assert_eq!(a.top, n.saturating_sub(row / 3), "{n}");
    }
    feed(&mut a, ":50\n");
    let f = screen(&mut a);
    assert!((9..=13).contains(&f.cursor.1), "centered to the nearest whole line: {}", f.cursor.1);
    // Going up through the wrapped lines keeps 8 rows above.
    for _ in 0..10 {
        feed(&mut a, "k");
        assert!(screen(&mut a).cursor.1 >= 8);
    }
}

#[test]
fn vis_12_a_huge_file_costs_what_the_screen_costs() {
    let mut a = app_with(Some("f.rs"), lines(100_000).as_bytes(), (80, 24));
    assert_eq!(view(&mut a, "G"), (99_999, 100_000 - 23, 22));
    assert_eq!(view(&mut a, "gg"), (0, 0, 0));
    assert_eq!(view(&mut a, ":50000\n"), (49_999, 49_999 - 11, MID));
    assert_eq!(view(&mut a, "<C-d>"), (50_010, 50_010 - 11, MID));
    assert_eq!(view(&mut a, "/line 99999\n"), (99_998, 99_998 - 11, MID));
    assert_eq!(
        view(&mut a, "G"),
        (99_999, 99_998 - 11, MID + 1),
        "G keeps the view when the last line is visible"
    );
}
