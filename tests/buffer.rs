//! SAFE-3: opening and saving without editing gives the same bytes.
mod common;
use common::*;
use proptest::prelude::*;
use si::core::buffer::Buffer;

fn round_trip(b: &[u8]) {
    assert_eq!(Buffer::decode(b).encode(), b, "{:?}", String::from_utf8_lossy(b));
}

#[test]
fn safe_3_fixed_cases_round_trip() {
    let cases = [
        "",
        "\n",
        "\n\n",
        "a",
        "a\n",
        "a\r\n",
        "a\r\nb",
        "a\r\nb\r\n",
        "a\r\nb\n",
        "a\nb\r\n",
        "\r",
        "\r\n",
    ];
    let more = [
        "\r\n\r\n",
        "\r\r\n",
        "a\rb",
        "\u{feff}",
        "\u{feff}\n",
        "\u{feff}a\r\n",
        "x\u{feff}y",
        "\u{feff}\u{feff}",
        "日本語\n",
    ];
    for s in cases.iter().chain(&more) {
        round_trip(s.as_bytes());
    }
}

#[test]
fn safe_3_line_endings_and_final_newline() {
    let b = Buffer::decode(b"a\r\nb\r\n");
    assert_eq!(b.text.to_string(), "a\nb", "CRLF files hold LF, without the final newline");
    let b = Buffer::decode(b"a\r\nb\n");
    assert_eq!(b.text.to_string(), "a\r\nb", "a mix is kept as it is (the \\r is a char)");
    assert_eq!(Buffer::decode(b"\xEF\xBB\xBFa").text.to_string(), "a", "the BOM is not text");
}

#[test]
fn safe_3_crlf_bom_and_missing_final_newline_survive_edits() {
    let mut a = app_with(Some("f"), b"\xEF\xBB\xBFa\r\nb\r\n", (40, 8));
    feed(&mut a, "ox\x1b");
    assert_eq!(a.buf.encode(), b"\xEF\xBB\xBFa\r\nx\r\nb\r\n");
    let mut a = app_with(Some("f"), b"a\nb", (40, 8));
    feed(&mut a, "jix\x1b");
    assert_eq!(a.buf.encode(), b"a\nxb", "no final newline stays that way");
}

#[test]
fn safe_3_an_empty_file_gets_a_final_newline_once_it_has_text() {
    let mut a = app_with(Some("f"), b"", (40, 8));
    assert_eq!(a.buf.encode(), b"");
    feed(&mut a, "ihi\x1b");
    assert_eq!(a.buf.encode(), b"hi\n");
    let mut a = app_with(Some("f"), b"\n", (40, 8));
    feed(&mut a, "x");
    assert_eq!(a.buf.encode(), b"\n", "an existing file keeps its newline");
}

/// Strings heavy in the chars that matter: `\r`, `\n` and the BOM.
fn tricky() -> impl Strategy<Value = String> {
    let ch = prop_oneof![
        3 => Just('\r'),
        3 => Just('\n'),
        1 => Just('\u{feff}'),
        3 => prop::char::range('a', 'c'),
        1 => any::<char>().prop_filter("no NUL", |c| *c != '\0'),
    ];
    prop::collection::vec(ch, 0..24).prop_map(|v| v.into_iter().collect())
}

proptest! {
    #[test]
    fn safe_3_encode_is_the_inverse_of_decode(s in tricky()) {
        round_trip(s.as_bytes());
    }
}

#[test]
fn safe_3_deleting_all_the_text_writes_an_empty_file_and_undo_brings_the_newline_back() {
    let bom = b"\xEF\xBB\xBF";
    for (file, bom) in
        [(&b"a\nb\n"[..], &b""[..]), (b"a\nb", b""), (b"\xEF\xBB\xBFa\r\nb\r\n", bom)]
    {
        for keys in ["ggdG", "ggVGd", "ggvGd", "999dd"] {
            let mut a = app_with(Some("f"), file, (40, 8));
            feed(&mut a, keys);
            assert_eq!(text(&a), "", "{file:?} {keys}");
            assert_eq!(a.buf.encode(), bom, "{file:?} {keys}: an empty file, not a lone newline");
            feed(&mut a, "u");
            assert_eq!(a.buf.encode(), file, "{file:?} {keys}: undo brings the final newline back");
            feed(&mut a, "<C-r>");
            assert_eq!(a.buf.encode(), bom, "{file:?} {keys}: and redo takes it away again");
        }
    }
    // Typing again ends the file as it ended before; one that had no final newline has none.
    for (file, want) in
        [(&b"abc"[..], &b"hi"[..]), (b"abc\n", b"hi\n"), (b"", b"hi\n"), (b"a\r\n", b"hi\r\n")]
    {
        let mut a = app_with(Some("f"), file, (40, 8));
        feed(&mut a, "ddihi<Esc>");
        assert_eq!(a.buf.encode(), want, "{file:?}");
    }
    // The text may go and come back inside one change without the file noticing.
    for keys in ["ccxyz<Esc>", "0Dixyz<Esc>", "xxxixyz<Esc>", "ddixyz<Esc>", "Vcxyz<Esc>"] {
        let mut a = app_with(Some("f"), b"abc", (40, 8));
        feed(&mut a, keys);
        assert_eq!(a.buf.encode(), b"xyz", "{keys}: a file without a final newline keeps that");
    }
    // An emptied file that was saved stays empty when it is saved again.
    let mut a = app_with(Some("f"), b"a\n", (40, 8));
    feed(&mut a, "dd:w\n");
    a.update(si::core::app::Event::Saved(Ok((si::core::buffer::hash(b""), None)), 0));
    let fx = feed(&mut a, ":w\n");
    let bytes = fx.iter().find_map(|e| match e {
        si::core::app::Effect::Write { bytes, .. } => Some(bytes.clone()),
        _ => None,
    });
    assert_eq!(bytes, Some(vec![]), "a second save writes the same empty file");
    // A file that holds one empty line is not an empty file, until its text goes.
    assert_eq!(app_with(Some("f"), b"\n", (40, 8)).buf.encode(), b"\n");
}
