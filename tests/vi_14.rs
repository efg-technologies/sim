//! VI-14: incremental search. While a `/` or `?` pattern is typed, the screen shows the first match
//! from where the search started, centered, as if the cursor were there (`App::pv`); the cursor
//! itself stays until Enter. Esc, or deleting the whole pattern, leaves nothing to give back. A
//! pattern that is bad so far, or matches nothing, shows the start, and nothing is highlighted.
mod common;
use common::*;
use si::core::app::{App, Event};
use si::core::vi::Mode;
use si::core::view::Style;

fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("row {i} of the text\n")).collect()
}

fn named(text: &str) -> App {
    app_with(Some("f"), text.as_bytes(), (60, 24))
}

/// The cursor: line and column.
fn at(a: &App) -> (usize, usize) {
    (a.buf.line_of(a.buf.cursor), a.buf.col())
}

/// The line the screen is drawn around: the first match while a search is typed, else the cursor's.
fn shown(a: &App) -> usize {
    a.buf.line_of(a.pv.filter(|_| a.vi.mode == Mode::Cmd).unwrap_or(a.buf.cursor))
}

/// The cursor, and the first buffer line and rows of it above the screen: where the user is.
fn place(a: &App) -> ((usize, usize), usize, usize) {
    (at(a), a.top, a.skip)
}

const FRUIT: &str = "apple\nbanana\ncherry\nbanana split\nDate\ncherry pie\n";

#[test]
fn vi_14_the_screen_follows_the_pattern_as_it_is_typed() {
    let mut a = named(FRUIT);
    let mut seen = vec![];
    for k in ["/", "b", "a", "n", "a", "n", "a", " ", "s"] {
        feed(&mut a, k);
        assert_eq!(a.vi.mode, Mode::Cmd, "still typing");
        assert_eq!(at(&a), (0, 0), "the cursor stays");
        seen.push(shown(&a));
    }
    assert_eq!(seen, [0, 1, 1, 1, 1, 1, 1, 3, 3], "the first match from the start, line by line");
    assert_eq!(text(&a), FRUIT.trim_end(), "nothing is changed");
    assert_eq!(status(&screen(&mut a)), "/banana s", "the status line is the command line");
    // Taking characters back goes back through the matches.
    for (k, line) in [("<BS>", 3), ("<BS>", 1), ("<BS><BS><BS><BS><BS>", 1), ("<BS>", 0)] {
        feed(&mut a, k);
        assert_eq!(shown(&a), line, "after {k}");
    }
    assert_eq!(a.vi.mode, Mode::Cmd, "the line is `/`: no pattern, still typing");
    // Taking the `/` back ends the typing.
    feed(&mut a, "<BS>");
    assert_eq!((a.vi.mode, at(&a)), (Mode::Normal, (0, 0)));
}

#[test]
fn vi_14_the_first_match_is_after_the_cursor_not_after_the_last_one_shown() {
    let mut a = named(FRUIT);
    feed(&mut a, "j"); // on banana
    feed(&mut a, "/cherry");
    assert_eq!(shown(&a), 2);
    feed(&mut a, " p");
    assert_eq!(shown(&a), 5);
    feed(&mut a, "<BS><BS>");
    assert_eq!(shown(&a), 2, "from the start again, not from the match shown last");
    assert_eq!(at(&a), (1, 0));
}

#[test]
fn vi_14_a_backward_search_goes_back_from_the_start() {
    let mut a = named(FRUIT);
    feed(&mut a, "G$"); // the end of "cherry pie"
    feed(&mut a, "?ban");
    assert_eq!(shown(&a), 3, "the nearest one before the cursor");
    feed(&mut a, "ana s");
    assert_eq!(shown(&a), 3);
    feed(&mut a, "<BS><BS><BS><BS><BS><BS><BS><BS><BS>");
    assert_eq!((a.vi.mode, at(&a)), (Mode::Normal, (5, 9)), "back at the start");
}

#[test]
fn vi_14_a_search_wraps_around_as_it_does_on_enter() {
    let mut a = named(FRUIT);
    feed(&mut a, "G/apple");
    assert_eq!(shown(&a), 0, "wrapped to the top");
    assert_eq!(at(&a).0, 5);
    feed(&mut a, "<CR>");
    assert_eq!(at(&a), (0, 0));
    assert_eq!(status(&screen(&mut a)), "search wrapped");
}

