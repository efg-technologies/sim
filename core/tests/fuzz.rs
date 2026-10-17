//! Fuzzing: random texts x random key sequences, checking the invariants that protect the user's
//! text (P1): no panic, cursor placement, undo/redo round trips, a frame that is safe and shows
//! the cursor where the char is, late answers from the world. The default case counts keep
//! `cargo test` fast; `SIM_FUZZ_CASES=100000 cargo test --test fuzz` runs every property that long.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use proptest::prelude::*;
use sim_core::core::app::{App, Event};
use sim_core::core::buffer::Buffer;
use sim_core::core::vi::Mode;
use sim_core::core::view::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Keys that never save, open or quit, so the text can be compared with the start.
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
    "fa",
    "ta",
    "Fb",
    "Tb",
    ";",
    ",",
    "%",
    "x",
    "r.",
    "ra",
    "r\t",
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
    "ciw",
    "caw",
    "diW",
    "daW",
    "ci(",
    "da(",
    "di\"",
    "da\"",
    "di'",
    "ci'",
    "di[",
    "da{",
    "ci<",
    "da>",
    "d%",
    "dG",
    "dgg",
    "dj",
    "dk",
    "dh",
    "dl",
    "db",
    "de",
    "dE",
    "dB",
    "dW",
    "cW",
    "cb",
    "ce",
    "c0",
    "c^",
    "y^",
    ">j",
    "<k",
    ">G",
    "<gg",
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
    "/$<CR>",
    "/^<CR>",
    "/x*<CR>",
    "/\\s<CR>",
    "/<CR>",
    "?<CR>",
    "n",
    "N",
    "*",
    "#",
    "dn",
    "dN",
    "d*",
    "c#",
    ":s/a/b/g<CR>",
    ":%s/./x/<CR>",
    ":2,3s/(/[/<CR>",
    ":3<CR>",
    ":$<CR>",
    ":0<CR>",
    ":%s/$/!/<CR>",
    ":%s/^/> /<CR>",
    ":s/\\s*$//<CR>",
    ":%s/\\(a\\)\\|b/[&\\1]/g<CR>",
    ":s/./\\&&/g<CR>",
    ":'<,'>s/a/\\&&/<CR>",
    ":%s/x*/-/g<CR>",
    ":s/a/\\//<CR>",
    ":s//Z/<CR>",
    ":%s/(.*)/<\\1>/<CR>",
    ":5,2s/a/q/<CR>",
    ":,<CR>",
    ":'<<CR>",
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
    "1",
    "2",
    "3",
    "9",
    "0",
    "a",
    "b",
    "x",
    " ",
    "(",
    ")",
    "\"",
    "'",
    "\t",
    "日",
    "é",
    "\\",
    "!j",
    "!!",
    ":",
    "/",
    "q:",
    ":q!<Esc>",
    "ZQZ",
    "9dd",
    "5x",
    "2p",
    "3P",
    "d2w",
    "c3e",
    "2dw",
    "3cw",
    "dfa",
    "dt)",
    "dFa",
    "dTb",
    "yiw",
    "vi(",
    "va\"",
    "viwd",
    "Vjd",
    "vlc",
    "vey",
    "vjy",
    "Vy",
    "vp",
    "viw>",
    "v$d",
    "v0d",
    "VGd",
    "ggVG>",
    "ggVGd",
    "9<C-d>",
    "5J",
    "3J",
    "3>>",
    "2<<",
    "gg9dd",
    "G9dd",
    "vjjo<Esc>",
    "viwo<Esc>",
    "<C-d><C-d>",
    "<C-u><C-u>",
    "99j",
    "99k",
    "99l",
    "99h",
    "99w",
    "99b",
    "99e",
    "99$",
    // Full-width keys (VI-10), the ends of their range and the chars next to it, and a search typed
    // and not finished (VI-14).
    "ｄｄ",
    "ｊ",
    "ｘ",
    "ｇｇ",
    "Ｇ",
    "ｆａ",
    "ｒａ",
    "ｄｉ（",
    "ｖｊｄ",
    "：",
    "／",
    "！",
    "～",
    "\u{ff00}",
    "\u{ff5f}",
    "/a",
    "?a",
    "/(",
    "/a<BS>",
    "/a<Esc>",
    "ｙｙｐ",
];

/// Counts that overflow `usize` (20 digits) or are merely huge: no panic, no spin, no giant allocation.
const BIG: &[&str] = &[
    "99999999999999999999",
    "18446744073709551615",
    "18446744073709551616",
    "4294967296",
    "1000000000",
];
const WITH_COUNT: &[&str] = &[
    "j",
    "k",
    "l",
    "h",
    "w",
    "b",
    "e",
    "W",
    "B",
    "E",
    "$",
    "G",
    "dd",
    "cc",
    "yy",
    ">>",
    "<lt><lt>",
    "x",
    "r.",
    "J",
    "p",
    "P",
    "dw",
    "cw",
    "ce",
    "de",
    "db",
    "d$",
    "dj",
    "dk",
    "dl",
    "dh",
    "yw",
    "y$",
    "<C-d>",
    "<C-u>",
    "<PageDown>",
    "<PageUp>",
    "fa",
    "ta",
    "Fa",
    "Ta",
    "ia<Esc>",
    "ox<Esc>",
    "Ax<Esc>",
    ".",
    "u",
    "<C-r>",
    "gg",
    "%",
    "D",
    "C",
    "Y",
    "X",
];

