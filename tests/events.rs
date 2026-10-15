//! PROP-2, VI-2: every kind of event, in any order, with awkward text. Nothing may panic (tests run
//! in debug: overflow checks are on), and the marks and hunks stay well formed; and `.` never
//! recurses without end. `SI_FUZZ_CASES` sets the number of cases.
#![allow(
    clippy::cmp_owned,
    clippy::type_complexity,
    clippy::manual_contains,
    clippy::unwrap_used,
    clippy::single_range_in_vec_init
)]
mod common;
use common::*;
use proptest::prelude::*;
use si::core::app::{App, Effect, Event, Key};
use si::core::buffer::hash;
use std::time::Instant;

/// Text built from pieces that make merges, wrapping and encodings awkward.
fn piece() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "a", "b", "c", "a b", "\n", "\n", "\n", "\r\n", "\r", "\t", "日本", "é", "e\u{301}",
        "\u{feff}", "\u{202e}", " ", "  ", "x_y", "\u{200b}", "😀", "z",
    ])
    .prop_map(String::from)
}

fn text_s() -> impl Strategy<Value = String> {
    prop::collection::vec(piece(), 0..14).prop_map(|v| v.concat())
}

fn bytes_s() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        12 => text_s().prop_map(String::into_bytes),
        1 => prop::collection::vec(any::<u8>(), 0..24),
        1 => text_s().prop_map(|s| {
            let mut b = s.into_bytes();
            b.push(0xff);
            b
        }),
        1 => text_s().prop_map(|s| {
            let mut b = vec![0xEF, 0xBB, 0xBF];
            b.extend(s.replace('\n', "\r\n").into_bytes());
            b
        }),
    ]
}

fn key_s() -> impl Strategy<Value = Key> {
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 :/?!$^%.,;'\"<>(){}[]#*&-_+=~日\u{ff41}".chars().collect();
    prop_oneof![
        30 => prop::sample::select(chars).prop_map(Key::Char),
        3 => prop::sample::select(vec!['c', 'r', 's', 'w', 'u', 'd', 'h', 'v', '[', ']', 'z']).prop_map(Key::Ctrl),
        4 => Just(Key::Esc),
        4 => Just(Key::Enter),
        2 => Just(Key::Backspace),
        1 => Just(Key::Tab),
        1 => Just(Key::Delete),
        1 => Just(Key::Up),
        1 => Just(Key::Down),
        1 => Just(Key::Left),
        1 => Just(Key::Right),
        1 => Just(Key::Home),
        1 => Just(Key::End),
        1 => Just(Key::PageUp),
        1 => Just(Key::PageDown),
    ]
}

#[derive(Clone, Debug)]
enum Ev {
    Key(Key),
    Keys(String),
    Paste(String),
    Resize(u16, u16),
    SavedOk(bool, bool, u64),
    SavedErr(u64),
    Conflict(Option<Vec<u8>>, bool, u64),
    Disk(Option<Vec<u8>>, bool, u64),
    Notice,
    Opened(Option<Vec<u8>>, bool, u64),
    Tick(u64),
    Signal,
}

/// The number of a read of the disk (PROP-1): a few, so that older and newer ones meet.
fn seq_s() -> impl Strategy<Value = u64> {
    0u64..8
}

fn ev() -> impl Strategy<Value = Ev> {
    prop_oneof![
        30 => key_s().prop_map(Ev::Key),
        10 => prop::sample::select(vec![
            "dd", "x", "ox\x1b", "Ox\x1b", "ggdG", "p", "P", "yyp", ":w\n", ":w!\n", ":wq\n", ":e!\n",
            "u", "\x12", "J", "V", "v", "jd", ":s/a/b/g\n", "ZZ", "gg", "G", "%", "ciwX\x1b", ":%d\n",
            "A\n\x1b", ".", "3.", "q", ":seen\n", ":5\n",
        ]).prop_map(|s| Ev::Keys(s.into())),
        3 => text_s().prop_map(Ev::Paste),
        1 => (0u16..30, 0u16..10).prop_map(|(w, h)| Ev::Resize(w, h)),
        4 => (any::<bool>(), any::<bool>(), seq_s()).prop_map(|(a, b, n)| Ev::SavedOk(a, b, n)),
        1 => seq_s().prop_map(Ev::SavedErr),
        4 => (prop::option::of(bytes_s()), any::<bool>(), seq_s()).prop_map(|(b, p, n)| Ev::Conflict(b, p, n)),
        12 => (prop::option::of(bytes_s()), any::<bool>(), seq_s()).prop_map(|(b, p, n)| Ev::Disk(b, p, n)),
        1 => Just(Ev::Notice),
        2 => (prop::option::of(bytes_s()), any::<bool>(), seq_s()).prop_map(|(b, e, n)| Ev::Opened(b, e, n)),
        2 => (0u64..100_000).prop_map(Ev::Tick),
        1 => Just(Ev::Signal),
    ]
}