#[test]
fn vi_14_the_match_is_centered_on_the_screen_and_esc_gives_the_screen_back() {
    let mut a = named(&lines(300));
    feed(&mut a, "40j<C-d>3j");
    let start = place(&a);
    assert!(start.1 > 0, "premise: the view has scrolled");
    let before = screen(&mut a);
    feed(&mut a, "/row 250 ");
    assert_eq!((shown(&a), place(&a)), (249, start), "only the screen moves");
    let moved = screen(&mut a);
    assert!(moved.rows[11].text.contains("row 250 of the text"), "centered: {moved:?}");
    feed(&mut a, "<Esc>");
    assert_eq!((a.vi.mode, place(&a)), (Mode::Normal, start));
    assert_eq!(screen(&mut a), before, "the screen is what it was");
    // The same for a backward search, a pattern with no match, and a line left by Backspace.
    for keys in ["?row 3 ", "/zzzz", "/row 2", "?"] {
        feed(&mut a, keys);
        feed(&mut a, "<Esc>");
        assert_eq!((a.vi.mode, place(&a)), (Mode::Normal, start), "{keys}");
        assert_eq!(screen(&mut a), before, "{keys}");
    }
}

#[test]
fn vi_14_the_screen_stays_centered_when_something_else_redraws_it() {
    let mut a = named(&lines(300));
    feed(&mut a, "/row 250");
    let first = screen(&mut a);
    a.update(Event::Resize(60, 24));
    assert_eq!(screen(&mut a).rows[11], first.rows[11]);
    assert_eq!(screen(&mut a).rows[11].text, first.rows[11].text);
}

#[test]
fn vi_14_enter_moves_the_cursor_to_the_first_match() {
    let mut a = named(&lines(300));
    feed(&mut a, "/row 250");
    feed(&mut a, "<CR>");
    assert_eq!((a.vi.mode, at(&a)), (Mode::Normal, (249, 0)));
    let f = screen(&mut a);
    assert!(f.rows[11].text.contains("row 250 of the text"), "centered");
    // The pattern is the last search: `n` goes on from here, `N` back.
    feed(&mut a, "n");
    assert_eq!(at(&a).0, 249, "row 250 is the only match, so n comes back to it");
    feed(&mut a, "gg/row 1");
    feed(&mut a, "0<CR>");
    assert_eq!(at(&a).0, 9, "row 10");
    feed(&mut a, "n");
    assert_eq!(at(&a).0, 99, "row 100");
    feed(&mut a, "N");
    assert_eq!(at(&a).0, 9);
}

#[test]
fn vi_14_the_result_of_enter_is_the_result_of_a_search_without_the_preview() {
    // Compare with the same search done on a buffer that never saw the keys one by one: paste the
    // pattern into the command line, which does not preview.
    for (start, pattern, keys) in [
        ("", "banana", "/"),
        ("G", "apple", "/"),
        ("j", "cherry", "/"),
        ("jj", "banana", "?"),
        ("G", "an", "?"),
        ("", "nothing", "/"),
        ("", "(", "/"),
        ("", "^c", "/"),
        ("", "a$", "/"),
        ("", "Date", "/"),
        ("", "date", "/"),
    ] {
        let mut live = named(FRUIT);
        feed(&mut live, start);
        feed(&mut live, &format!("{keys}{pattern}<CR>"));
        let mut pasted = named(FRUIT);
        feed(&mut pasted, start);
        feed(&mut pasted, keys);
        pasted.update(Event::Paste(pattern.into()));
        feed(&mut pasted, "<CR>");
        assert_eq!(place(&live), place(&pasted), "{start}{keys}{pattern}");
        assert_eq!(live.msg, pasted.msg, "{start}{keys}{pattern}");
    }
}

#[test]
fn vi_14_a_paste_into_the_search_line_shows_its_match_too() {
    let mut a = named(FRUIT);
    feed(&mut a, "/");
    a.update(Event::Paste("cherry p".into()));
    assert_eq!(shown(&a), 5);
    feed(&mut a, "<Esc>");
    assert_eq!(shown(&a), 0);
}