/// Characters that matter: ASCII words and punctuation, blanks, brackets, quotes, CJK, wide and
/// zero-width chars, and everything SAFE-8 talks about.
const ALPHABET: &[char] = &[
    'a', 'b', 'c', 'x', 'a', 'b', ' ', ' ', ' ', '\n', '\n', '\n', '\t', '.', '(', ')', '[', ']',
    '{', '}', '<', '>', '"', '\'', '\\', '_', '-', '$', '^', '*', '/', '日', '本', '語', '、',
    'あ', 'ア', 'ｱ', 'é', '😀', '\u{301}', '\r', '\x1b', '\0', '\x7f', '\u{85}', '\u{202e}',
    '\u{200b}', '\u{feff}', '\u{3000}', '\u{a0}', '\x01', '\u{61c}', '\u{fe0f}', '\u{200d}', '⚠',
    '👨',
];

fn gen_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(ALPHABET), 0..60)
        .prop_map(|v| v.into_iter().collect())
}

/// Texts with line-ending and final-newline variety, as the bytes of a file.
fn file_bytes() -> impl Strategy<Value = Vec<u8>> {
    (gen_text(), prop::bool::ANY, prop::bool::ANY, prop::bool::ANY).prop_map(
        |(t, crlf, nl, bom)| {
            let mut t = t.replace('\r', "");
            if crlf {
                t = t.replace('\n', "\r\n");
            }
            if nl && !t.is_empty() {
                t.push_str(if crlf { "\r\n" } else { "\n" });
            }
            let mut b = if bom { b"\xEF\xBB\xBF".to_vec() } else { vec![] };
            b.extend(t.as_bytes());
            b
        },
    )
}

fn sane(c: char) -> bool {
    !c.is_control()
        && !matches!(c, '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200b}' | '\u{feff}')
}

/// The cell under display column `col` of `row`: the char that starts there.
fn cell_at(row: &str, col: usize) -> Option<char> {
    let (cs, mut x) = (row.chars().collect::<Vec<_>>(), 0);
    for (i, &c) in cs.iter().enumerate() {
        let w = sim_core::core::view::width(&cs, i, x);
        if x == col && w > 0 {
            return Some(c);
        }
        x += w;
        if x > col {
            return None;
        }
    }
    None
}

/// Frame checks. `strict_width`: also that the cursor is on its char (only meaningful on a screen wide
/// enough for the gutter and the widest unit).
fn check_frame(a: &App, f: &Frame, strict_width: bool) -> Result<(), String> {
    let (w, h) = (f.width as usize, f.height as usize);
    if f.rows.len() != h {
        return Err(format!("{} rows for height {h}", f.rows.len()));
    }
    for (i, r) in f.rows.iter().enumerate() {
        if let Some(c) = r.text.chars().find(|&c| !sane(c)) {
            return Err(format!("row {i} holds raw {c:?}: {:?}", r.text));
        }
        if r.text.width() > w {
            return Err(format!(
                "row {i} is {} columns on a {w}-column screen: {:?}",
                r.text.width(),
                r.text
            ));
        }
    }
    if f.cursor.1 as usize >= h || f.cursor.0 as usize >= w {
        return Err(format!("cursor {:?} outside {w}x{h}", f.cursor));
    }
    // The cursor sits on the char it stands for, also in a line taller than the screen.
    let b = &a.buf;
    if a.vi.mode != Mode::Cmd && strict_width {
        let (c, prev) = (b.ch(b.cursor), b.cursor.checked_sub(1).map(|p| b.ch(p)));
        let joined =
            prev == Some('\u{200d}') || b.ch(b.cursor + 1) == '\u{fe0f}' && c.width() == Some(1);
        if !matches!(c, '\n' | '\0') && c.width() != Some(0) && !joined {
            let want = match c {
                '\t' => ' ',
                '\0'..='\x1f' | '\x7f' => '^',
                '\u{80}'..='\u{9f}'
                | '\u{61c}'
                | '\u{200b}'
                | '\u{200e}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}' => '<',
                c => c,
            };
            let row = &f.rows[f.cursor.1 as usize].text;
            let got = cell_at(row, f.cursor.0 as usize);
            // A tab's first cell is a space; any char of a wide tab is fine.
            if got != Some(want) {
                return Err(format!(
                    "cursor {:?} should be on {want:?} (buffer char {c:?}), the cell is {got:?} in row {row:?}",
                    f.cursor
                ));
            }
        }
    }
    Ok(())
}

