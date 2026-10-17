//! KEY-1: the key table is the one source of which-key and the help. Its lines are well formed, and
//! the keys it lists are exactly the keys that do something: every key, every second key after each
//! prefix and every third after `i` and `a` after an operator is fed to a Sim in a state in which
//! any bound key changes something, and what changes something is what the table has for Normal mode.
#![allow(clippy::unwrap_used, clippy::panic)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::keys::{Entry, table};
use sim::layout::Skin;
use sim::paint::Screen;
use sim::sim::SimEvent;
use sim_core::core::app::{Event, Key};
use std::collections::BTreeSet;

#[test]
fn key_1_every_line_of_the_table_is_well_formed() {
    let text = include_str!("../keys.tsv");
    assert!(text.ends_with('\n'));
    let names =
        "Esc CR BS Tab S-Tab Space Del Up Down Left Right Home End PageUp PageDown lt Bar F1";
    let mut seen = BTreeSet::new();
    for (n, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 4, "line {}: four fields separated by tabs: {line:?}", n + 1);
        assert!(
            f.iter().all(|x| !x.trim().is_empty() && x.trim() == *x),
            "line {}: {line:?}",
            n + 1
        );
        assert!(f[1].chars().all(|c| "nvoictp".contains(c)), "line {}: modes {:?}", n + 1, f[1]);
        assert!(f[2].chars().count() <= 40, "line {}: a one-line description: {:?}", n + 1, f[2]);
        assert!(
            f[3].chars().count() >= 15 && f[3].len() > f[2].len(),
            "line {}: a longer text: {:?}",
            n + 1,
            f[3]
        );
        for token in f[0].split(' ').flat_map(|t| t.split('|')) {
            let named = token.strip_prefix('<').and_then(|t| t.strip_suffix('>'));
            let ok = match named {
                Some(name) => {
                    names.split(' ').any(|k| k == name)
                        || (name.len() == 3 && name.starts_with("C-"))
                        || name == "lt"
                }
                None => {
                    token.chars().count() == 1
                        || token == "{char}"
                        || (f[1] == "c" && token.starts_with(':'))
                }
            };
            assert!(ok, "line {}: the token {token:?} is not a key", n + 1);
        }
        // a key is in the table once for each mode
        for m in f[1].chars() {
            let first = f[0].split(' ').next().unwrap().to_string();
            assert!(
                seen.insert((f[0].to_string(), m)),
                "line {}: {} twice in mode {m}",
                n + 1,
                first
            );
        }
    }
    assert!(table().len() > 100 && table().len() == text.lines().count());
}

/// What is typed: a key of the core, or one of sim's own events.
#[derive(Clone, Copy, Debug)]
enum Typed {
    K(Key),
    BackTab,
    Help,
}

fn feed(r: &mut Rig, t: Typed) {
    match t {
        Typed::K(k) => r.send(SimEvent::Term(Event::Key(k))),
        Typed::BackTab => r.send(SimEvent::BackTab),
        Typed::Help => r.send(SimEvent::Help),
    }
}

/// Every key there is, with the name the table gives it.
fn universe() -> Vec<(String, Typed)> {
    let mut all = vec![];
    for c in ' '..='~' {
        let name = match c {
            ' ' => "<Space>".to_string(),
            '<' => "<lt>".into(),
            '|' => "<Bar>".into(),
            c => c.to_string(),
        };
        all.push((name, Typed::K(Key::Char(c))));
    }
    for c in 'a'..='z' {
        all.push((format!("<C-{c}>"), Typed::K(Key::Ctrl(c))));
    }
    let named = [
        ("<Esc>", Key::Esc),
        ("<CR>", Key::Enter),
        ("<Tab>", Key::Tab),
        ("<BS>", Key::Backspace),
        ("<Del>", Key::Delete),
        ("<Up>", Key::Up),
        ("<Down>", Key::Down),
        ("<Left>", Key::Left),
        ("<Right>", Key::Right),
        ("<Home>", Key::Home),
        ("<End>", Key::End),
        ("<PageUp>", Key::PageUp),
        ("<PageDown>", Key::PageDown),
    ];
    all.extend(named.into_iter().map(|(n, k)| (n.to_string(), Typed::K(k))));
    all.push(("<S-Tab>".into(), Typed::BackTab));
    all.push(("<F1>".into(), Typed::Help));
    all
}