#[test]
fn vi_14_a_pattern_that_is_bad_so_far_shows_the_start() {
    let mut a = named(FRUIT);
    feed(&mut a, "j");
    let start = place(&a);
    for bad in
        ["(", "[", "(an", "[a-", "\\", "a{", "a{2", "*", "+a", "?", "\\p", "(?", "(?i", "\\x", "[]"]
    {
        assert!(regex_lite::Regex::new(bad).is_err(), "premise: {bad:?} is not a pattern");
        feed(&mut a, &format!("/{bad}"));
        assert_eq!(a.vi.mode, Mode::Cmd);
        assert_eq!((shown(&a), place(&a)), (1, start), "{bad:?}");
        feed(&mut a, "<Esc>");
        assert_eq!(place(&a), start, "{bad:?}");
    }
    // A pattern that goes bad after it was fine goes back to the start.
    feed(&mut a, "/cherry");
    assert_eq!(shown(&a), 2);
    feed(&mut a, "(");
    assert_eq!(shown(&a), 1);
    feed(&mut a, ")");
    assert_eq!(shown(&a), 2, "`cherry()` is a pattern again");
    feed(&mut a, "<BS><BS>[");
    assert_eq!(shown(&a), 1, "`cherry[` is not");
}

#[test]
fn vi_14_a_pattern_with_no_match_shows_the_start() {
    let mut a = named(FRUIT);
    feed(&mut a, "j/cher");
    assert_eq!(shown(&a), 2);
    feed(&mut a, "x");
    assert_eq!(shown(&a), 1);
    feed(&mut a, "<CR>");
    assert_eq!(
        (at(&a), status(&screen(&mut a)).to_string()),
        ((1, 0), "not found: cherx".to_string())
    );
}

#[test]
fn vi_14_it_is_smartcase_like_the_search_itself() {
    let mut a = named(FRUIT);
    feed(&mut a, "/date");
    assert_eq!(shown(&a), 4, "lower case ignores case");
    feed(&mut a, "<Esc>/Date");
    assert_eq!(shown(&a), 4);
    feed(&mut a, "<Esc>G/Apple");
    assert_eq!(shown(&a), 5, "an upper case letter makes it exact: no `Apple` here, so the start");
    feed(&mut a, "<Esc>G/apple");
    assert_eq!(shown(&a), 0, "wrapped to the match");
}

#[test]
fn vi_14_esc_leaves_the_last_search_as_it_was() {
    let mut a = named(FRUIT);
    feed(&mut a, "/cherry<CR>");
    assert_eq!(at(&a).0, 2);
    feed(&mut a, "/banana<Esc>");
    assert_eq!(at(&a).0, 2);
    feed(&mut a, "n");
    assert_eq!(at(&a).0, 5, "n still searches for cherry");
    feed(&mut a, "N");
    assert_eq!(at(&a).0, 2);
}

#[test]
fn vi_14_nothing_is_highlighted() {
    let mut a = named(&lines(50));
    feed(&mut a, "/row 3");
    let f = screen(&mut a);
    for (i, r) in f.rows.iter().enumerate() {
        assert!(r.spans.iter().all(|(_, s)| *s != Style::Change), "row {i} has a change style");
        assert!(r.underline.is_empty());
    }
}

#[test]
fn vi_14_ex_command_lines_do_not_preview_and_do_not_move_anything() {
    let mut a = named(&lines(300));
    feed(&mut a, "30j");
    let start = place(&a);
    let before = screen(&mut a);
    feed(&mut a, ":s/row 250/x/");
    assert_eq!((a.pv, place(&a)), (None, start));
    feed(&mut a, "<Esc>");
    assert_eq!((place(&a), screen(&mut a)), (start, before));
    assert_eq!(text(&a), lines(300).trim_end());
}

#[test]
fn vi_14_an_empty_pattern_shows_the_start_and_enter_repeats_the_last_search() {
    let mut a = named(FRUIT);
    feed(&mut a, "/cherry<CR>");
    feed(&mut a, "gg/");
    assert_eq!((a.pv, at(&a)), (None, (0, 0)), "nothing typed yet");
    feed(&mut a, "<CR>");
    assert_eq!(at(&a).0, 2, "Enter on an empty line uses the last pattern");
}

