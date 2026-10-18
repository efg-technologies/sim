//! TUTOR-1 to TUTOR-4: `:tutor`, a scratch copy of short lessons with exercises that are graded as
//! the text changes, in the language of the locale, packed small.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::sim::SimEffect;
use sim::tutor::{self, LANGS, Tutor};
use sim_core::core::app::Effect;
use std::collections::HashMap;

// ---- the language ----

fn locale(vars: &[(&str, &str)]) -> &'static str {
    let map: HashMap<String, String> =
        vars.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    tutor::lang(&|k| map.get(k).cloned())
}

#[test]
fn tutor_3_the_language_is_chosen_from_lc_all_then_lc_messages_then_lang() {
    assert_eq!(locale(&[]), "en");
    assert_eq!(locale(&[("LANG", "ja_JP.UTF-8")]), "ja");
    assert_eq!(locale(&[("LANG", "ja_JP.UTF-8"), ("LC_MESSAGES", "de_DE.UTF-8")]), "de");
    let all = [("LANG", "ja_JP.UTF-8"), ("LC_MESSAGES", "de_DE.UTF-8"), ("LC_ALL", "fr_FR.UTF-8")];
    assert_eq!(locale(&all), "fr", "LC_ALL wins");
    // an empty variable is unset; LC_ALL=C is a choice (POSIX), and English is the C locale's
    assert_eq!(locale(&[("LC_ALL", ""), ("LANG", "ko_KR.UTF-8")]), "ko");
    assert_eq!(locale(&[("LC_ALL", "C"), ("LANG", "ja_JP.UTF-8")]), "en");
    assert_eq!(locale(&[("LC_ALL", "POSIX"), ("LANG", "ja_JP.UTF-8")]), "en");
    // the codes of the ten, with their regions, encodings and modifiers
    for (v, want) in [
        ("zh_CN.UTF-8", "zh-cn"),
        ("zh_SG", "zh-cn"),
        ("zh", "zh-cn"),
        ("zh_TW.UTF-8", "zh-tw"),
        ("zh_HK.UTF-8", "zh-tw"),
        ("zh_Hant_TW", "zh-tw"),
        ("pt_BR.UTF-8", "pt-br"),
        ("pt_PT.UTF-8", "pt-br"),
        ("ko_KR.eucKR", "ko"),
        ("es_MX.UTF-8", "es"),
        ("fr_CA.UTF-8", "fr"),
        ("de_AT@euro", "de"),
        ("ru_RU.KOI8-R", "ru"),
        ("en_US.UTF-8", "en"),
        ("ja", "ja"),
        ("JA_JP", "ja"),
    ] {
        assert_eq!(locale(&[("LANG", v)]), want, "{v}");
    }
    // anything else is English, and so is garbage
    for v in ["sv_SE.UTF-8", "it_IT", "C", "POSIX", "", "???", "_", ".UTF-8", "x"] {
        assert_eq!(locale(&[("LANG", v)]), "en", "{v:?}");
    }
    assert_eq!(locale(&[("LANGUAGE", "ja")]), "en", "LANGUAGE is gettext's, not a locale");
    // an unknown code asked of `load` is English, too
    assert_eq!(tutor::load("xx", false).text, tutor::load("en", false).text);
}

#[test]
fn tutor_3_there_are_ten_languages_and_each_has_the_same_exercises() {
    assert_eq!(LANGS, ["en", "ja", "zh-cn", "zh-tw", "ko", "es", "pt-br", "fr", "de", "ru"]);
    for plain in [false, true] {
        let en = tutor::load("en", plain);
        assert!(en.goals.len() >= if plain { 14 } else { 19 }, "{} goals", en.goals.len());
        for lang in LANGS {
            let t = tutor::load(lang, plain);
            assert_eq!(t.goals, en.goals, "{lang}: the exercises are the same in every language");
            // the lines of an exercise are in the text, row first, and a row is on a line of its own
            let lines: Vec<&str> = t.text.lines().collect();
            for g in &t.goals {
                let at = lines.iter().position(|l| l.starts_with(&format!("[ ] {} ", g.id)));
                let at = at.unwrap_or(usize::MAX);
                assert!(at != usize::MAX, "{lang}: no row for {}", g.id);
                assert_eq!(&lines[at + 1..at + 1 + g.work.len()], &g.work[..], "{lang}: {}", g.id);
                assert_eq!(
                    lines.iter().filter(|l| l.starts_with(&format!("[ ] {} ", g.id))).count(),
                    1,
                    "{lang}: {} is on one row",
                    g.id
                );
            }
            // every lesson has its title line, for the search that jumps to it
            let top = if plain { 8 } else { 9 };
            for n in 1..=top {
                assert!(lines.iter().any(|l| l.starts_with(&format!("## {n} "))), "{lang}: ## {n}");
            }
            assert!(!lines.iter().any(|l| l.starts_with("## 9 ")) || !plain, "{lang}: plain has 8");
            // nothing of the markup of the files is left in the text
            for l in &lines {
                assert!(!l.starts_with('@') && !l.starts_with('%'), "{lang}: {l:?}");
            }
            assert!(!t.text.contains("\\u{"), "{lang}: an escape was not read");
            // a row is short enough for the screen it is meant for: 70 columns, as the core counts them
            for l in &lines {
                let cs: Vec<char> = l.chars().collect();
                let w: usize = (0..cs.len()).map(|i| sim_core::core::view::width(&cs, i, 0)).sum();
                assert!(w <= 70, "{lang}: {w} columns: {l:?}");
            }
        }
    }
}