fn run(bytes: &[u8], keys: &[&str], size: (u16, u16), path: &str) -> Result<(), String> {
    let mut a = app_with(Some(path), bytes, size);
    let orig = text(&a);
    let strict = size.0 >= 24;
    for k in keys {
        feed(&mut a, k);
        let b = &a.buf;
        let (l, n) = (b.line_of(b.cursor), b.text.len_chars());
        if b.cursor > n {
            return Err(format!("{k:?}: cursor {} past the end {n}", b.cursor));
        }
        if matches!(a.vi.mode, Mode::Normal | Mode::Visual | Mode::VLine)
            && !(b.cursor < b.end(l) || b.line_len(l) == 0)
        {
            return Err(format!(
                "{k:?}: Normal-mode cursor rests past the last char: {:?}",
                marked(&a)
            ));
        }
        if !a.selection().is_none_or(|(s, e)| s <= e && e <= n) {
            return Err(format!("{k:?}: bad selection {:?}", a.selection()));
        }
        let f = screen(&mut a);
        check_frame(&a, &f, strict)
            .map_err(|e| format!("after {k:?}: {e}\ntext {:?}", text(&a)))?;
    }
    // Leave everything and walk the undo history.
    feed(&mut a, "<Esc><Esc><Esc>");
    if a.vi.mode != Mode::Normal {
        return Err(format!("not in Normal mode after Esc: {:?}", a.vi.mode));
    }
    // The tip of the history, then all the way back and forth again.
    while a.buf.travel(false) {}
    let end = text(&a);
    let mut steps = 0;
    while a.buf.travel(true) {
        steps += 1;
        if steps > 10_000 {
            return Err("undo does not end".into());
        }
    }
    if text(&a) != orig {
        return Err(format!("undo all: {:?} != original {:?} (final {end:?})", text(&a), orig));
    }
    if a.buf.dirty() {
        return Err("undo all leaves the buffer dirty".into());
    }
    while a.buf.travel(false) {}
    if text(&a) != end {
        return Err(format!("redo all: {:?} != final {end:?}", text(&a)));
    }
    if !a.buf.dirty() && end != orig {
        return Err("redo all is clean but the text differs from the saved text".into());
    }
    // The bytes written decode to a buffer that writes the same bytes again.
    if a.buf.readonly.is_none() {
        let enc = a.buf.encode();
        let again = Buffer::decode(&enc);
        if again.readonly.is_some() {
            return Err(format!("what we write is not readable back: {enc:?}"));
        }
        if again.encode() != enc {
            return Err(format!("encode(decode(encode)) differs: {enc:?} vs {:?}", again.encode()));
        }
    }
    Ok(())
}

/// One command is one undo unit: walk the history back and forth and compare every state
/// (text and version) with the one seen when the editor was idle in Normal mode.
fn undo_granularity(bytes: &[u8], keys: &[&str]) -> Result<(), String> {
    let mut a = app_with(Some("f.rs"), bytes, (60, 12));
    let mut hist = vec![(a.buf.version(), text(&a))];
    for k in keys {
        feed(&mut a, k);
        if a.vi.idle() && a.buf.version() != hist.last().map_or(0, |h| h.0) {
            hist.push((a.buf.version(), text(&a)));
        }
    }
    feed(&mut a, "<Esc><Esc><Esc>");
    if a.buf.version() != hist.last().map_or(0, |h| h.0) {
        hist.push((a.buf.version(), text(&a)));
    }
    for (i, want) in hist.iter().enumerate().rev().skip(1) {
        if !a.buf.travel(true) {
            return Err(format!("undo ended early at step {i}: history {hist:?}"));
        }
        if (a.buf.version(), text(&a)) != *want {
            return Err(format!(
                "undo to step {i}: got {:?}, want {want:?}; history {hist:?}",
                (a.buf.version(), text(&a))
            ));
        }
    }
    if a.buf.travel(true) {
        return Err(format!("more undo units than commands: history {hist:?}"));
    }
    for (i, want) in hist.iter().enumerate().skip(1) {
        if !a.buf.travel(false) || (a.buf.version(), text(&a)) != *want {
            return Err(format!("redo to step {i} differs; history {hist:?}"));
        }
    }
    Ok(())
}

