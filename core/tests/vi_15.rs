//! VI-15 (registers). Rows are (before, keys, after); `|` is the cursor.
mod common;
use common::*;
use sim_core::core::app::{Effect, Event};

fn clipboard(fx: &[Effect]) -> Vec<&str> {
    fx.iter()
        .filter_map(|e| if let Effect::Clipboard(t) = e { Some(t.as_str()) } else { None })
        .collect()
}

#[test]
fn vi_15_named_registers_hold_text_and_uppercase_appends() {
    table(&[
        ("|ab cd", "\"ayw\"ap", "aab| b cd"),
        ("|ab cd", "\"ayw$\"aP", "ab cab| d"),
        ("|ab cd", "\"ayw\"Ayy\"ap", "ab cd\n|ab \nab cd"),
        ("|ab\ncd", "\"ayy\"Ayyj\"ap", "ab\ncd\n|ab\nab"),
        ("|ab cd", "\"ayw\"byy\"ap", "aab| b cd"),
        ("|ab cd", "\"ayw\"Ayw\"ap", "aab ab| b cd"),
        ("|ab cd", "\"adw\"ap", "cab| d"),
        ("|ab cd", "\"ayw$p", "ab cdab| "),
        ("|a\nb", "\"ayyj\"ap\"aP", "a\nb\n|a\na"),
        ("|ab", "\"aylp", "a|ab"),
        ("|ab", "\"Ayl\"ap", "a|ab"),
        ("|ab", "\"qyl\"qp", "a|ab"),
        ("|ab", "\"\u{ff41}yl\"ap", "a|ab"),
    ]);
}

#[test]
fn vi_15_a_register_name_is_read_in_any_case_and_before_a_count() {
    table(&[
        ("|ab", "\"ayl\"Ap", "a|ab"),
        ("|ab", "\"ayl2\"ap", "aa|ab"),
        ("|ab", "\"ayl\"a2p", "aa|ab"),
        ("|a\nb\nc", "2\"ayy\"ap", "a\n|a\nb\nb\nc"),
        ("|a\nb\nc", "\"a2yyjj\"ap", "a\nb\nc\n|a\nb"),
    ]);
}

#[test]
fn vi_15_the_unnamed_register_points_to_what_was_written_last() {
    table(&[
        ("|ab cd", "\"ayw\"bdwP", "ab| cd"),
        ("|ab cd", "\"aywyw\"ap", "aab| b cd"),
        ("|ab cd", "yw\"adwP", "ab| cd"),
        ("|ab cd", "yw\"_dwP", "ab| cd"),
        ("|ab cd", "\"adw\"_xp", "dab| "),
        ("|ab cd", "\"\"ywp", "aab| b cd"),
        ("|ab cd", "yw\"\"p", "aab| b cd"),
    ]);
}

#[test]
fn vi_15_zero_is_the_last_yank_and_one_to_nine_the_deletes_of_lines() {
    table(&[
        ("|ab\ncd", "ywjdd\"0P", "a|bab"),
        ("|ab\ncd", "ywjdd\"0p", "aa|bb"),
        ("|a\nb\nc\nd", "dddddd\"1p", "d\n|c"),
        ("|a\nb\nc\nd", "dddddd\"2p", "d\n|b"),
        ("|a\nb\nc\nd", "dddddd\"3p", "d\n|a"),
        ("|a\nb\nc\nd", "dddddd\"3pu\"2p", "d\n|b"),
        ("|a\nb\nc\nd", "dd\"0p", "|b\nc\nd"),
        ("|a b\nc", "dw\"1p", "|b\nc"),
        ("|a b\nc", "dw\"-p", "ba| \nc"),
        ("|a b\nc", "dwx\"-p", "|b\nc"),
        ("|a b\nc", "d$\"-p", "a |b\nc"),
        ("|ab cd", "yw\"0p", "aab| b cd"),
        ("|ab", "\"1p", "|ab"),
        ("|a\nb\nc", "\"addj\"1p", "b\nc\n|a"),
    ]);
    // Nine pushes the first out: ten deletes leave "1 to "9 holding the last nine.
    let lines = (1..=10).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
    let mut a = typed(&format!("|{lines}\nend"), &"dd".repeat(10));
    assert_eq!(text(&a), "end");
    feed(&mut a, "\"9p");
    assert_eq!(text(&a), "end\nl2");
    feed(&mut a, "\"1p");
    assert_eq!(text(&a), "end\nl2\nl10");
}