// ---- the grading ----

fn solved(t: &Tutor) -> Vec<String> {
    let mut lines: Vec<String> = t.text.lines().map(str::to_string).collect();
    for g in t.goals.iter().rev() {
        let at = lines.iter().position(|l| l.starts_with(&format!("[ ] {} ", g.id))).unwrap();
        lines.splice(at + 1..at + 1 + g.work.len(), g.done.iter().cloned());
    }
    lines
}

#[test]
fn tutor_2_a_row_is_a_check_when_the_lines_under_it_are_the_solution_and_a_cross_when_not() {
    let t = tutor::load("en", false);
    let start: Vec<String> = t.text.lines().map(str::to_string).collect();
    let marks = t.grade(&start, 0);
    assert_eq!(marks.len(), t.goals.len(), "every row is found");
    assert!(marks.iter().all(|m| !m.ok), "nothing is done at the start: {marks:?}");
    // the rows are on the lines where they are
    for m in &marks {
        assert!(start[m.line].starts_with(&format!("[ ] {} ", t.goals[m.goal].id)));
    }
    // done: every exercise that is a text is a check, an event is one only once it was seen
    let done = solved(&t);
    let marks = t.grade(&done, 0);
    for m in &marks {
        assert_eq!(m.ok, t.goals[m.goal].event.is_none(), "{}", t.goals[m.goal].id);
    }
    let all = (1..=31).fold(0u8, |a, i| a | (1 << (i % 8)));
    assert!(t.grade(&done, all).iter().all(|m| m.ok), "every event seen");
    // one exercise at a time: undoing it turns only its mark back
    let g = t.goals.iter().position(|g| g.id == "3.1").unwrap();
    let mut lines = done.clone();
    let at = lines.iter().position(|l| l.starts_with("[ ] 3.1 ")).unwrap();
    lines[at + 1] = "one two two three".into();
    let now = t.grade(&lines, 0);
    assert!(!now.iter().find(|m| m.goal == g).unwrap().ok);
    assert!(now.iter().filter(|m| m.goal != g && t.goals[m.goal].event.is_none()).all(|m| m.ok));
    // the lines can move (a line added above), and trailing blanks do not matter
    let mut moved = done.clone();
    moved.insert(0, "a line typed at the top".into());
    let at = moved.iter().position(|l| l.starts_with("[ ] 3.1 ")).unwrap();
    moved[at + 1].push_str("   ");
    assert!(t.grade(&moved, 0).iter().find(|m| m.goal == g).unwrap().ok);
    // a row that was typed over is not found, so it has no mark at all
    let mut broken = done;
    let at = broken.iter().position(|l| l.starts_with("[ ] 3.1 ")).unwrap();
    broken[at] = "[x] 3.1 done".into();
    assert!(!t.grade(&broken, 0).iter().any(|m| m.goal == g));
}

