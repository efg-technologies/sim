//! DISK-6: marks under every kind of edit, against a line-diff reference.
//! Lines have unique text, so a line diff of the text before and after one key says which lines
//! "stayed" (Equal) and where they went. A mark stays on a line the key left as it was and goes
//! from a line it changed (and from the line the cursor ends on: `see()`).
#![allow(
    clippy::cmp_owned,
    clippy::type_complexity,
    clippy::manual_contains,
    clippy::needless_range_loop,
    clippy::unwrap_used,
    clippy::single_range_in_vec_init
)]
mod common;
use common::*;
use proptest::prelude::*;
use sim_core::core::app::{App, Event, Key};
use sim_core::core::buffer::Mark;
use similar::{Algorithm, DiffOp, capture_diff_slices};
use std::collections::HashSet;

const ATOMS: &[&str] = &[
    "dd",
    "2dd",
    "3dd",
    "x",
    "dw",
    "J",
    "3J",
    "A!<Esc>",
    "I!<Esc>",
    "cc!<Esc>",
    ">>",
    "<<",
    "Vjd",
    "Vjjd",
    "VGd",
    "ggdG",
    "dG",
    "dgg",
    "Gdd",
    "ggdd",
    "dj",
    "dk",
    "Vjc!<Esc>",
    "vjd",
    "vjjd",
    "vld",
    ":s/line/ln/\n",
    ":%s/abc/X/\n",
    ":1,3s/-/+/g\n",
    "i\n<Esc>",
    "a\n<Esc>",
    "A\n<Esc>",
    "I\n<Esc>",
    "i<BS><Esc>",
    "A<Del><Esc>",
    "ia\nb<Esc>",
    "A<C-w><Esc>",
    "i<C-u><Esc>",
    "C!<Esc>",
    "$x",
    "0x",
    "ggx",
    "Gx",
    "xxxx",
    "dd>>",
    "Y",
    "Jx",
    "A<CR>q<CR>r<Esc>",
    "I<CR><CR><Esc>",
    "gg<CR>",
    "O<Esc>",
    "o<Esc>",
    ":%d\n",
    ":2,3s/l/L/\n",
];