/// Complete commands only (each starts and ends idle in Normal mode, and is one undo unit), so
/// the model above is sound: motions, changes, whole insert sessions, Visual operations.
const COMPLETE: &[&str] = &[
    "h",
    "j",
    "k",
    "l",
    "w",
    "b",
    "e",
    "0",
    "$",
    "gg",
    "G",
    "fa",
    "ta",
    ";",
    "%",
    "x",
    "ra",
    "J",
    "3J",
    "p",
    "P",
    "2p",
    ".",
    "dd",
    "yy",
    "D",
    "Y",
    ">>",
    "<lt><lt>",
    "dw",
    "yw",
    "d$",
    "diw",
    "daw",
    "da(",
    "di\"",
    "d%",
    "dG",
    "dgg",
    "dj",
    "dk",
    "db",
    "de",
    "d2w",
    "9dd",
    "5x",
    "3>>",
    "ihello<Esc>",
    "Ax<Esc>",
    "Ix<Esc>",
    "ox<Esc>",
    "Oy<Esc>",
    "cwq<Esc>",
    "ciwq<Esc>",
    "ccz<Esc>",
    "Cq<Esc>",
    "ci(w<Esc>",
    "ox<CR>y<Esc>",
    "ia<BS>b<Esc>",
    "iab<C-w>c<Esc>",
    "ia<C-u>b<Esc>",
    "ia<CR>b<Left>c<Esc>",
    "viwd",
    "Vjd",
    "vlcq<Esc>",
    "ggVG>",
    "vey",
    "Vjy",
    "viw>",
    "vjjo<Esc>",
    "v$d",
    "VGd",
    "vi(d",
    ":s/a/b/g<CR>",
    ":%s/./x/<CR>",
    ":2,3s/(/[/<CR>",
    ":%s/$/!/<CR>",
    ":3<CR>",
    "/a<CR>",
    "n",
    "*",
    "<C-d>",
    "zz",
    "r\t",
    "i\t<Esc>",
    "i<Tab><Esc>",
    "o<Esc>",
    "O<Esc>",
    "cwz<Esc>",
];

fn counted() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        (prop::sample::select(BIG), prop::sample::select(WITH_COUNT), prop::bool::ANY),
        1..8,
    )
    .prop_map(|v| {
        v.into_iter()
            .map(|(n, c, plain)| if plain { c.to_string() } else { format!("{n}{c}") })
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(2000)) })]

    /// Undo walks back through exactly the states the commands made.
    #[test]
    fn vi_2_fuzz_undo_granularity(bytes in file_bytes(), keys in prop::collection::vec(prop::sample::select(COMPLETE), 0..25)) {
        if let Err(e) = undo_granularity(&bytes, &keys) {
            prop_assert!(false, "{e}\nbytes {bytes:?}\nkeys {keys:?}");
        }
    }

    /// Huge counts on every command that takes one: nothing panics, spins or allocates the world.
    #[test]
    fn vi_2_fuzz_huge_counts(bytes in file_bytes(), keys in counted(), w in 24u16..100, h in 2u16..14) {
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        // Seed the register so `p` has something to repeat.
        let mut a = app_with(Some("f.rs"), &bytes, (w, h));
        feed(&mut a, "yw");
        for k in &keys {
            feed(&mut a, k);
            let b = &a.buf;
            prop_assert!(b.cursor <= b.text.len_chars());
        }
        prop_assert!(a.buf.text.len_bytes() < (64 << 20), "a count made the text huge: {} bytes", a.buf.text.len_bytes());
    }

    /// No panic, cursor and frame invariants, undo/redo round trip. Code file, wide screen.
    #[test]
    fn vi_3_fuzz_keys(bytes in file_bytes(), keys in prop::collection::vec(prop::sample::select(TOKENS), 0..30), w in 24u16..100, h in 2u16..14) {
        if let Err(e) = run(&bytes, &keys, (w, h), "f.rs") {
            prop_assert!(false, "{e}\nbytes {bytes:?}\nkeys {keys:?} size {w}x{h}");
        }
    }

    /// The same on prose files (72-column wrapping) and tiny screens: only the no-panic and
    /// cursor-in-bounds invariants.
    #[test]
    fn vis_6_fuzz_keys_on_tiny_screens(bytes in file_bytes(), keys in prop::collection::vec(prop::sample::select(TOKENS), 0..30), w in 1u16..24, h in 2u16..8) {
        if let Err(e) = run(&bytes, &keys, (w, h), "f.md") {
            prop_assert!(false, "{e}\nbytes {bytes:?}\nkeys {keys:?} size {w}x{h}");
        }
    }

    /// Paste at any moment: never keys, always text, and the undo round trip still holds.
    #[test]
    fn vi_9_fuzz_paste_and_resize(bytes in file_bytes(), keys in prop::collection::vec(prop::sample::select(TOKENS), 0..20), pasted in gen_text(), at in 0usize..20, w in 24u16..100, h in 2u16..14) {
        let mut a = app_with(Some("f.rs"), &bytes, (w, h));
        let orig = text(&a);
        let rests = |a: &App| {
            let b = &a.buf;
            let l = b.line_of(b.cursor);
            b.cursor <= b.text.len_chars()
                && (a.vi.mode == Mode::Insert || a.vi.mode == Mode::Cmd || b.cursor < b.end(l) || b.line_len(l) == 0)
        };
        for (i, k) in keys.iter().enumerate() {
            if i == at {
                a.update(Event::Paste(pasted.clone()));
                prop_assert!(rests(&a), "paste {pasted:?} leaves the cursor off the text: {:?} {:?}", marked(&a), a.vi.mode);
                a.update(Event::Resize(w / 2 + 1, h));
                a.update(Event::Resize(w, h));
            }
            feed(&mut a, k);
        }
        a.update(Event::Paste(pasted.clone()));
        prop_assert!(rests(&a), "paste {pasted:?} leaves the cursor off the text: {:?} {:?}", marked(&a), a.vi.mode);
        feed(&mut a, "<Esc><Esc>");
        while a.buf.travel(true) {}
        prop_assert_eq!(text(&a), orig);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(20000)) })]

    /// SAFE-3 on arbitrary valid UTF-8, with the chars that matter.
    #[test]
    fn safe_3_fuzz_round_trip(bytes in file_bytes()) {
        let b = Buffer::decode(&bytes);
        prop_assert!(b.readonly.is_none() || bytes.contains(&0));
        if b.readonly.is_none() {
            prop_assert_eq!(b.encode(), bytes);
        }
    }

    /// And on arbitrary bytes: whatever is not read-only must round trip.
    #[test]
    fn safe_3_fuzz_round_trip_raw_bytes(bytes in prop::collection::vec(prop::sample::select(vec![b'a', b'\n', b'\r', 0xEF, 0xBB, 0xBF, 0xC3, 0xA9, 0xFF, b' ']), 0..24)) {
        let b = Buffer::decode(&bytes);
        if b.readonly.is_none() {
            prop_assert_eq!(b.encode(), bytes);
        }
    }
}

