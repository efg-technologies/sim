//! VI-3: no sequence of keys, however silly, panics (a panic aborts the editor with unsaved
//! text, P1), and the cursor always rests where the mode says it may.
mod common;
use common::*;
use proptest::prelude::*;
use si::core::app::Event;
use si::core::vi::Mode;

const TOKENS: &[&str] = &[
    "h",
    "j",
    "k",
    "l",
    "w",
    "b",
    "e",
    "W",
    "B",
    "E",
    "0",
    "^",
    "$",
    "gg",
    "G",
    "f(",
    "F\"",
    "t ",
    "T.",
    ";",
    ",",
    "%",
    "x",
    "r.",
    "J",
    "p",
    "P",
    "u",
    "U",
    "<C-r>",
    ".",
    "i",
    "a",
    "I",
    "A",
    "o",
    "O",
    "dd",
    "cc",
    "yy",
    "D",
    "C",
    "Y",
    ">>",
    "<<",
    "dw",
    "cw",
    "yw",
    "d$",
    "diw",
    "daw",
    "ci(",
    "da\"",
    "di'",
    "d%",
    "dG",
    "dgg",
    "dj",
    "dk",
    "dh",
    "dl",
    "db",
    "de",
    "c0",
    "y^",
    ">j",
    "<k",
    "v",
    "V",
    "o",
    "<Esc>",
    "<C-c>",
    "<C-d>",
    "<C-u>",
    "<C-w>",
    "<PageDown>",
    "<PageUp>",
    "zz",
    "/a<CR>",
    "?b<CR>",
    "n",
    "N",
    "*",
    "#",
    ":s/a/b/g<CR>",
    ":%s/./x/<CR>",
    ":2,3s/(/[/<CR>",
    ":3<CR>",
    ":$<CR>",
    ":'<,'>s/a/\\&&/<CR>",
    "<BS>",
    "<Del>",
    "<Left>",
    "<Right>",
    "<Up>",
    "<Down>",
    "<Home>",
    "<End>",
    "<Tab>",
    "<CR>",
    "2",
    "3",
    "9",
    "a",
    "b",
    " ",
    "(",
    ")",
    "\"",
    "'",
    "\t",
    "!j",
    "!!",
    ":",
    "/",
    "ZZ",
    "ZQ",
    "q",
    "\"",
    "<C-s>",
    "J",
    "3J",
    "9dd",
    "5x",
    "2p",
    "d2w",
    "c3e",
    "dfa",
    "dt)",
    "yiw",
    "vi(",
    "va\"",
    "viwd",
    "Vjd",
    "vlc",
    "ggVG>",
    "ggVGd",
    "9<C-d>",
];

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec!['a', 'b', ' ', '(', ')', '"', '\'', '\n', '\n', '\t', '.', '日']),
        0..40,
    )
    .prop_map(|v| v.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(400) })]
    #[test]
    fn vi_3_random_keys_never_panic(t in text(), keys in prop::collection::vec(prop::sample::select(TOKENS), 0..40), w in 1u16..30, h in 2u16..12) {
        let mut a = app_with(Some("f.rs"), t.as_bytes(), (w, h));
        for k in keys {
            feed(&mut a, k);
            let b = &a.buf;
            let (l, n) = (b.line_of(b.cursor), b.text.len_chars());
            prop_assert!(b.cursor <= n, "{k}: cursor {} past the end {n}", b.cursor);
            if matches!(a.vi.mode, Mode::Normal | Mode::Visual | Mode::VLine) {
                prop_assert!(b.cursor < b.end(l) || b.line_len(l) == 0, "{k}: Normal mode rests on a char");
            }
            prop_assert!(a.selection().is_none_or(|(s, e)| s <= e && e <= n));
            let f = screen(&mut a);
            prop_assert!(f.cursor.1 < h && f.cursor.0 < w);
        }
        a.update(Event::Paste("p\nq".into()));
        feed(&mut a, "<Esc>u<C-r>");
    }
}