/// A Sim in which any key that is bound changes something: two files open, a long text with brackets,
/// quotes and repeated words, a search lit, a character found, something yanked, something to undo
/// and something to redo and to repeat, marks of changes from the disk above and below the cursor, the
/// cursor in the middle of a word inside quotes, braces, parentheses, brackets and angle brackets.
fn rich() -> Rig {
    let line = |i: usize| format!("<x [a (b {{c '\"d\"' e}} f) g] y> foo {i} foo bar\n");
    let text: String = (1..=80).map(line).collect();
    let mut r = rig_env(
        "k1-rich",
        &[&format!("a.txt={text}"), "b.txt=other\n"],
        &["a.txt"],
        (100, 30),
        |e| {
            e.skin = Skin::Depth;
            e.chrome = true;
            e.syn = Syntax::new(true);
            e.tab_uses = 3;
        },
    );
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys(":e b.txt\n<Tab>");
    r.keys("25G$xxuyy"); // two characters off the end of the line and one back: undo and redo have something
    let id = r.sim.bufs[r.sim.cur].id;
    let theirs: String = (1..=80)
        .map(|i| if [2, 10, 40].contains(&i) { line(i).replacen("foo", "FOO", 1) } else { line(i) })
        .collect();
    r.send(SimEvent::App(id, disk(theirs.as_bytes())));
    r.keys("/foo\n0f\"l3j"); // (the search centered the line: three down, `zz` has something to do)
    r
}

type Fingerprint = (Screen, Vec<String>, bool, Option<i32>, usize);

fn print(r: &Rig) -> Fingerprint {
    (r.shot.clone(), r.sim.prefix(), r.app().vi.idle(), r.quit, r.sim.cur)
}

/// The keys that do something after `prefix` (tokens already typed): those after which the Sim is not
/// as it was. Each is tried on a Sim of its own, on all the threads there are (a Sim is slow to make
/// in a debug build).
fn handled(prefix: &[&Typed], names: &[(String, Typed)]) -> BTreeSet<String> {
    let count = |n: &str| n.len() == 1 && "123456789".contains(n);
    // sim's own keys work whatever has been typed: a prefix does not hold them back
    let keep = |n: &str| prefix.is_empty() || !(count(n) || GLOBAL.contains(&n));
    let todo: Vec<&(String, Typed)> = names.iter().filter(|(n, _)| keep(n)).collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
    let part = todo.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let jobs: Vec<_> = todo
            .chunks(part)
            .map(|chunk| {
                scope.spawn(move || {
                    let base = print(&rich());
                    let did = |t: &Typed| {
                        let mut r = rich();
                        prefix.iter().for_each(|p| feed(&mut r, **p));
                        feed(&mut r, *t);
                        print(&r) != base
                    };
                    chunk.iter().filter(|(_, t)| did(t)).map(|(n, _)| n.clone()).collect::<Vec<_>>()
                })
            })
            .collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap()).collect()
    })
}

/// The keys that sim takes before the core's parser sees them: with a prefix typed they end it, and are
/// not keys after it.
const GLOBAL: [&str; 6] = ["<C-n>", "<C-p>", "<C-f>", "<C-s>", "<F1>", "<S-Tab>"];
const OPERATORS: [&str; 6] = ["d", "c", "y", "<lt>", ">", "!"];

/// The keys the table has after `prefix` in Normal mode (and, after an operator, for `o`), without the
/// class tokens.
fn listed(prefix: &[&str]) -> BTreeSet<String> {
    let after = |head: &[&str], mode: char| -> Vec<String> {
        let tokens = |e: &Entry| -> Vec<String> {
            let follows = e.seq.len() > head.len()
                && head.iter().zip(&e.seq).all(|(h, t)| t.iter().any(|a| a == h));
            if e.modes.contains(mode) && follows { e.seq[head.len()].clone() } else { vec![] }
        };
        table().iter().flat_map(tokens).filter(|t| t.len() < 2 || !t.starts_with('{')).collect()
    };
    let mut set: BTreeSet<String> = after(prefix, 'n').into_iter().collect();
    if prefix.first().is_some_and(|p| OPERATORS.contains(p)) {
        set.extend(after(&prefix[1..], 'o'));
    }
    if !prefix.is_empty() {
        set.retain(|t| !(t.len() == 1 && "123456789".contains(t.as_str())));
    }
    set
}

fn typed(name: &str) -> Typed {
    universe().into_iter().find(|(n, _)| n == name).unwrap().1
}

fn compare(prefix: &[&str]) {
    let all = universe();
    let seq: Vec<Typed> = prefix.iter().map(|p| typed(p)).collect();
    let refs: Vec<&Typed> = seq.iter().collect();
    let did = handled(&refs, &all);
    let has = listed(prefix);
    let (undocumented, dead): (Vec<_>, Vec<_>) =
        (did.difference(&has).cloned().collect(), has.difference(&did).cloned().collect());
    assert!(
        undocumented.is_empty() && dead.is_empty(),
        "after {prefix:?}: keys that do something and the table lacks {undocumented:?}; keys the table has and that do nothing {dead:?}"
    );
}

#[test]
fn key_1_the_keys_that_do_something_are_the_normal_mode_keys_of_the_table() {
    compare(&[]);
}

#[test]
fn key_1_after_a_prefix_the_keys_that_do_something_are_the_table_s() {
    for prefix in ["g", "z", "Z", "[", "]", "<Space>"] {
        compare(&[prefix]);
    }
    compare(&["<Space>", "b"]);
}

#[test]
fn key_1_after_an_operator_the_motions_and_the_objects_are_the_table_s() {
    for op in OPERATORS {
        compare(&[op]);
    }
    compare(&["d", "i"]);
    compare(&["d", "a"]);
}