// ---- events: the world answers late and out of order --------------------------------------------

#[derive(Clone, Debug)]
enum Step {
    Keys(&'static str),
    /// The save thread answers (Ok, Err or conflict) for the write in flight.
    Saved(u8),
    /// The main loop answers a pending `:e`.
    Opened(&'static str),
    Paste(&'static str),
    Resize(u16, u16),
    /// The file on disk now holds this (the watcher's read).
    Disk(&'static str),
    /// The filter that was started (if any) ends: upper-cased, unchanged, empty, failed, killed, not
    /// text (FILT-2); with none running the answer comes unasked and must be ignored.
    Filter(u8),
    /// The place the file was left at, arriving after the file (CLI-6).
    Place(usize, usize),
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let keys = prop::sample::select(vec![
        "x",
        "ix<Esc>",
        "dd",
        "u",
        "<C-r>",
        "p",
        "yy",
        "o<Esc>",
        ".",
        "J",
        ":w<CR>",
        ":w!<CR>",
        ":wq<CR>",
        ":x<CR>",
        ":q<CR>",
        "ZZ",
        "q",
        "<C-s>",
        ":e<CR>",
        ":e!<CR>",
        ":e g<CR>",
        ":e! g<CR>",
        ":w g<CR>",
        ":wq g<CR>",
        ":s/a/b/<CR>",
        "gg",
        "G",
        "cwz<Esc>",
        "]c",
        "]c",
        "[c",
        ":%!f<CR>",
        ":.!f<CR>",
        "Vj!f<CR>",
        "<C-c>",
        "y",
        "n",
        ":q!<CR>",
        "ZQ",
    ]);
    prop::collection::vec(
        prop_oneof![
            4 => keys.prop_map(Step::Keys),
            2 => prop::sample::select(vec!["other\n", "from disk\n", "x\ny\n", "", "a\n"]).prop_map(Step::Disk),
            2 => (0u8..3).prop_map(Step::Saved),
            2 => prop::sample::select(vec!["f", "g"]).prop_map(Step::Opened),
            1 => prop::sample::select(vec!["pasted", "a\nb\n", ""]).prop_map(Step::Paste),
            1 => (1u16..60, 2u16..12).prop_map(|(w, h)| Step::Resize(w, h)),
            2 => (0u8..6).prop_map(Step::Filter),
            1 => (0usize..8, 0usize..8).prop_map(|(l, c)| Step::Place(l, c)),
        ],
        0..40,
    )
}

fn event_session(bytes: &[u8], steps: &[Step]) -> Result<(), String> {
    use sim_core::core::app::Effect;
    use sim_core::core::buffer::hash;
    let mut a = app_with(Some("f"), bytes, (40, 8));
    let mut writes: Vec<(String, Vec<u8>)> = vec![]; // in flight
    let mut opens: Vec<(String, u64)> = vec![]; // asked, with the version that asked
    let mut ran: Option<Vec<u8>> = None; // the input of the filter that was started
    let mut loaded_clean = false; // the buffer was just loaded and not touched since
    let check = |a: &mut App,
                 fx: &[Effect],
                 what: &str,
                 writes: &mut Vec<(String, Vec<u8>)>,
                 opens: &mut Vec<(String, u64)>,
                 discarded: bool|
     -> Result<(), String> {
        for e in fx {
            match e {
                Effect::Write { path, bytes, .. } => writes.push((path.clone(), bytes.clone())),
                Effect::Open(p) => opens.push((p.clone(), a.buf.version())),
                Effect::Quit(0) if a.buf.dirty() && !discarded => {
                    return Err(format!("{what}: Quit(0) with unsaved changes"));
                }
                _ => {}
            }
        }
        if !a.buf.dirty() && a.buf.text != a.buf.base {
            return Err(format!(
                "{what}: clean, but text {:?} != saved text {:?}",
                text(a),
                a.buf.base.to_string()
            ));
        }
        Ok(())
    };
    for st in steps {
        let what = format!("{st:?}");
        let discard = matches!(st, Step::Keys(":q!<CR>" | "ZQ"));
        let fx = match st {
            Step::Keys(k) => {
                loaded_clean = false;
                feed(&mut a, k)
            }
            Step::Saved(n) => match writes.first().cloned() {
                None => a.update(Event::Saved(Ok((1, None)), 0)), // nobody asked: must be ignored
                Some((_, bytes)) => {
                    writes.remove(0);
                    match n {
                        0 => a.update(Event::Saved(Ok((hash(&bytes), None)), 0)),
                        1 => a.update(Event::Saved(Err("disk full".into()), 0)),
                        _ => a.update(Event::SaveConflict(Some(b"other\n".to_vec()), far(), 0)),
                    }
                }
            },
            Step::Opened(p) => {
                // Only a path that was asked for arrives.
                let Some(i) = opens.iter().position(|(q, _)| q == p) else { continue };
                let (_, asked_at) = opens.remove(i);
                let before = (text(&a), a.path.clone());
                let fx = a.update(Event::Opened((*p).into(), Ok(Some(b"from disk\n".to_vec())), 0));
                if a.buf.version() != asked_at && before.0 != text(&a) && a.buf.dirty() {
                    return Err(format!(
                        "{what}: dropped edits made since :e: {:?} -> {:?}",
                        before.0,
                        text(&a)
                    ));
                }
                if text(&a) == "from disk"
                    && a.buf.version() == 0
                    && !a.msg.as_deref().is_some_and(|m| m.starts_with("not opened"))
                {
                    loaded_clean = true;
                    writes.clear(); // the old buffer's save no longer concerns this one
                }
                if before.0 != "from disk" && text(&a) != "from disk" && a.path != before.1 {
                    return Err(format!("{what}: path changed without loading"));
                }
                fx
            }
            Step::Paste(s) => {
                loaded_clean = false;
                a.update(Event::Paste((*s).into()))
            }
            Step::Resize(w, h) => a.update(Event::Resize(*w, *h)),
            Step::Disk(t) => {
                loaded_clean = false;
                a.update(Event::Disk {
                    bytes: Some(t.as_bytes().to_vec()),
                    hash: None,
                    deadline: far(),
                    seq: 0,
                })
            }
            Step::Filter(k) => {
                let input = String::from_utf8_lossy(&ran.take().unwrap_or_default()).to_string();
                let (version, before, marks) = (a.buf.version(), text(&a), a.buf.marks.len());
                let up = input.to_uppercase();
                let (status, out): (_, Vec<u8>) = match k {
                    0 => (Some(0), up.into()),
                    1 => (Some(0), input.into()),
                    2 => (Some(0), vec![]),
                    3 => (Some(2), b"changed\n".to_vec()),
                    4 => (None, vec![]),
                    _ => (Some(0), vec![0xff, b'\n']),
                };
                let fx = a.update(Event::FilterDone(status, out, b"err\n".to_vec(), far()));
                // A failed, cancelled or unusable result changes nothing; a good one may (FILT-2).
                let same =
                    (text(&a), a.buf.version(), a.buf.marks.len()) == (before, version, marks);
                if *k >= 3 && !same {
                    return Err(format!("{what}: a filter's result changed the text"));
                }
                fx
            }
            Step::Place(l, c) => {
                let (cursor, version, before) = (a.buf.cursor, a.buf.version(), text(&a));
                let fx = a.update(Event::Place(a.path.clone().unwrap_or_default(), *l, *c));
                if (text(&a), a.buf.version()) != (before, version) {
                    return Err(format!("{what}: a place changed the text"));
                }
                if cursor != 0 && a.buf.cursor != cursor {
                    return Err(format!("{what}: a place moved a cursor that had moved"));
                }
                fx
            }
        };
        for e in &fx {
            if let Effect::Spawn { input, .. } = e {
                ran = Some(input.clone());
            }
        }
        check(&mut a, &fx, &what, &mut writes, &mut opens, discard)?;
        if loaded_clean && a.buf.dirty() {
            return Err(format!("{what}: a buffer just read from disk reads as modified"));
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(3000)) })]

    /// Saved / SaveConflict / Opened arriving late, out of order or unasked: clean means the text
    /// is the saved text, no `Quit(0)` while modified, and a file never replaces newer edits.
    #[test]
    fn vi_7_fuzz_late_answers(bytes in file_bytes(), steps in steps()) {
        if let Err(e) = event_session(&bytes, &steps) {
            prop_assert!(false, "{e}\nbytes {bytes:?}\nsteps {steps:?}");
        }
    }
}

// ---- filters (FILT-2) -----------------------------------------------------------------------------

fn small_lines(max: usize) -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec("[a-c ]{0,4}", 0..max)
}

/// The output of a filter that prints these lines, with or without a final line break.
fn printed(out: &[String], final_nl: bool) -> String {
    let mut s: String = out.iter().map(|l| format!("{l}\n")).collect();
    if !final_nl && out.last().is_some_and(|l| !l.is_empty()) {
        s.pop();
    }
    s
}

fn filtered(lines: &[String], lo: usize, hi: usize, out: &str) -> App {
    let mut a = app_with(Some("f"), format!("{}\n", lines.join("\n")).as_bytes(), (40, 10));
    let fx = feed(&mut a, &format!(":{},{}!f<CR>", lo + 1, hi + 1));
    let input = fx.iter().find_map(|e| match e {
        sim_core::core::app::Effect::Spawn { input, .. } => {
            Some(String::from_utf8_lossy(input).to_string())
        }
        _ => None,
    });
    assert_eq!(input, Some(format!("{}\n", lines[lo..=hi].join("\n"))));
    let (stdout, stderr) = (out.as_bytes().to_vec(), vec![]);
    a.update(Event::FilterDone(Some(0), stdout, stderr, far()));
    a
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(3000)) })]

    /// With no edit in between, the output takes the place of the lines of the range, as one undo
    /// step: `u` gives the buffer back, clean.
    #[test]
    fn filt_2_fuzz_the_output_is_the_range_replaced_and_undo_gives_the_buffer_back(
        lines in small_lines(8).prop_filter("a line", |l| !l.is_empty()),
        (x, y) in (0usize..8, 0usize..8),
        out in small_lines(8),
        final_nl in any::<bool>(),
    ) {
        let (lo, hi) = (x.min(y).min(lines.len() - 1), x.max(y).min(lines.len() - 1));
        let out_text = printed(&out, final_nl);
        let mut want = lines[..lo].to_vec();
        want.extend(out.iter().cloned());
        want.extend(lines[hi + 1..].iter().cloned());
        let before = lines.join("\n");
        let mut a = filtered(&lines, lo, hi, &out_text);
        let changed = want.join("\n") != before;
        prop_assert_eq!(text(&a), want.join("\n"));
        prop_assert_eq!(a.buf.dirty(), changed, "modified iff the output differs from the lines");
        if changed {
            feed(&mut a, "u");
            prop_assert_eq!((text(&a), a.buf.dirty()), (before, false), "u gives the buffer back");
        }
    }

    /// With edits in between the output is applied whole (every line of it is in the text, and
    /// `u` gives mine back) or not at all (mine is as it was, and nothing is lost).
    #[test]
    fn filt_2_fuzz_edits_meanwhile_are_kept_and_the_output_comes_in_whole_or_not_at_all(
        lines in small_lines(8).prop_filter("a line", |l| !l.is_empty()),
        (x, y) in (0usize..8, 0usize..8),
        out in small_lines(8),
        edits in prop::collection::vec(prop::sample::select(vec!["x", "dd", "Onew<Esc>", "A!<Esc>", "yyp", "jj", "gg", "G", "J", "ox<Esc>", "u", "cwq<Esc>"]), 0..6),
    ) {
        let (lo, hi) = (x.min(y).min(lines.len() - 1), x.max(y).min(lines.len() - 1));
        let mut a = app_with(Some("f"), format!("{}\n", lines.join("\n")).as_bytes(), (40, 10));
        feed(&mut a, &format!(":{},{}!f<CR>", lo + 1, hi + 1));
        for e in &edits {
            feed(&mut a, e);
        }
        feed(&mut a, "<Esc>");
        let mine = text(&a);
        let out_text = printed(&out, true);
        a.update(Event::FilterDone(Some(0), out_text.into(), vec![], far()));
        let after = text(&a);
        if after != mine {
            let have: Vec<&str> = after.split('\n').collect();
            let input = &lines[lo..=hi];
            for l in out.iter().filter(|l| !input.contains(l)) {
                prop_assert!(have.contains(&l.as_str()), "{l:?} of the output is lost: {after:?}");
            }
            feed(&mut a, "u");
            prop_assert_eq!(text(&a), mine, "u gives mine back");
        }
    }
}