#[test]
fn tutor_2_the_events_are_five_and_a_name_that_is_none_is_never_seen() {
    let t = tutor::load("en", false);
    let events: Vec<&str> = t.goals.iter().filter_map(|g| g.event.as_deref()).collect();
    assert_eq!(events, ["finder", "grep", "tree", "look", "kept"]);
    let bits: Vec<u8> = events.iter().map(|e| tutor::bit(e)).collect();
    assert!(bits.iter().all(|b| b.count_ones() == 1), "{bits:?}");
    assert_eq!(bits.iter().fold(0, |a, b| a | b).count_ones(), 5, "five different bits");
    assert_eq!(tutor::bit("nonsense"), 0);
    // the plain tutor has none of them: those are the editor's
    assert!(tutor::load("en", true).goals.iter().all(|g| g.event.is_none()));
}

// ---- the packing ----

#[test]
fn tutor_4_the_packed_lessons_are_the_text_of_the_files_and_smaller_than_it() {
    let mut all = String::new();
    for lang in LANGS {
        let text = std::fs::read_to_string(format!("tutor/{lang}.txt")).unwrap();
        all += &format!("\u{1}{lang}\n{text}");
    }
    assert_eq!(
        tutor::unpack(),
        all,
        "run scripts/tutor-pack.sh after the text of a language changes"
    );
    assert!(tutor::PACKED.len() * 2 < all.len(), "{} of {}", tutor::PACKED.len(), all.len());
    // every language is in it, once
    for lang in LANGS {
        assert_eq!(all.matches(&format!("\u{1}{lang}\n")).count(), 1, "{lang}");
    }
}

// ---- on the screen ----

const FILES: [&str; 2] = ["a.txt=one\ntwo\n", "keep.txt=kept\n"];

fn row(r: &Rig, what: &str) -> String {
    let rows: Vec<&String> = r.shot.frame.rows.iter().map(|x| &x.text).collect();
    rows.iter().find(|x| x.contains(what)).map(|x| (*x).clone()).unwrap_or_else(|| {
        let all: Vec<&str> = rows.iter().map(|x| x.as_str()).collect();
        format!("<{what:?} is not on the screen:\n{}>", all.join("\n"))
    })
}

fn goto(r: &mut Rig, id: &str) {
    r.keys(&format!("gg/\\[ \\] {} \n", id.replace('.', "\\.")));
}

#[test]
fn tutor_1_tutor_opens_a_scratch_buffer_with_the_lessons_and_a_second_one_goes_to_it() {
    let mut r = rig_env("tutor-open", &FILES, &["a.txt"], (80, 24), |_| {});
    assert_eq!(r.open_files(), ["a.txt"]);
    r.keys(":tutor\n");
    assert_eq!(r.sim.bufs.len(), 2);
    let b = &r.sim.bufs[r.sim.cur];
    assert_eq!(b.app.path, None, "a scratch buffer: it has no file");
    assert!(row(&r, "sim tutor").contains("about 15 minutes"), "{}", row(&r, "sim tutor"));
    assert!(r.text().contains("## 1 Moving around") && r.text().contains("## 9 Finding"));
    assert!(!r.text().contains("%editor") && !r.text().contains("@ "), "the markup is gone");
    // its tab has a name of its own, and `:qa` says it is why sim stays
    assert_eq!(
        r.sim.tabs().iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["a.txt", "[tutor]"]
    );
    r.keys("x:qa\n");
    assert!(r.status().contains("[tutor]"), "{}", r.status());
    assert_eq!(r.sim.bufs.len(), 2);
    r.keys("u");
    // the same again goes to it and does not open another; the file is where it was
    r.keys(":tutor\n");
    assert_eq!(r.sim.bufs.len(), 2);
    r.keys(":q!\n");
    assert_eq!(r.sim.bufs.len(), 1, ":q! closes it");
    assert_eq!(r.text(), "one\ntwo");
    // and :tutor again is a new one, as at first
    r.keys(":tutor\n");
    assert!(r.text().contains("## 1 Moving around"));
}

#[test]
fn tutor_2_a_row_turns_into_a_check_when_the_edit_is_right_and_back_when_it_is_undone() {
    let mut r = rig_env("tutor-grade", &FILES, &["a.txt"], (80, 24), |_| {});
    r.keys(":tutor\n");
    goto(&mut r, "1.1");
    assert!(row(&r, "1.1 Move").contains("[✗] 1.1 Move"), "{}", row(&r, "1.1 Move"));
    // the buffer's text is `[ ]`: the mark is what is drawn
    assert!(r.text().contains("[ ] 1.1 Move"));
    // move to the x with w, and delete it
    r.keys("j0wwwwx");
    assert_eq!(r.text().lines().find(|l| l.starts_with("a b c d")).unwrap(), "a b c d  e f g");
    assert!(row(&r, "1.1 Move").contains("[✓] 1.1 Move"), "{}", row(&r, "1.1 Move"));
    assert!(r.status().contains("1.1") && r.status().contains("of "), "{}", r.status());
    assert!(
        r.text().contains("[ ] 1.1 Move"),
        "the text is not touched: only the screen has the mark"
    );
    r.keys("u");
    assert!(row(&r, "1.1 Move").contains("[✗]"), "undone");
    r.keys("<C-r>");
    assert!(row(&r, "1.1 Move").contains("[✓]"), "redone");
    // a wrong edit is not a check
    r.keys("uhx");
    assert!(row(&r, "1.1 Move").contains("[✗]"), "a wrong edit");
}