#[derive(Clone, Debug)]
enum Step {
    Move(&'static str),
    Edit(&'static str),
    Open(bool),
    Paste(&'static str),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => prop::sample::select(vec!["j", "k", "gg", "G", "jj", "kk", "$", "0", "w", "l", "h", "V<Esc>", "<Esc>"]).prop_map(Step::Move),
        9 => prop::sample::select(ATOMS.to_vec()).prop_map(Step::Edit),
        1 => any::<bool>().prop_map(Step::Open),
        1 => prop::sample::select(vec!["p\nq", "p\n", "\np", "p", "\n", "p\nq\nr\n"]).prop_map(Step::Paste),
    ]
}

/// The mark's own number: the first column it paints, which `setup` makes the line it was put on.
fn identity(m: &Mark) -> usize {
    words(m).map_or(usize::MAX, |w| w[0].start)
}

fn lines(a: &App) -> Vec<String> {
    text(a).split('\n').map(String::from).collect()
}

/// Old line index -> new line index, for the lines the diff keeps.
fn kept(old: &[String], new: &[String]) -> Vec<Option<usize>> {
    let mut out = vec![None; old.len()];
    for op in capture_diff_slices(Algorithm::Myers, old, new) {
        if let DiffOp::Equal { old_index, new_index, len } = op {
            for k in 0..len {
                out[old_index + k] = Some(new_index + k);
            }
        }
    }
    out
}

fn unique(v: &[String]) -> bool {
    // An empty line has no identity either: a diff may match any empty line with any other.
    let s: HashSet<&String> = v.iter().collect();
    s.len() == v.len() && !v.iter().any(|l| l.trim().is_empty())
}

fn setup(n: usize, marks: &[bool]) -> App {
    let src: String =
        (0..n).map(|i| format!("line-{i}-abcdef-{}\n", char::from(b'p' + i as u8))).collect();
    let mut a = app_with(Some("f"), src.as_bytes(), (80, 12));
    a.buf.marks = (0..n)
        .filter(|i| marks.get(*i).copied().unwrap_or(true))
        .map(|line| mark(line, '~', vec![line..line + 1]))
        .collect::<Vec<_>>()
        .into();
    a
}

/// One event, with the check of what it did to the marks.
fn event(a: &mut App, ev: Event, what: &str) -> Result<(), TestCaseError> {
    let (old, pre_marks) = (lines(a), a.buf.marks.to_vec());
    a.update(ev);
    let new = lines(a);
    if !unique(&old) || !unique(&new) {
        return Ok(());
    }
    let k = kept(&old, &new);
    let cl = a.buf.line_of(a.buf.cursor);
    // DISK-6: the marks are exactly those of the lines that stayed, minus the cursor's line.
    let want: Vec<(usize, usize)> = pre_marks
        .iter()
        .filter_map(|m| k[m.line].map(|j| (j, identity(m))))
        .filter(|(j, _)| *j != cl)
        .collect();
    let got: Vec<(usize, usize)> = a.buf.marks.iter().map(|m| (m.line, identity(m))).collect();
    prop_assert_eq!(
        &got,
        &want,
        "marks after {} on {:?} -> {:?} (cursor on line {})",
        what,
        old,
        new,
        cl
    );
    Ok(())
}

fn press(a: &mut App, ks: &str) -> Result<(), TestCaseError> {
    for k in keys(ks) {
        event(a, Event::Key(k), &format!("{ks:?} at {k:?}"))?;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(1000), max_shrink_iters: 3000, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn disk_6_fuzz_shift_marks_follow_their_lines(
        n in 1usize..8,
        marks in prop::collection::vec(any::<bool>(), 8),
        steps in prop::collection::vec(step(), 1..10),
    ) {
        let mut a = setup(n, &marks);
        let mut tok = 0;
        for st in &steps {
            match st {
                Step::Move(k) | Step::Edit(k) => press(&mut a, k)?,
                Step::Open(below) => {
                    tok += 1;
                    let k = format!("{}Z{tok}<Esc>", if *below { "o" } else { "O" });
                    press(&mut a, &k)?;
                }
                Step::Paste(s) => event(&mut a, Event::Paste((*s).into()), &format!("paste {s:?}"))?,
            }
            drop(screen(&mut a));
        }
        let _ = Key::Esc;
    }
}

// ---------------------------------------------------------------------------------------------
// Every `Buffer::replace(range, with)` over small texts that have empty lines, against a
// char-level oracle of which lines survive unchanged. (tests/disk.rs checks texts without
// empty lines only, where `start == end` of a line never happens.)
//
// The oracle is the text: the lines with text are distinct (and so are the texts the edits bring
// in), so an old line survives iff its text is still a line of the new text, and that line is
// where it went. An empty line has no identity and is not followed.
// ---------------------------------------------------------------------------------------------

use sim_core::core::buffer::Buffer;

/// (a, b) of every line of `t`.
fn spans(t: &[char]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut a = 0;
    for (i, c) in t.iter().enumerate() {
        if *c == '\n' {
            out.push((a, i));
            a = i + 1;
        }
    }
    out.push((a, t.len()));
    out
}

const TEXTS: &[&str] = &[
    "",
    "a",
    "\n",
    "a\n",
    "\na",
    "\n\n",
    "a\nb",
    "a\n\nb",
    "a\n\n",
    "\n\na",
    "ab\ncd\n\nef",
    "a\nb\nc",
    "\na\n",
    "a\n\n\nb",
];
const WITHS: &[&str] = &["", "\n", "x", "x\n", "\nx", "\n\n", "x\ny", "\nx\n", "xy", "\n\nx"];

#[test]
fn disk_6_every_replace_over_texts_with_empty_lines_shifts_marks() {
    let mut bad: Vec<String> = vec![];
    let mut cases = 0usize;
    for t in TEXTS {
        let chars: Vec<char> = t.chars().collect();
        let sp = spans(&chars);
        {
            for s in 0..=chars.len() {
                for e in s..=chars.len() {
                    for w in WITHS {
                        cases += 1;
                        let mut b = Buffer::decode(format!("{t}\n").as_bytes());
                        assert_eq!(b.text.to_string(), *t);
                        b.marks = (0..sp.len())
                            .map(|i| mark(i, '~', vec![i..i + 1]))
                            .collect::<Vec<_>>()
                            .into();
                        if s == e && w.is_empty() {
                            continue;
                        }
                        b.replace(s..e, w);
                        let new: Vec<char> = chars[..s]
                            .iter()
                            .copied()
                            .chain(w.chars())
                            .chain(chars[e..].iter().copied())
                            .collect();
                        if b.text.to_string() != new.iter().collect::<String>() {
                            continue; // a no-op edit (nothing to replace with nothing)
                        }
                        let nsp = spans(&new);
                        // Where a surviving old line starts in the new text, hence its new index.
                        let new_lines: Vec<String> =
                            nsp.iter().map(|&(a, z)| new[a..z].iter().collect()).collect();
                        let surv: Vec<Option<usize>> = sp
                            .iter()
                            .map(|&(a, z)| {
                                let l: String = chars[a..z].iter().collect();
                                (z > a).then(|| new_lines.iter().position(|n| *n == l)).flatten()
                            })
                            .collect();
                        if b.marks.windows(2).any(|w| w[0].line >= w[1].line)
                            || b.marks.iter().any(|m| m.line >= nsp.len())
                        {
                            let l: Vec<_> = b.marks.iter().map(|m| m.line).collect();
                            bad.push(format!("MARKS NOT SORTED/UNIQUE/IN RANGE: text {t:?}, replace {s}..{e} by {w:?}: {l:?} in {} lines", nsp.len()));
                        }
                        let want: Vec<usize> = surv.iter().flatten().copied().collect();
                        let got: Vec<usize> = b
                            .marks
                            .iter()
                            .map(|m| m.line)
                            .filter(|&l| nsp[l].1 > nsp[l].0)
                            .collect();
                        let ctx = format!("text {t:?}, replace {s}..{e} by {w:?}");
                        if got != want {
                            bad.push(format!("MARKS {ctx}: got lines {got:?}, want {want:?}"));
                            continue;
                        }
                    }
                }
            }
        }
    }
    eprintln!("{cases} cases, {} bad", bad.len());
    bad.sort();
    bad.dedup();
    for m in bad.iter().take(40) {
        eprintln!("{m}");
    }
    assert!(bad.is_empty(), "{} defects, first: {}", bad.len(), bad[0]);
}