// ---- the command line ----------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(5000)) })]

    /// Editing the command line: the position stays inside, the cursor is drawn after the
    /// chars before it, and pasting never leaves a newline in it.
    #[test]
    fn vi_12_fuzz_command_line_editing(
        keys in prop::collection::vec(prop::sample::select(vec![
            "a", "日", "\t", "é", " ", "\\", "<Left>", "<Right>", "<Home>", "<End>", "<BS>", "<Del>", "<Tab>", "\u{202e}", "\x01", "😀",
        ]), 0..30),
        opener in prop::sample::select(vec![":", "/", "?", "v:", "V:", "d!"]),
        w in 10u16..60,
        pasted in gen_text(),
    ) {
        // (How a variation selector or a ZWJ changes widths is tested with the view.)
        let pasted = pasted.replace(['\u{fe0f}', '\u{200d}'], "");
        let mut a = app_with(Some("f.rs"), b"a b\nc d\n", (w, 6));
        feed(&mut a, opener);
        if a.vi.mode != Mode::Cmd {
            return Ok(());
        }
        for (i, k) in keys.iter().enumerate() {
            if i == keys.len() / 2 {
                a.update(Event::Paste(pasted.clone()));
            }
            feed(&mut a, k);
            if a.vi.mode != Mode::Cmd {
                break;
            }
            let n = a.vi.line.chars().count();
            prop_assert!(a.vi.pos >= 1 && a.vi.pos <= n, "pos {} of {n}: {:?}", a.vi.pos, a.vi.line);
            prop_assert!(!a.vi.line.contains(['\n', '\r']), "{:?}", a.vi.line);
            let f = screen(&mut a);
            let wd = |c: char| match c {
                '\t' => 4,
                '\0'..='\x1f' | '\x7f' => 2,
                '\u{80}'..='\u{9f}' | '\u{61c}' | '\u{200b}' | '\u{200e}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}' => 8,
                c => c.width().unwrap_or(0),
            };
            // The line scrolls: from its `off`-th char on, the cursor stays on the screen, and
            // the cursor is after the chars between the two.
            let (off, pos) = (a.vi.off, a.vi.pos);
            prop_assert!(off < pos, "off {off} pos {pos}: the char before the cursor stays");
            let want: usize = a.vi.line.chars().skip(off).take(pos - off).map(wd).sum();
            prop_assert!(want < w as usize, "cursor col {want} on a {w}-column screen");
            prop_assert!(f.rows[5].text.width() <= w as usize, "{:?}", f.rows[5].text);
            if !a.vi.line.contains('\t') {
                prop_assert_eq!(f.cursor.0 as usize, want, "line {:?} pos {} off {}", a.vi.line, pos, off);
                let shown: String = a.vi.line.chars().skip(off).collect();
                prop_assert!(shown.starts_with(&f.rows[5].text.chars().take(1).collect::<String>()) || a.vi.line.chars().skip(off).any(|c| wd(c) != 1));
            }
            prop_assert_eq!(f.cursor.1 as usize, 5);
        }
    }
}