#[test]
fn vi_15_a_small_delete_goes_to_minus_and_a_move_by_percent_to_one_as_well() {
    table(&[
        ("|(a) b", "d%\"1p", " (a|)b"),
        ("|(a) b", "d%\"-p", " (a|)b"),
        ("|(a) b", "\"ayw\"1p", "|(a) b"),
        ("|a b\nc", "\"add\"1p", "c\n|a b"),
        ("|a b\nc", "\"aD\"1p", "|\nc"),
        ("|ab ab", "/b\ndn\"-p", "abb |a"),
        ("|a b", "dw\"-P", "a| b"),
        ("|a (b) c", "fbd%\"1p", "a )(|b c"),
        ("|foo bar foo", "d*\"1P", "foo bar| foo"),
        ("|foo bar foo", "d*\"-P", "foo bar| foo"),
        ("|foo bar foo", "d#\"1P", "foo bar| foo"),
    ]);
}

#[test]
fn vi_15_the_black_hole_takes_everything_and_holds_nothing() {
    table(&[
        ("|ab\ncd", "yy\"_ddp", "cd\n|ab"),
        ("|ab cd", "yw\"_dwP", "ab| cd"),
        ("|ab", "\"_yl\"_p", "|ab"),
        ("|ab", "\"_ylp", "|ab"),
        ("|a\nb", "\"_dd\"1p", "|b"),
        ("|ab cd", "\"_cwxy<Esc>", "x|y cd"),
    ]);
    let mut a = app("ab");
    let fx = feed(&mut a, "\"_yl");
    assert!(clipboard(&fx).is_empty(), "the black hole is not the clipboard");
    assert!(!status(&screen(&mut a)).contains("yanked"), "and a yank into it says nothing");
}

#[test]
fn vi_15_a_register_with_a_name_stays_inside_and_a_plain_yank_goes_out() {
    let mut a = app("ab cd\nef");
    assert_eq!(clipboard(&feed(&mut a, "yw")), ["ab "]);
    assert!(clipboard(&feed(&mut a, "\"ayw\"byy\"_yy\"1yy\"-yl\"Ayw")).is_empty());
    assert!(clipboard(&feed(&mut a, "dwddp")).is_empty());
    assert!(clipboard(&feed(&mut a, "\"add")).is_empty());
    // It still says what it took: the clipboard is not the only sign.
    let mut a = app("ab cd");
    feed(&mut a, "\"ayw");
    assert_eq!(status(&screen(&mut a)), "3 chars yanked");
}

#[test]
fn vi_15_plus_and_star_write_the_clipboard_and_cannot_be_read() {
    let mut a = app("ab cd\nef");
    assert_eq!(clipboard(&feed(&mut a, "\"+yw")), ["ab "]);
    assert_eq!(clipboard(&feed(&mut a, "\"*yy")), ["ab cd\n"]);
    assert_eq!(clipboard(&feed(&mut a, "\"+dw")), ["ab "], "a delete named + is a cut");
    assert!(clipboard(&feed(&mut a, "dd")).is_empty(), "and a delete with no name is not");
    // They write what a plain yank writes: p puts it.
    table(&[
        ("|ab cd", "\"+ywP", "ab| ab cd"),
        ("|ab cd", "\"*ywP", "ab| ab cd"),
        ("|ab cd", "\"+dwP", "ab| cd"),
        ("|a\nb", "\"+ddp", "b\n|a"),
        ("|a\nb", "\"*dd\"1p", "b\n|a"),
    ]);
    // Nothing reads the clipboard: no program is run to ask for it (PLAIN-1), and a terminal does not
    // give it away. The terminal's own paste is the way in (VI-9).
    for keys in ["\"+p", "\"*p", "\"+P", "\"*P", "3\"+p"] {
        let mut a = app("ab");
        let fx = feed(&mut a, keys);
        assert_eq!(text(&a), "ab", "{keys}");
        assert!(clipboard(&fx).is_empty(), "{keys}");
        let said = status(&screen(&mut a)).to_string();
        assert!(said.contains("write-only") && said.contains("paste"), "{keys}: {said}");
    }
    // After a yank into the clipboard the message is the yank's.
    let mut a = app("ab cd");
    feed(&mut a, "\"+yw");
    assert_eq!(status(&screen(&mut a)), "3 chars yanked");
}