#[test]
fn tutor_2_the_exercises_of_the_lessons_can_be_done_with_the_keys_they_teach() {
    let mut r = rig_env("tutor-keys", &FILES, &["a.txt"], (80, 24), |_| {});
    r.keys(":tutor\n");
    // (the row, then the keys from the first line of its work)
    for (id, keys) in [
        ("1.1", "j0wwwwx"),
        ("1.2", "3j$x"),
        ("2.1", "j0wwwi4 <Esc>"),
        ("2.2", "jA 3<Esc>"),
        ("2.3", "jo2<Esc>"),
        ("3.1", "jwwdw"),
        ("3.2", "jjdd"),
        ("3.3", "jwwcw3<Esc>"),
        ("4.1", "jdwdwu"),
        ("5.1", "jyyp"),
        ("5.2", "jddjp"),
        ("6.1", "/zq7\nndw"),
        ("6.2", "j:s/a/b/g\n"),
        ("7.1", "jddjp"),
        ("8.1", "j04lx"),
    ] {
        goto(&mut r, id);
        let before = row(&r, &format!("{id} "));
        assert!(before.contains("[✗]"), "{id}: {before}");
        r.keys(keys);
        goto(&mut r, id);
        let after = row(&r, &format!("{id} "));
        assert!(after.contains("[✓]"), "{id} with {keys:?}: {after}\n{}", r.text());
    }
}

#[test]
fn tutor_2_what_sim_adds_is_done_by_doing_it_and_stays_done() {
    let mut r = rig_env("tutor-events", &FILES, &["a.txt"], (80, 24), |_| {});
    r.keys(":tutor\n");
    for (id, keys) in [
        ("9.1", "<C-p><Esc>"),
        ("9.2", "<C-f><Esc>"),
        ("9.3", "<C-n><C-n>"),
        ("9.4", "zl"),
        ("9.5", "<Space>bk"),
    ] {
        goto(&mut r, id);
        assert!(row(&r, &format!("{id} ")).contains("[✗]"), "{id}");
        r.keys(keys);
        r.keys("<Esc>");
        goto(&mut r, id);
        assert!(row(&r, &format!("{id} ")).contains("[✓]"), "{id} with {keys:?}");
    }
    // the tree was shown and hidden again: it stays done
    goto(&mut r, "9.3");
    assert!(row(&r, "9.3 ").contains("[✓]"));
}

#[test]
fn tutor_1_nothing_the_learner_types_writes_a_file_or_a_snapshot() {
    let mut r = rig_env("tutor-write", &FILES, &["a.txt"], (80, 24), |_| {});
    let list = |dir: &std::path::Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    let (files, state) = (list(&r.cwd), list(&r.state));
    r.keys(":tutor\n");
    let id = r.sim.bufs[r.sim.cur].id;
    r.keys("ddddihello<Esc>");
    for cmd in [
        ":w\n",
        ":w out.txt\n",
        ":w! a.txt\n",
        ":w! keep.txt\n",
        ":wq\n",
        ":x\n",
        "ZZ",
        ":w! ../x\n",
        ":saveas y\n",
        ":w >> a.txt\n",
        "<C-s>",
    ] {
        r.keys(cmd);
        assert_eq!(r.sim.bufs.len(), 2, "{cmd:?}: it does not quit on a failed write");
    }
    assert_eq!(list(&r.cwd), files, "no file made");
    assert_eq!(std::fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "one\ntwo\n");
    assert_eq!(std::fs::read_to_string(r.cwd.join("keep.txt")).unwrap(), "kept\n");
    // a tick (and a signal) asks every buffer for a snapshot: not this one
    r.send(sim::sim::SimEvent::Tick(1000));
    r.send(sim::sim::SimEvent::Tick(2000));
    assert_eq!(list(&r.state), state, "no snapshot, no place remembered");
    let mine = |e: &SimEffect| matches!(e, SimEffect::App(i, _) if *i == id);
    let said: Vec<&SimEffect> = r.log.iter().filter(|e| mine(e)).collect();
    assert!(
        said.iter().all(|e| matches!(e, SimEffect::App(_, Effect::Clipboard(_)))),
        "the only effect it has is the clipboard's: {said:?}"
    );
    // it says so
    r.keys(":w out.txt\n");
    assert!(r.status().contains("scratch"), "{}", r.status());
    // :q! leaves it, and nothing is remembered of it
    r.keys(":q!\n");
    assert_eq!(r.sim.bufs.len(), 1);
    assert_eq!(list(&r.cwd), files);
}