// ---- marks: any mark, drawn under any keys ------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::with_cases(cases(1500)) })]

    /// Marks that no longer fit the text (made-up columns that overrun it or run backwards, as a
    /// stale diff could) are drawn on any screen: nothing panics or spins, no row is wider than the
    /// screen, nothing dangerous reaches it raw, and the cursor stays on the screen.
    #[test]
    fn vis_3_fuzz_marked_frames_fit_any_screen(
        bytes in file_bytes(),
        specs in prop::collection::vec((0usize..6, prop::sample::select(vec!['+', '~', '-']), prop::option::of(prop::collection::vec((0usize..90, 0usize..90), 0..4))), 1..5),
        keys in prop::collection::vec(prop::sample::select(vec![
            "]c", "[c", "j", "k", "gg", "G", "u", "<C-r>", "<Esc>", "x", "dd", "p", "o<Esc>", "zz",
            "<C-d>", "<C-u>", "J", "V", "i", "ix<Esc>", ":s/a/b/<CR>", "]c]c", "[c[c",
        ]), 0..25),
        w in 1u16..60, h in 2u16..14,
    ) {
        let mut a = app_with(Some("f.rs"), &bytes, (w, h));
        let n = a.buf.lines();
        let mut ms: Vec<_> = specs.iter().map(|(l, k, ws)| {
            let ws = ws.as_ref().map(|v| v.iter().map(|&(x, y)| x..y).collect::<Vec<_>>());
            mark(*l.min(&(n - 1)), *k, ws.unwrap_or_default())
        }).collect();
        ms.sort_by_key(|m| m.line);
        ms.dedup_by_key(|m| m.line);
        a.buf.marks = std::sync::Arc::new(ms);
        screen(&mut a);
        for k in &keys {
            feed(&mut a, k);
            let f = screen(&mut a);
            if let Err(e) = check_frame(&a, &f, false) {
                prop_assert!(false, "after {k:?}: {e}\ntext {:?}", text(&a));
            }
            prop_assert!(a.buf.cursor <= a.buf.text.len_chars());
        }
    }
}