#[test]
fn vi_15_a_yank_into_the_clipboard_leaves_zero_alone_and_is_the_register_plus() {
    // A yank with a name does not touch "0 (vim's :help quote0): the clipboard's names are names.
    let mut a = app("alpha\nbeta");
    feed(&mut a, "yyj\"ayy\"+yy:reg\n");
    assert_eq!(
        status(&screen(&mut a)),
        "\"\" beta⏎ · \"+ beta⏎ · \"0 alpha⏎ · \"a beta⏎",
        "the unnamed register points to \"+, \"0 is the plain yank's, and sim shows what it sent"
    );
    let mut a = app("alpha\nbeta");
    feed(&mut a, "yyj\"*yy:reg\n");
    assert_eq!(
        status(&screen(&mut a)),
        "\"\" beta⏎ · \"+ beta⏎ · \"0 alpha⏎",
        "\"* is the same register"
    );
    table(&[
        ("|ab cd", "yw\"*yy\"0P", "ab| ab cd"),
        ("|ab cd", "yw\"+yl\"0P", "ab| ab cd"),
        ("|ab cd", "\"+ywP", "ab| ab cd"),
        // a cut named + fills + (and "1 for lines, as a name does), and not "-
        ("|ab cd", "\"+dw\"-p", "|cd"),
        ("|a\nb", "\"+dd\"1p", "b\n|a"),
    ]);
    // Reading it stays refused, and what sim sent is shown, not read from the clipboard.
    let mut a = app("ab cd");
    feed(&mut a, "\"+yw\"+p:reg\n");
    assert_eq!(text(&a), "ab cd");
    assert_eq!(status(&screen(&mut a)), "\"\" ab  · \"+ ab ");
}

#[test]
fn vi_15_visual_mode_and_x_take_a_register_too() {
    table(&[
        ("|ab cd", "v$\"ay\"ap", "aab c|db cd"),
        ("|ab cd", "vl\"ay$\"ap", "ab cda|b"),
        ("|ab\ncd", "V\"ad\"ap", "cd\n|ab"),
        ("|ab", "\"ax\"ap", "b|a"),
        ("|abc", "\"_x\"1p", "|bc"),
        ("|ab cd", "\"aD\"ap", "ab c|d"),
        ("|ab cd", "\"aY$\"ap", "ab cdab c|d"),
        ("a|b cd", "\"aCx<Esc>\"ap", "axb c|d"),
    ]);
}

#[test]
fn vi_15_dot_walks_the_numbered_registers() {
    let mut a = typed("|a\nb\nc\nd\ne", "dddddddd");
    assert_eq!(text(&a), "e");
    feed(&mut a, "\"1pu.u.");
    assert_eq!(marked(&a), "e\n|b", "`\"1p` puts d, `u.` puts \"2 (c), `u.` puts \"3 (b)");
    let a = typed("|a\nb\nc\nd\ne", "dddddddd\"1p..");
    assert_eq!(marked(&a), "e\nd\nc\n|b", "without `u` each one goes below the last");
    // A `"1` that was typed as text is text, not a register.
    let mut a = app("a");
    feed(&mut a, "i\"1<Esc>.");
    assert_eq!(text(&a), "\"\"11a");
}

#[test]
fn vi_15_a_name_that_is_no_register_fails() {
    for keys in ["\"!yy", "\"!p", "\"%dd", "\"ñp", "\".p", "\":p", "\"/p"] {
        let mut a = app("ab");
        feed(&mut a, keys);
        assert!(status(&screen(&mut a)).starts_with("not a register: "), "{keys}");
        assert_eq!(text(&a), "ab", "{keys}");
    }
    let mut a = app("ab");
    feed(&mut a, "\"a<Esc>x");
    assert_eq!(text(&a), "b", "Esc cancels the register name's command");
    let mut a = app("ab");
    feed(&mut a, "\"<Esc>x");
    assert_eq!(text(&a), "b", "and a half-typed `\"` too");
}