#[test]
fn tutor_3_the_language_of_the_environment_is_the_text_and_an_unknown_one_is_english() {
    for lang in LANGS {
        let mut r =
            rig_env("tutor-lang", &FILES, &["a.txt"], (80, 24), |e| e.lang = lang.to_string());
        r.keys(":tutor\n");
        let first = std::fs::read_to_string(format!("tutor/{lang}.txt")).unwrap();
        let title = first.lines().next().unwrap();
        assert!(row(&r, title.split(':').next().unwrap()).contains(title), "{lang}: {title}");
    }
    let mut r = rig_env("tutor-xx", &FILES, &["a.txt"], (80, 24), |e| e.lang = "xx".into());
    r.keys(":tutor\n");
    assert!(r.text().contains("## 1 Moving around"));
    // the default, with no language told, is English as well
    let mut r = rig_env("tutor-def", &FILES, &["a.txt"], (80, 24), |_| {});
    r.keys(":tutor\n");
    assert!(r.text().contains("## 1 Moving around"));
}

#[test]
fn tutor_1_plain_mode_has_the_tutor_with_eight_lessons_and_writes_nothing() {
    let mut r = rig_env("tutor-plain", &FILES, &["--plain", "a.txt"], (80, 24), |_| {});
    r.keys(":tutor\n");
    assert!(r.text().contains("## 8 What sim shows"));
    assert!(
        !r.text().contains("## 9") && !r.text().contains("Finding files"),
        "the editor's lesson"
    );
    assert!(!r.text().contains("%end") && !r.text().contains("%editor"));
    goto(&mut r, "2.3");
    r.keys("jo2<Esc>");
    goto(&mut r, "2.3");
    assert!(row(&r, "2.3 ").contains("[✓]"));
    r.keys(":w\n:w out.txt\n:w! a.txt\n");
    assert!(!r.cwd.join("out.txt").exists());
    assert_eq!(std::fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "one\ntwo\n");
    r.keys(":q!\n");
    assert_eq!(r.text(), "one\ntwo");
    // plain mode without a file: the unnamed buffer stays, and the tutor is over it
    let mut r = rig_env("tutor-plain2", &[], &["--plain"], (80, 24), |_| {});
    r.keys(":tutor\n:q!\n");
    assert_eq!(r.sim.bufs.len(), 1);
    assert_eq!(r.text(), "");
}

#[test]
fn tutor_1_a_file_opened_from_the_tutor_is_a_file_and_the_lesson_is_over() {
    let mut r = rig_env("tutor-edit", &FILES, &["a.txt"], (80, 24), |_| {});
    r.keys(":tutor\n:e keep.txt\n");
    assert_eq!(r.sim.bufs.len(), 3, ":e opens another buffer, as ever");
    assert_eq!(r.text(), "kept");
    r.keys("ihi<Esc>:w\n");
    assert_eq!(std::fs::read_to_string(r.cwd.join("keep.txt")).unwrap(), "hikept\n");
}

#[test]
fn tutor_2_what_is_done_while_another_file_is_shown_is_not_the_lesson_s() {
    let mut r = rig_env("tutor-away", &FILES, &["a.txt"], (80, 24), |_| {});
    r.keys(":tutor\n");
    r.keys("<Tab><C-p><Esc>"); // to a.txt, and its finder
    assert_eq!(r.sim.bufs[r.sim.cur].app.path.as_deref(), Some("a.txt"));
    r.keys("<Tab>");
    goto(&mut r, "9.1");
    assert!(row(&r, "9.1 ").contains("[✗]"), "{}", row(&r, "9.1 "));
}