#[test]
fn vi_14_marks_of_changes_stay_on_the_lines_the_preview_passes() {
    // DISK-6: a mark goes when the cursor enters its line. The preview does not move the cursor, so
    // the mark of a line shown for a moment is still there, and is on the screen when it shows.
    let mut a = named("one\ntwo\nthree\nfour\n");
    a.update(Event::Disk {
        bytes: Some(b"one\ntwo\nTHREE\nfour\n".to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
    assert_eq!(a.buf.marks.len(), 1);
    feed(&mut a, "/thr");
    let f = screen(&mut a);
    assert_eq!(shown(&a), 2);
    assert!(f.rows[2].text.starts_with("~"), "the mark is drawn: {:?}", f.rows[2].text);
    feed(&mut a, "<Esc>");
    assert_eq!(a.buf.marks.len(), 1, "and it is still there when the search is given up");
    feed(&mut a, "/thr<CR>");
    assert!(a.buf.marks.is_empty(), "the cursor entered the line");
}

#[test]
fn vi_14_a_change_on_disk_while_typing_is_taken_in_and_nothing_is_left_stale() {
    let mut a = named(&lines(300));
    feed(&mut a, "3l/row 250");
    assert_eq!(shown(&a), 249);
    // The file shrinks under the search: the line shown is gone.
    a.update(Event::Disk {
        bytes: Some(lines(5).into_bytes()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
    assert_eq!(a.vi.mode, Mode::Cmd);
    assert!(a.buf.cursor <= a.buf.text.len_chars());
    drop(screen(&mut a));
    feed(&mut a, "1");
    assert!(shown(&a) < 5);
    feed(&mut a, "<Esc>");
    assert!(at(&a).0 < 5, "{:?}", at(&a));
    drop(screen(&mut a));
    // A change that keeps the cursor where it was: Esc leaves it there, whatever was shown.
    let mut a = named("a\nb\nc\nd\ne\n");
    feed(&mut a, "j/d");
    a.update(Event::Disk {
        bytes: Some(b"a\nb\nc\nd\nE\n".to_vec()),
        hash: None,
        deadline: far(),
        seq: 0,
    });
    feed(&mut a, "<Esc>");
    assert_eq!(at(&a).0, 1, "where the search started");
}

#[test]
fn vi_14_a_file_that_loads_while_typing_the_search_leaves_nothing_stale() {
    // `:e g` was asked before; the file arrives while the search line is open. That ends the line
    // (the parser starts over).
    let mut a = named(&lines(300));
    feed(&mut a, "3l:e g\n/row 250");
    assert_eq!(shown(&a), 249);
    a.update(Event::Opened("g".into(), Ok(Some(b"n\n".to_vec())), 0));
    assert_eq!((a.vi.mode, text(&a)), (Mode::Normal, "n".to_string()));
    drop(screen(&mut a));
    for keys in ["/n", "<Esc>", "/n<CR>", "?x", "<BS><BS>"] {
        feed(&mut a, keys);
        assert!(a.buf.cursor <= a.buf.text.len_chars(), "{keys}");
        drop(screen(&mut a));
    }
    assert_eq!(at(&a), (0, 0));
    // A new file with as many lines: it is shown from its top, not around the old match.
    let mut a = named(&lines(300));
    feed(&mut a, "3l:e g\n/row 250");
    a.update(Event::Opened("g".into(), Ok(Some(lines(400).into_bytes())), 0));
    assert!(screen(&mut a).rows[0].text.contains("row 1 of"), "{:?}", screen(&mut a).rows[0]);
    // The file is not there: an empty buffer.
    let mut a = named(&lines(300));
    feed(&mut a, "3l:e g\n/row 250");
    a.update(Event::Opened("g".into(), Ok(None), 0));
    feed(&mut a, "/x<Esc>");
    assert_eq!((a.vi.mode, at(&a)), (Mode::Normal, (0, 0)));
    drop(screen(&mut a));
}

#[test]
fn vi_14_a_search_inside_the_text_of_a_long_wrapped_line_keeps_the_match_visible() {
    let long = format!("{}needle at the end\nshort\n", "word ".repeat(400));
    let mut a = named(&long);
    feed(&mut a, "/needle");
    let f = screen(&mut a);
    assert!(f.rows.iter().any(|r| r.text.contains("needle at the end")), "{f:?}");
    feed(&mut a, "<Esc>");
    assert_eq!(place(&a), ((0, 0), 0, 0));
}
