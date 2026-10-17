//! VI-5: Visual mode (`v`, `V`, `o`) with the operators of VI-3, and how it is drawn.
//! Rows are (before, keys, after); `|` is the cursor.
mod common;
use common::*;
use sim_core::core::vi::Mode;

#[test]
fn vi_5_charwise_selection_is_inclusive_and_takes_operators() {
    table(&[
        ("|abcd", "vld", "|cd"),
        ("a|bcd", "vd", "a|cd"),
        ("|abcd", "v$d", "|"),
        ("|abcd", "v2ld", "|d"),
        ("a|bc\ndef", "vjd", "a|f"),
        ("abc|d", "vhhd", "|a"),
        ("|ab cd", "vwd", "|d"),
        ("|ab cd", "vfcd", "|d"),
        ("|a\nb\nc", "vGd", "|"),
        ("|abcd", "vlx", "|cd"),
        ("|abcd", "vl<Del>", "|cd"),
        ("|abc", "vlcX<Esc>", "|Xc"),
        ("|abcd", "vly$p", "abcda|b"),
        ("a|bcd", "vlohd", "|d"),
        ("a|bcd", "vlod", "a|d"),
        ("|abcd", "vly", "|abcd"),
    ]);
}

#[test]
fn vi_5_linewise_selection() {
    table(&[
        ("a\n|b\nc", "Vd", "a\n|c"),
        ("|a\nb\nc", "Vjd", "|c"),
        ("a\nb\n|c", "Vd", "a\n|b"),
        ("|a\nb\nc", "Vjjd", "|"),
        ("a\nb\n|c", "Vkd", "|a"),
        ("a\n  |b\nc", "VcX<Esc>", "a\n  |X\nc"),
        ("|a\nb\nc", "Vjyjp", "a\nb\n|a\nb\nc"),
        ("|a\nb\nc", "Vj>", "    |a\n    b\nc"),
        ("    |a\n    b\nc", "Vj<", "|a\nb\nc"),
        ("a|b\nc", "v>", "    |ab\nc"),
        ("|a\nb\nc", "Vjo>", "    |a\n    b\nc"),
    ]);
}

#[test]
fn vi_5_text_objects_extend_the_selection() {
    table(&[
        ("fo|o bar", "viwd", "| bar"),
        ("fo|o bar baz", "vawd", "|bar baz"),
        ("f(a|b)", "vi(d", "f(|)"),
        ("f(a|b)", "va(d", "|f"),
        ("say \"a|b\"", "vi\"d", "say \"|\""),
        ("say \"a|b\" x", "va\"d", "say |x"),
        ("f(a|b)", "vi(y$p", "f(ab)a|b"),
        ("a|b", "vi(<Esc>", "a|b"),
    ]);
}

#[test]
fn vi_5_v_and_v_switch_kind_or_leave() {
    for (keys, want) in [
        ("v", Mode::Visual),
        ("V", Mode::VLine),
        ("vV", Mode::VLine),
        ("Vv", Mode::Visual),
        ("vv", Mode::Normal),
        ("VV", Mode::Normal),
        ("v<Esc>", Mode::Normal),
        ("V<Esc>", Mode::Normal),
        ("v<C-c>", Mode::Normal),
        ("vld", Mode::Normal),
        ("vlcx", Mode::Insert),
    ] {
        assert_eq!(typed("|abc", keys).vi.mode, want, "{keys}");
    }
    let mut a = typed("|abc", "v");
    assert_eq!(status(&screen(&mut a)), "[no name]  1:1  VISUAL");
    feed(&mut a, "V");
    assert_eq!(status(&screen(&mut a)), "[no name]  1:1  V-LINE");
    check("a|bc", "vl<Esc>", "ab|c");
}

#[test]
fn vi_5_colon_and_bang_open_the_command_line_with_the_selection() {
    for (keys, want) in
        [("Vj:", ":'<,'>"), ("v:", ":'<,'>"), ("Vj!", ":'<,'>!"), ("vl!", ":'<,'>!")]
    {
        let mut a = typed("|a\nb\nc", keys);
        assert_eq!((a.vi.mode, status(&screen(&mut a))), (Mode::Cmd, want), "{keys}");
    }
    check("|a\nb\nc", "Vj:s/./X/<CR>", "X\n|X\nc");
    check("a\n|b\nc\nd", "Vj<Esc>gg:'<,'>s/./X/<CR>", "a\nX\n|X\nd");
    check("a\nb\n|c\nd", "vk<Esc>G:'<,'>s/./X/<CR>", "a\nX\n|X\nd");
    check("|a\nb\nc", "Vj:<Esc>", "a\n|b\nc");
}

#[test]
fn vi_5_selection_is_drawn_with_underline_spans() {
    let mut out = String::new();
    let mut snap = |name: &str, a: &mut sim_core::core::app::App| {
        out += &format!("== {name}\n{}", dump(&screen(a)));
    };
    let text = "alpha beta\n\tgamma\n\nlong long long long long long long long\nz";
    let mut a = app_with(Some("f.rs"), text.as_bytes(), (24, 9));
    snap("none", &mut a);
    feed(&mut a, "wvl");
    snap("charwise inside a line", &mut a);
    feed(&mut a, "j");
    snap("charwise over a tab", &mut a);
    feed(&mut a, "jj");
    snap("charwise over an empty line and into a wrapped one", &mut a);
    feed(&mut a, "<Esc>ggjV");
    snap("linewise on a tab line", &mut a);
    feed(&mut a, "jjj");
    snap("linewise over a wrapped line", &mut a);
    feed(&mut a, "<Esc>");
    snap("left again", &mut a);
    golden("vi_5_visual_underline", &out);
}

#[test]
fn vi_5_the_selection_uses_underline_and_never_a_style() {
    let mut a = typed("|alpha\nbeta", "vj");
    let f = screen(&mut a);
    let (under, styled): (Vec<_>, Vec<_>) = f.rows.iter().partition(|r| !r.underline.is_empty());
    assert_eq!(under.len(), 2);
    assert!(
        f.rows
            .iter()
            .all(|r| r.spans.iter().all(|(_, s)| *s == sim_core::core::view::Style::Secondary))
    );
    assert!(styled.iter().all(|r| r.underline.is_empty()));
    assert_eq!(f.shape, sim_core::core::view::Shape::Block, "Visual uses the block cursor");
}
