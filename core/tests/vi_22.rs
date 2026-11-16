//! VI-22 (lines moved with Alt-h/j/k/l, mini.move's). Rows are (before, keys, after); `|` is the cursor.
//! Neovim 0.12.6 with mini.move gave the `after` of every row but those that say otherwise.
#![allow(clippy::single_range_in_vec_init)] // the columns of a mark
mod common;
use common::*;
use sim_core::core::app::{App, Event, Key};
use sim_core::core::vi::Mode;
use sim_core::io::term::convert_all;

#[test]
fn vi_22_alt_j_and_alt_k_move_the_line_with_a_count_as_far_as_there_are_lines() {
    table(&[
        ("a|1\nb2\nc3\nd4", "<M-j>", "b2\na|1\nc3\nd4"),
        ("a|1\nb2\nc3\nd4", "2<M-j>", "b2\nc3\na|1\nd4"),
        ("a|1\nb2\nc3\nd4", "9<M-j>", "b2\nc3\nd4\na|1"),
        ("a1\nb2\nc3\nd|4", "<M-j>", "a1\nb2\nc3\nd|4"),
        ("a1\nb2\nc|3\nd4", "<M-k>", "a1\nc|3\nb2\nd4"),
        ("a1\nb2\n|c3\nd4", "<M-k><M-k>", "|c3\na1\nb2\nd4"),
        ("|a1\nb2\nc3\nd4", "<M-k>", "|a1\nb2\nc3\nd4"),
        // (sim: each move is one undo step, as the brief has it; mini.move makes consecutive ones one)
        ("a|1\nb2\nc3", "<M-j>u", "a|1\nb2\nc3"),
        ("a|1\nb2\nc3", "<M-j><M-j>u", "b2\na|1\nc3"),
        // (sim: `.` repeats a move, as it repeats any change; mini.move's cannot be repeated)
        ("a|1\nb2\nc3", "<M-j>.", "b2\nc3\na|1"),
    ]);
}

#[test]
fn vi_22_in_visual_mode_the_selected_lines_move_and_stay_selected() {
    let mut a = typed("a1\nb|2\nc3\nd4", "Vj<M-j>");
    assert_eq!((text(&a), a.vi.mode), ("a1\nd4\nb2\nc3".into(), Mode::VLine));
    feed(&mut a, "<M-k><M-k><Esc>");
    assert_eq!(text(&a), "b2\nc3\na1\nd4");
    // a V-LINE selection that ends on an empty line takes it along
    let a = typed("end;\nfoo(bar)\n|\nx", "Vk3<M-k><Esc>");
    assert_eq!(text(&a), "foo(bar)\n\nend;\nx");
    // a charwise selection moves the lines it is on (mini moves its chars: not built)
    let a = typed("a1\nb|2\nc3\nd4", "vj<M-j>d");
    assert_eq!(text(&a), "a1\nd4\nb");
}

#[test]
fn vi_22_alt_l_and_alt_h_shift_by_the_indent_unit_and_the_cursor_stays_on_its_char() {
    table(&[
        ("    a|1\nb2", "<M-l>", "        a|1\nb2"),
        ("    a|1\nb2", "<M-h>", "a|1\nb2"),
        ("    a|1\nb2", "2<M-l>", "            a|1\nb2"),
        ("    a|1\nb2", "9<M-h>", "a|1\nb2"),
        ("a|1\n\nc3\n    d", "<M-l>", "    a|1\n\nc3\n    d"),
    ]);
    let mut a = typed("    a|1\nb2", "Vj<M-l>");
    assert_eq!((text(&a), a.vi.mode), ("        a1\n    b2".into(), Mode::VLine));
    feed(&mut a, "<M-h><M-h>");
    assert_eq!(text(&a), "a1\nb2");
    // a count that would add more than 16 MiB is refused (VI-2), and takes no time
    let a = typed("    a|1", "99999999999999999999<M-l>");
    assert_eq!((text(&a), a.msg.as_deref()), ("    a1".into(), Some("too large: 16 MiB at most")));
}

#[test]
fn vi_22_the_marks_go_with_their_lines() {
    let mut a = typed("a|1\nb2\nc3\nd4", "jmajmbgg<M-j><M-j>");
    assert_eq!(text(&a), "b2\nc3\na1\nd4");
    feed(&mut a, "'ax'bx");
    assert_eq!(text(&a), "2\n3\na1\nd4");
    let mut a = typed("|a1\nb2\nc3\nd4", "majjmb<M-k><M-k>");
    assert_eq!(text(&a), "c3\na1\nb2\nd4");
    feed(&mut a, "'ax'bx");
    assert_eq!(text(&a), "3\n1\nb2\nd4");
}

#[test]
fn vi_22_the_marks_of_changes_from_the_disk_go_with_their_lines() {
    let mut a = typed("|a1\nb2\nc3\nd4", "");
    a.buf.marks = vec![mark(1, '~', vec![0..1]), mark(3, '+', vec![0..2])].into();
    feed(&mut a, "3G<M-k><M-k>");
    // c3 went above a1 and b2, and b2 is a line further down, with its mark; d4 stays
    let at: Vec<(usize, char)> = a.buf.marks.iter().map(|m| (m.line, m.kind)).collect();
    assert_eq!((text(&a), at), ("c3\na1\nb2\nd4".into(), vec![(2, '~'), (3, '+')]));
}

#[test]
fn vi_22_where_nothing_moves_an_alt_key_is_esc_and_the_key_as_before() {
    // Insert mode: Esc, then `j` moves down (a fast Esc and j over tmux arrive as Alt-j)
    let a = typed("a|1\nb2", "ix<M-j>");
    assert_eq!((marked(&a), a.vi.mode), ("ax1\nb|2".into(), Mode::Normal));
    // an operator half typed: Esc gives it up, then `j`
    let a = typed("a|1\nb2\nc3", "d<M-j>");
    assert_eq!(marked(&a), "a1\nb|2\nc3");
    // the command line: Esc closes it, then `k`
    let a = typed("a1\nb|2", ":<M-k>");
    assert_eq!((marked(&a), a.vi.mode), ("a|1\nb2".into(), Mode::Normal));
    // only Alt-h/j/k/l are keys of their own: Alt-x is Esc and x
    let ev = |c: char| {
        crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::ALT,
        ))
    };
    assert!(matches!(convert_all(ev('j'))[..], [Event::Key(Key::Alt('j'))]));
    assert!(matches!(convert_all(ev('x'))[..], [Event::Key(Key::Esc), Event::Key(Key::Char('x'))]));
}

#[test]
fn vi_22_a_move_after_the_selection_is_gone_does_not_look_at_where_it_was() {
    // (the fuzz found it: the other end of a selection that a delete took away is past the text)
    let a = typed("|", "i\t<Esc>Vjd<M-j><M-l>");
    assert_eq!(text(&a), "");
    // and a selection of an empty text, which takes no char at all
    let a = typed("|", "vj<M-l>Vk<M-j>");
    assert_eq!(text(&a), "");
}

#[test]
fn vi_22_a_read_only_buffer_moves_nothing() {
    let mut a = App::new(None, Ok(Some(b"\xffa\nb\n".to_vec())), (80, 24));
    feed(&mut a, "<M-j>");
    assert_eq!(a.msg.as_deref(), Some("read-only: not valid UTF-8"));
}