fn deadline(past: bool) -> Instant {
    if past { Instant::now() } else { far() }
}

fn check(a: &mut App) -> Result<(), TestCaseError> {
    let b = &a.buf;
    let mut last = None;
    for m in b.marks.iter() {
        prop_assert!(
            last.is_none_or(|l| l < m.line) && m.line < b.lines(),
            "marks {:?} in {} lines",
            b.marks,
            b.lines()
        );
        last = Some(m.line);
    }
    let mut prev = 0;
    for h in b.hunks.iter() {
        prop_assert!(
            h.mine.start <= h.mine.end && h.mine.end <= b.lines() && prev <= h.mine.start,
            "hunks {:?} in {} lines",
            b.hunks.iter().map(|h| h.mine.clone()).collect::<Vec<_>>(),
            b.lines()
        );
        prev = h.mine.start;
    }
    prop_assert!(b.cursor <= b.text.len_chars());
    prop_assert!(b.lines() >= 1);
    drop(screen(a));
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(500), max_shrink_iters: 5000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn prop_2_fuzz_no_panic_whatever_the_events(
        named in any::<bool>(),
        start in bytes_s(),
        w in 1u16..60, h in 2u16..14,
        recover in prop::option::of((bytes_s(), bytes_s(), 0u64..200_000_000, any::<bool>())),
        evs in prop::collection::vec(ev(), 1..40),
    ) {
        if let Some(path) = std::env::var_os("SI_FUZZ_LOG") {
            // A stack overflow aborts the process: leave the case behind to be replayed.
            std::fs::write(path, format!("{named} {start:?} {w} {h} {recover:?}\n{evs:#?}")).unwrap();
        }
        let mut a = if named { app_with(Some("f"), &start, (w, h)) } else { app_with(None, &start, (w, h)) };
        let mut inflight: Option<Vec<u8>> = None;
        // Once, first: what the shell does at startup.
        if let Some((b, t, ms, p)) = recover {
            a.update(Event::Recover(b, t, ms, "snap".into(), deadline(p)));
            check(&mut a)?;
        }
        for e in evs {
            let real = match &e {
                Ev::SavedOk(true, ..) => inflight.clone(),
                _ => None,
            };
            let event = match e {
                Ev::Key(k) => Event::Key(k),
                Ev::Keys(s) => {
                    for k in keys(&s) {
                        for e in a.update(Event::Key(k)) {
                            if let Effect::Write { bytes, .. } = e {
                                inflight = Some(bytes);
                            }
                        }
                    }
                    check(&mut a)?;
                    continue;
                }
                Ev::Paste(s) => Event::Paste(s),
                Ev::Resize(w, h) => Event::Resize(w, h),
                Ev::SavedOk(real_hash, warn, n) => {
                    let h = match (&real, real_hash) { (Some(b), true) => hash(b), _ => 7 };
                    Event::Saved(Ok((h, warn.then(|| "warning".to_string()))), n)
                }
                Ev::SavedErr(n) => Event::Saved(Err("disk full".into()), n),
                Ev::Conflict(b, p, n) => Event::SaveConflict(b, deadline(p), n),
                Ev::Disk(b, p, n) => Event::Disk { bytes: b, hash: None, deadline: deadline(p), seq: n },
                Ev::Notice => Event::Notice("n".into()),
                Ev::Opened(b, e, n) => Event::Opened("f".into(), if e { Err("boom".into()) } else { Ok(b) }, n),
                Ev::Tick(t) => Event::Tick(t),
                Ev::Signal => Event::Signal(15),
            };
            for e in a.update(event) {
                if let Effect::Write { bytes, .. } = e {
                    inflight = Some(bytes);
                }
            }
            check(&mut a)?;
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(500), failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn vi_2_fuzz_dot_never_overflows_the_stack(ops in prop::collection::vec(prop::sample::select(vec![
        "x", "dd", ".", "i", "a", "\x1b", "cw", "V", "v", "u", "<C-r>", "j", "3", "p", "o", "J", "S",
        "ix", "Ax", ":e!\n", "OPENED",
    ]), 1..14)) {
        if let Some(path) = std::env::var_os("SI_FUZZ_LOG") {
            std::fs::write(path, format!("{ops:?}")).unwrap();
        }
        let mut a = app_with(Some("f"), b"a b c\nd e f\ng h i\n", (80, 12));
        for o in ops {
            if o == "OPENED" {
                a.update(Event::Opened("f".into(), Ok(Some(b"a b c\nd e f\ng h i\n".to_vec())), 0));
            } else {
                feed(&mut a, o);
            }
        }
    }
}