#[test]
fn vi_15_p_from_a_register_with_nothing_says_so_and_does_not_move() {
    for keys in ["\"zp", "\"1P", "\"-p", "\"0p", "\"_p", "3\"ap"] {
        let mut a = app("ab\ncd\n");
        feed(&mut a, "j");
        let before = marked(&a);
        feed(&mut a, keys);
        assert_eq!(marked(&a), before, "{keys}");
        assert_eq!(status(&screen(&mut a)), "the register is empty", "{keys}");
    }
}

#[test]
fn vi_15_registers_shows_each_register_that_holds_something() {
    let mut a = app("ab cd\nef");
    feed(&mut a, ":reg\n");
    assert_eq!(status(&screen(&mut a)), "no registers");
    feed(&mut a, "ywjdd\"ayl:registers\n");
    assert_eq!(status(&screen(&mut a)), "\"\" a · \"0 ab  · \"1 ef⏎ · \"a a");
    // Long text is cut, and a line break shows as ⏎.
    let mut a = app("0123456789abcdefghij\nxy");
    feed(&mut a, "yj:reg\n");
    let said = status(&screen(&mut a)).to_string();
    assert!(said.starts_with("\"\" 0123456789abcdef… ·"), "{said}");
    // Each one is cut to what is left of the line, so that the last one is on it, not past its end.
    let mut a = app("the quick brown fox jumps over the lazy dog");
    for r in 'a'..='e' {
        feed(&mut a, &format!("\"{r}yy"));
    }
    feed(&mut a, ":reg\n");
    let said = status(&screen(&mut a)).to_string();
    assert!(said.contains("\"e the"), "{said}");
    assert!(said.chars().count() <= 80, "{said}");
    // The black hole and a register that was cleared are not there.
    let mut a = app("ab");
    feed(&mut a, "\"_yl:reg\n");
    assert_eq!(status(&screen(&mut a)), "no registers");
}

#[test]
fn vi_15_no_register_content_reaches_the_terminal_raw() {
    let mut a = app("a\u{1b}[31mb\u{7}\u{202e}c");
    feed(&mut a, "yy:reg\n");
    let f = screen(&mut a);
    let line = status(&f);
    assert!(line.contains("^[") && line.contains("^G"), "{line}");
    assert!(
        !line.chars().any(|c| c.is_control() && c != '\n') && !line.contains('\u{202e}'),
        "{line:?}"
    );
}

#[test]
fn vi_15_registers_are_memory_only_and_the_app_hands_them_over() {
    let mut a = app("ab cd");
    feed(&mut a, "\"ayw");
    let regs = a.take_regs();
    feed(&mut a, "\"ap");
    assert_eq!(status(&screen(&mut a)), "the register is empty", "taken, it holds nothing");
    let mut b = app("xy");
    b.put_regs(regs);
    feed(&mut b, "\"aP");
    assert_eq!(text(&b), "ab xy");
}

#[test]
fn vi_15_a_register_is_never_written_to_the_disk() {
    // What the App asks the shell to put on the disk: a save, the journal's snapshot, the place it
    // remembers. A line that was typed and then deleted into a name is in the register and in none of
    // them (the file's own text is on the disk already, so the secret is typed here).
    let mut a = app_with(Some("/nonexistent/f"), b"rest\n", (80, 24));
    feed(&mut a, "ihunter2\n<Esc>k\"add");
    assert_eq!(text(&a), "rest");
    let mut fx = feed(&mut a, ":w\n");
    fx.extend(a.update(Event::Tick(60_000)));
    fx.extend(a.update(Event::Signal(15)));
    assert!(fx.iter().any(|e| matches!(e, Effect::Write { .. })), "a save was asked for");
    assert!(fx.iter().any(|e| matches!(e, Effect::Snapshot { .. })), "so was a snapshot");
    let asked = fx.iter().filter(|e| !matches!(e, Effect::Render(_))).collect::<Vec<_>>();
    assert!(!format!("{asked:?}").contains("hunter2"), "{asked:?}");
    let mut b = app_with(Some("/nonexistent/f"), b"rest\n", (80, 24));
    feed(&mut b, "ihunter2\n<Esc>k\"add\"ap");
    assert_eq!(text(&b), "rest\nhunter2", "and the register had it");
}
