//! TREE-2, TREE-4: the window keys that move the focus between the text and the panel (`<C-w>`), and
//! the folder keys of Neo-tree's defaults (Enter, `C`, `z`, `Z`), driven through sim's state.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::{Event, Key};
use std::os::unix::fs::symlink;

const FILES: [&str; 5] = ["src/main.rs", "src/lib.rs", "docs/a.md", "README.md", "Cargo.toml"];

/// A file open, the panel closed, and the text with the focus.
fn text(name: &str) -> Rig {
    rig(name, &["f.txt=one two three\nfour\n", "src/main.rs", "README.md"], &["f.txt"])
}

fn focus(r: &Rig) -> (bool, bool) {
    (r.sim.panel.open, r.sim.panel.focus)
}

#[test]
fn tree_2_ctrl_w_w_toggles_the_focus_between_the_text_and_the_panel_which_stays_open() {
    for keys in ["<C-w>w", "<C-w><C-w>"] {
        let mut r = text("t2-w");
        assert_eq!(focus(&r), (false, false));
        r.keys(keys);
        assert_eq!(
            focus(&r),
            (true, true),
            "{keys}: from the text with the panel closed it opens and focuses"
        );
        assert!(r.selected().is_some(), "{keys}: the cursor row is shown");
        r.keys(keys);
        assert_eq!(
            focus(&r),
            (true, false),
            "{keys}: from the panel the focus goes to the text, the panel stays"
        );
        assert!(r.selected().is_none() && !r.panel().is_empty(), "{keys}: and is still drawn");
        r.keys("x"); // the text has the keys again
        assert_eq!(r.text(), "ne two three\nfour");
        r.keys(keys);
        assert_eq!(
            focus(&r),
            (true, true),
            "{keys}: from the text with the panel open it only focuses"
        );
        r.keys("x"); // the panel's: no `x`
        assert_eq!(r.text(), "ne two three\nfour");
    }
}

#[test]
fn tree_2_the_other_forms_of_the_window_keys_are_neovims() {
    let all = [(false, false), (true, false), (true, true)];
    // from the text, with the panel closed and open: to the panel
    for keys in [
        "<C-w>w",
        "<C-w>W",
        "<C-w>p",
        "<C-w><C-p>",
        "<C-w><C-w>",
        "<C-w>h",
        "<C-w><Left>",
        "<C-w><C-h>",
        "<C-w><BS>",
    ] {
        for from in &all[..2] {
            let mut r = text("t2-forms-panel");
            if from.0 {
                r.keys("<C-n><Esc>");
            }
            r.keys(keys);
            assert_eq!(focus(&r), (true, true), "{keys} with the panel {from:?}");
        }
    }
    // from the panel: the other pane is the text; h and t stay
    for (keys, to) in [
        ("<C-w>w", (true, false)),
        ("<C-w>W", (true, false)),
        ("<C-w>p", (true, false)),
        ("<C-w><C-p>", (true, false)),
        ("<C-w><C-w>", (true, false)),
        ("<C-w>l", (true, false)),
        ("<C-w><Right>", (true, false)),
        ("<C-w><C-l>", (true, false)),
        ("<C-w>b", (true, false)),
        ("<C-w><C-b>", (true, false)),
        ("<C-w>h", (true, true)),
        ("<C-w><Left>", (true, true)),
        ("<C-w><C-h>", (true, true)),
        ("<C-w><BS>", (true, true)),
        ("<C-w>t", (true, true)),
        ("<C-w><C-t>", (true, true)),
    ] {
        let mut r = text("t2-forms-from-panel");
        r.keys("<C-n>");
        r.keys(keys);
        assert_eq!(focus(&r), to, "{keys} from the panel");
    }
    // from the text: l and b do nothing (the panel closed stays closed, an open one unfocused)
    for keys in ["<C-w>l", "<C-w><Right>", "<C-w><C-l>", "<C-w>b", "<C-w><C-b>"] {
        let mut r = text("t2-forms-text");
        r.keys(keys);
        assert_eq!(focus(&r), (false, false), "{keys}: panel closed");
        r.keys("<C-n><Esc>");
        r.keys(keys);
        assert_eq!(focus(&r), (true, false), "{keys}: panel open");
    }
    // t is the panel if it is open, and nothing if it is not
    for keys in ["<C-w>t", "<C-w><C-t>"] {
        let mut r = text("t2-forms-t");
        r.keys(keys);
        assert_eq!(focus(&r), (false, false), "{keys}: no panel, no window");
        r.keys("<C-n><Esc>");
        r.keys(keys);
        assert_eq!(focus(&r), (true, true), "{keys}: the panel is open");
    }
}

#[test]
fn tree_2_a_window_key_leaves_the_panel_as_it_was() {
    let mut r = rig("t2-state", &FILES, &[]);
    r.keys("jl"); // src is open, the cursor on it
    let rows = r.panel();
    let at = r.selected();
    r.keys("jj<C-w>l");
    assert_eq!(r.panel(), rows, "the same rows");
    r.keys("<C-w>h");
    assert_eq!(r.panel(), rows);
    assert_eq!(
        r.selected().as_deref(),
        Some("    main.rs"),
        "{at:?}: the cursor row is where it was left"
    );
    // a search that is lit stays lit when the focus goes to the text and comes back
    r.keys("/lib<Down>");
    let lit = r.panel();
    r.keys("<C-w>l<C-w>h");
    assert_eq!(r.panel(), lit, "the matches stay");
    assert_eq!(focus(&r), (true, true));
}

#[test]
fn tree_2_with_no_file_open_the_focus_stays_in_the_panel() {
    let mut r = rig("t2-nofile", &FILES, &[]);
    assert_eq!(focus(&r), (true, true));
    for keys in ["<C-w>l", "<C-w>b", "<C-w>w", "<C-w><C-w>", "<C-w>p", "<C-w>h", "<C-w>t"] {
        r.keys(keys);
        assert_eq!(focus(&r), (true, true), "{keys}: there is no text to go to");
    }
}

#[test]
fn tree_2_ctrl_w_waits_for_one_key_and_any_other_key_ends_it_and_does_nothing() {
    let others = [
        "j", "k", "<Up>", "<Down>", "<C-j>", "<C-k>", "s", "v", "n", "q", "c", "o", "x", "r", "H",
        "J", "K", "L", "T", "P", "=", "+", "-", "_", "|", "<lt>", ">", "2", "0", "<Esc>", "<Tab>",
        "<S-Tab>", "<F1>", "<C-n>", "<C-f>", "<C-s>", "<CR>", " ", "i",
    ];
    for k in others {
        let mut r = rig("t2-other", &["f.txt=one two three\nfour\n", "g.txt=other\n"], &["f.txt"]);
        r.keys(":e g.txt\n<Tab>");
        let (before, cur) = (r.text(), r.sim.cur);
        r.keys("<C-w>");
        assert_eq!(r.sim.prefix(), ["<C-w>"], "pending");
        r.keys(k);
        assert!(r.sim.prefix().is_empty(), "{k}: the prefix is over");
        assert_eq!(focus(&r), (false, false), "{k}: nothing moved");
        assert!(r.win().is_none(), "{k}: no picker");
        assert_eq!((r.text(), r.sim.cur), (before, cur), "{k}: nothing changed");
        // and the key after it is an ordinary key: `w` is the motion and does not move the focus
        r.keys("<Esc>w");
        assert_eq!(focus(&r), (false, false), "{k}: the next key is not a window key");
    }
}

#[test]
fn tree_2_ctrl_w_is_the_cores_where_it_is_text_or_a_command_is_half_typed() {
    // Insert mode: it deletes a word, and the next key is typed
    let mut r = text("t2-insert");
    r.keys("A xy<C-w>w");
    assert_eq!(focus(&r), (false, false));
    assert!(r.sim.prefix().is_empty());
    assert_eq!(r.text(), "one two three w\nfour");
    // the command line: it is the line's, and the next key is typed in it
    let mut r = text("t2-cmd");
    r.keys(":ab<C-w>w");
    assert_eq!(focus(&r), (false, false));
    assert!(r.status().starts_with(':') && r.status().ends_with('w'), "{}", r.status());
    r.keys("<Esc>");
    // an operator, a count, a register name and Visual mode are the App's
    for pending in ["d", "3", "\"a", "v", "V"] {
        let mut r = text("t2-pending");
        r.keys(pending);
        r.keys("<C-w>w");
        assert!(!r.sim.panel.open, "{pending}: nothing opened the panel");
        assert!(r.sim.prefix().is_empty(), "{pending}: and nothing waits for a key");
    }
    // a picker: it deletes a word of the query
    let mut r = text("t2-picker");
    r.keys("<C-p>ab cd<C-w>");
    assert_eq!(r.query(), "ab");
    r.keys("w");
    assert_eq!(r.query(), "ab w", "(and the next key is typed in the query)");
    r.keys("<Esc>");
    // the tree's query: Ctrl-w is not a window key while one is typed
    let mut r = text("t2-query");
    r.keys("<C-n>/ab<C-w>w");
    assert_eq!(focus(&r), (true, true));
    assert!(r.sim.prefix().is_empty());
    assert_eq!(r.status(), "/abw  no match");
    // and not while a `g` waits in the tree: the key forgets it
    let mut r = text("t2-g");
    r.keys("<C-n>g<C-w>w");
    assert_eq!(focus(&r), (true, true));
    assert!(r.sim.prefix().is_empty());
}

#[test]
fn tree_2_the_core_has_nothing_on_ctrl_w_in_normal_mode_so_it_is_sims_to_take() {
    let mut r = text("t2-core");
    r.keys("w");
    let (status, text) = (r.status().to_string(), r.text());
    // straight to the App, past sim's keys: nothing happens, and nothing waits
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, Event::Key(Key::Ctrl('w'))));
    assert!(r.app().vi.idle());
    assert_eq!((r.status(), r.text()), (status.as_str(), text));
}

#[test]
fn key_3_a_pause_after_ctrl_w_opens_which_key_with_the_window_keys() {
    let mut r = rig_env("k3-ctrl-w", &["f.txt=one\n", "g.txt=two\n"], &["f.txt"], (100, 24), |e| {
        e.skin = Skin::Depth;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys("<C-w>");
    assert!(
        r.log.iter().any(|e| matches!(e, sim::sim::SimEffect::Pause(_))),
        "a pause is asked for"
    );
    r.pause();
    let rows = sim::keys::next(&r.sim.prefix(), 'n');
    assert_eq!(rows[0], ("w / <C-w> / W / p / <C-p>".into(), "other pane".into()));
    assert_eq!(rows.len(), 5, "{rows:?}");
    let shown: Vec<String> = r.screen.rows.iter().map(|x| x.text.clone()).collect();
    assert!(shown.iter().any(|t| t.contains("╭─ <C-w> ─")), "{shown:?}");
    assert!(shown.iter().any(|t| t.contains("other pane")), "{shown:?}");
    // in the tree it is the same window
    r.keys("<Esc><C-n>");
    r.keys("<C-w>");
    r.pause();
    assert_eq!(sim::keys::next(&r.sim.prefix(), 't').len(), 5);
    assert!(r.screen.rows.iter().any(|x| x.text.contains("╭─ <C-w> ─")));
    r.keys("<Esc>");
    assert!(r.sim.prefix().is_empty());
}

// ---- TREE-4: Enter, C, z, Z

#[test]
fn tree_4_enter_opens_a_closed_directory_closes_an_open_one_and_opens_a_file() {
    let mut r = rig("t4-enter", &["src/main.rs=fn main() {}\n", "a.txt"], &[]);
    r.keys("<CR>");
    assert_eq!(r.panel(), ["▾ src/", "    main.rs", "  a.txt"]);
    r.keys("<CR>");
    assert_eq!(r.panel(), ["▸ src/", "  a.txt"], "an open directory closes");
    assert_eq!(r.selected().as_deref(), Some("▸ src/"), "and the cursor stays");
    r.keys("<CR><CR>");
    assert_eq!(r.panel(), ["▸ src/", "  a.txt"]);
    // l and Right only open
    r.keys("ll<Right>");
    assert_eq!(r.panel(), ["▾ src/", "    main.rs", "  a.txt"], "an open directory is left open");
    // a file is opened as it was, and the focus goes to the text
    r.keys("j<CR>");
    assert_eq!(r.open_files(), ["src/main.rs"]);
    assert!(r.sim.panel.open && !r.sim.panel.focus);
}

#[test]
fn tree_4_enter_toggles_in_a_search_that_is_lit_and_opens_the_best_match_while_it_is_typed() {
    let mut r = rig("t4-enter-find", &FILES, &[]);
    r.keys("/lib");
    r.keys("<CR>"); // typed: it opens the best match
    assert_eq!(r.open_files(), ["src/lib.rs"]);
    // lit: a directory toggles, a file opens
    let mut r = rig("t4-enter-lit", &FILES, &[]);
    r.keys("/m<Down>"); // (stops typing; the matches stay lit)
    r.keys("gg");
    assert_eq!(r.selected().as_deref(), Some("▸ docs/"));
    r.keys("<CR>");
    assert!(r.panel().contains(&"▾ docs/".to_string()), "{:?}", r.panel());
    r.keys("<CR>");
    assert!(r.panel().contains(&"▸ docs/".to_string()), "{:?}", r.panel());
}

#[test]
fn tree_4_enter_toggles_in_the_narrowed_tree_and_the_changes_view() {
    // the narrowed tree: `.` keeps the open file and its directories, all open
    let mut r = rig("t4-enter-narrow", &["src/main.rs=fn main() {}\n", "a.txt"], &["src/main.rs"]);
    r.keys("<C-n>.");
    assert_eq!(r.panel(), ["▾ src/", "  • main.rs"]);
    r.keys("gg<CR>");
    assert_eq!(r.panel(), ["▸ src/"], "it closes there as h does");
    r.keys("<CR>");
    assert_eq!(r.panel(), ["▾ src/", "  • main.rs"]);
    // the changes view
    let mut r = rig_env(
        "t4-enter-changes",
        &["src/main.rs=fn main() {}\n", "docs/guide.md=# g\n"],
        &[],
        (100, 24),
        |e| {
            let d = &e.cwd;
            git(d, &["init", "-q"]);
            git(d, &["add", "-A"]);
            git(d, &["commit", "-q", "-m", "init"]);
            std::fs::write(d.join("src/main.rs"), "fn main() {}\nfn a() {}\n").unwrap();
        },
    );
    r.keys(" ge");
    let before = r.panel();
    assert!(before.iter().any(|l| l.contains("main.rs")), "{before:?}");
    r.defer = true;
    r.keys("Z");
    assert!(r.waiting.is_empty() && r.panel() == before, "Z has nothing to do there");
    r.defer = false;
    r.keys("gg<CR>");
    assert!(!r.panel().iter().any(|l| l.contains("main.rs")), "closed: {:?}", r.panel());
    r.keys("<CR>");
    assert_eq!(r.panel(), before, "and open again");
    r.keys("j<CR>");
    assert_eq!(r.open_files(), ["src/main.rs"], "a file opens");
}

#[test]
fn tree_4_a_click_still_opens_a_closed_directory_and_closes_an_open_one() {
    let mut r = rig("t4-click", &["src/main.rs=fn main() {}\n", "a.txt"], &[]);
    r.click(3, 0);
    assert_eq!(r.panel(), ["▾ src/", "    main.rs", "  a.txt"]);
    r.click(3, 0);
    assert_eq!(r.panel(), ["▸ src/", "  a.txt"]);
}

#[test]
fn tree_4_c_closes_the_open_directory_under_the_cursor_or_the_one_that_holds_the_entry() {
    let mut r = rig("t4-c", &["a/b/c.txt", "a/b/d.txt", "a/e.txt", "z.txt"], &[]);
    r.keys("ljl"); // a open, b open, the cursor on b
    assert_eq!(r.panel(), ["▾ a/", "  ▾ b/", "      c.txt", "      d.txt", "    e.txt", "  z.txt"]);
    r.keys("C");
    assert_eq!(r.panel(), ["▾ a/", "  ▸ b/", "    e.txt", "  z.txt"], "an open directory closes");
    assert_eq!(r.selected().as_deref(), Some("  ▸ b/"), "and the cursor stays on it");
    r.keys("C");
    assert_eq!(r.panel(), ["▸ a/", "  z.txt"], "a closed one closes the directory that holds it");
    assert_eq!(r.selected().as_deref(), Some("▸ a/"), "and the cursor goes there");
    r.keys("C");
    assert_eq!(r.panel(), ["▸ a/", "  z.txt"], "at the top level the holder is the root: nothing");
    assert_eq!(r.selected().as_deref(), Some("▸ a/"));
    // a file closes the directory it is in
    r.keys("lljj"); // a open, b closed: down to e.txt
    assert_eq!(r.selected().as_deref(), Some("    e.txt"));
    r.keys("C");
    assert_eq!(r.panel(), ["▸ a/", "  z.txt"]);
    assert_eq!(r.selected().as_deref(), Some("▸ a/"));
    // a file at the top level has no directory to close
    r.keys("jC");
    assert_eq!(r.selected().as_deref(), Some("  z.txt"));
    assert_eq!(r.panel(), ["▸ a/", "  z.txt"]);
}

#[test]
fn tree_4_c_moves_nothing_above_the_cursor() {
    let names: Vec<String> = (0..30).map(|i| format!("f{i:02}.txt")).collect();
    let mut files: Vec<String> = names.clone();
    files.push("d/x.txt".into());
    files.push("d/y.txt".into());
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let mut r = rig_sized("t4-c-above", &files, &[], (60, 11));
    r.keys("l"); // d open: the first row
    r.keys("jC");
    assert_eq!(r.selected().as_deref(), Some("▸ d/"));
    assert_eq!(r.panel()[0], "▸ d/", "the first row is where it was");
}

#[test]
fn tree_4_z_collapses_everything_as_dash_does() {
    let mut r = rig("t4-z", &["a/b/c.txt", "a/d.txt", "z.txt"], &[]);
    r.keys("ljlj");
    assert_eq!(r.panel(), ["▾ a/", "  ▾ b/", "      c.txt", "    d.txt", "  z.txt"]);
    r.keys("z");
    assert_eq!(r.panel(), ["▸ a/", "  z.txt"]);
    assert_eq!(r.selected().as_deref(), Some("▸ a/"), "the cursor stays on the top-level entry");
    r.keys("l"); // what was loaded is kept
    assert_eq!(r.panel(), ["▾ a/", "  ▸ b/", "    d.txt", "  z.txt"]);
}

const DEEP: [&str; 6] = ["a/b/c/d.txt", "a/b/e.txt", "a/f/g.txt", "a/h.txt", "i/j.txt", "k.txt"];

#[test]
fn tree_4_big_z_opens_the_directory_under_the_cursor_and_everything_below_it() {
    let mut r = rig("t4-bigz", &DEEP, &[]);
    r.keys("Z");
    assert_eq!(
        r.panel(),
        [
            "▾ a/",
            "  ▾ b/",
            "    ▾ c/",
            "        d.txt",
            "      e.txt",
            "  ▾ f/",
            "      g.txt",
            "    h.txt",
            "▸ i/",
            "  k.txt"
        ],
        "i/ is another directory and stays closed"
    );
    assert_eq!(r.selected().as_deref(), Some("▾ a/"), "the cursor stays on its entry");
    assert!(r.sim.panel.open && r.sim.panel.focus);
    // Z on a closed directory below an open one
    let mut r = rig("t4-bigz-below", &DEEP, &[]);
    r.keys("jZ");
    assert!(
        r.panel().contains(&"▾ i/".to_string()) && r.panel().contains(&"    j.txt".to_string())
    );
    assert!(r.panel().contains(&"▸ a/".to_string()), "a/ is not below it: {:?}", r.panel());
}

#[test]
fn tree_4_big_z_on_a_file_opens_the_directory_that_holds_it_and_the_cursor_stays_on_the_file() {
    let mut r = rig("t4-bigz-file", &DEEP, &[]);
    r.keys("l"); // a open
    r.keys("jjj"); // b, f, h.txt: the cursor on h.txt
    assert_eq!(r.selected().as_deref(), Some("    h.txt"));
    r.keys("Z");
    assert_eq!(
        r.selected().as_deref(),
        Some("    h.txt"),
        "the directories that opened above it moved it down"
    );
    assert!(r.panel().contains(&"    ▾ c/".to_string()), "{:?}", r.panel());
    // a file at the top level: the holder is the root, so everything opens
    let mut r = rig("t4-bigz-top", &DEEP, &[]);
    r.keys("G");
    assert_eq!(r.selected().as_deref(), Some("  k.txt"));
    r.keys("Z");
    assert_eq!(r.selected().as_deref(), Some("  k.txt"));
    assert_eq!(r.panel().iter().filter(|l| l.contains('▾')).count(), 5, "{:?}", r.panel());
}

#[test]
fn tree_4_big_z_waits_for_listings_and_goes_on_as_they_arrive() {
    let mut r = rig("t4-bigz-async", &DEEP, &[]);
    r.defer = true;
    r.keys("Z");
    assert_eq!(r.waiting.len(), 1, "only a/ was asked for");
    assert_eq!(r.panel(), ["▾ a/", "▸ i/", "  k.txt"], "open and empty until its listing comes");
    let mut rounds = 0;
    while !r.waiting.is_empty() {
        r.deliver();
        rounds += 1;
        assert!(rounds < 10);
    }
    assert_eq!(rounds, 3, "a/, then b/ and f/, then c/");
    assert_eq!(r.panel().len(), 10, "{:?}", r.panel());
    assert_eq!(r.selected().as_deref(), Some("▾ a/"));
    // it is over: a directory closed later stays closed when a listing arrives
    r.keys("jh");
    assert_eq!(r.selected().as_deref(), Some("  ▸ b/"));
    r.keys("<Esc><C-n>"); // (the panel reads every open directory again)
    assert!(r.panel().contains(&"  ▸ b/".to_string()), "{:?}", r.panel());
}

#[test]
fn tree_4_a_key_of_the_panels_own_stops_a_big_z_that_is_waiting() {
    let below = |r: &Rig| {
        let rows = r.sim.tree.rows(0, 100);
        (rows.iter().find(|x| x.name == "b").map(|x| x.open), rows.iter().any(|x| x.name == "a"))
    };
    for stop in ["j", "k", "-", "z", "Z", "<Esc>", "q"] {
        let mut r = rig("t4-bigz-cancel", &DEEP, &[]);
        r.defer = true;
        r.keys("Z");
        r.keys(stop);
        while !r.waiting.is_empty() {
            r.deliver();
        }
        if stop == "Z" {
            assert_eq!(r.sim.tree.rows(0, 100).len(), 10, "another Z starts over from the cursor");
            continue;
        }
        let (b, a) = below(&r);
        assert!(a && b != Some(true), "{stop}: a/ was read, and nothing below it opened ({b:?})");
        if !matches!(stop, "-" | "z") {
            assert_eq!(b, Some(false), "{stop}: b/ is there, closed");
        }
    }
    // a click stops it too
    let mut r = rig("t4-bigz-click", &DEEP, &[]);
    r.defer = true;
    r.keys("Z");
    r.click(3, 2); // k.txt
    while !r.waiting.is_empty() {
        r.deliver();
    }
    assert_eq!(below(&r), (Some(false), true));
}

#[test]
fn tree_4_big_z_opens_at_most_200_directories_and_says_so_and_z_again_goes_on() {
    let files: Vec<String> = (0..250).map(|i| format!("big/d{i:03}/f.txt")).collect();
    let mut files: Vec<&str> = files.iter().map(String::as_str).collect();
    files.push("big/z-last/g/h.txt");
    let mut r = rig_sized("t4-bigz-cap", &files, &[], (60, 40));
    let open = |r: &Rig| r.sim.tree.rows(0, 10_000).iter().filter(|x| x.is_dir && x.open).count();
    let is_open = |r: &Rig, name: &str| {
        r.sim.tree.rows(0, 10_000).iter().find(|x| x.name == name).map(|x| x.open)
    };
    r.keys("Z");
    assert_eq!(open(&r), 200, "big/ and 199 of its directories");
    assert!(
        r.status().starts_with("opened 200 folders; stopped there (Z again goes on)"),
        "{}",
        r.status()
    );
    assert_eq!(r.selected().as_deref(), Some("▾ big/"), "the cursor stays on its entry");
    assert_eq!((is_open(&r, "d198"), is_open(&r, "d199")), (Some(true), Some(false)));
    r.keys("Z");
    assert_eq!(open(&r), 253, "the 53 that were left: 51 of d, z-last and the one in it");
    assert!(!r.status().contains("stopped"), "{}", r.status());
}

#[test]
fn tree_4_big_z_never_opens_a_link_or_a_directory_git_ignores() {
    let mut r = rig_env(
        "t4-bigz-never",
        &[
            ".gitignore=target/\nnode_modules/\n",
            "a/src/x.rs",
            "a/target/debug/y.o",
            "a/node_modules/pkg/z.js",
            "a/real/inner/w.txt",
        ],
        &[],
        (100, 40),
        |e| {
            let d = &e.cwd;
            git(d, &["init", "-q"]);
            git(d, &["add", "-A"]);
            git(d, &["commit", "-q", "-m", "init"]);
            symlink(d.join("a/real"), d.join("a/link")).unwrap();
            symlink(d.join("a"), d.join("a/real/loop")).unwrap(); // (leads to a directory above: a file)
            symlink(d.join("a/src"), d.join("a/real/inner/other")).unwrap();
        },
    );
    // git's answer is in (the rig plays the shell)
    r.keys("Z");
    let rows = r.panel();
    let has = |s: &str| rows.iter().any(|l| l.trim().starts_with(s));
    assert!(has("▾ a/") && has("▾ src/") && has("▾ real/") && has("▾ inner/"), "{rows:?}");
    assert!(has("▸ target/") && has("▸ node_modules/"), "ignored ones stay closed: {rows:?}");
    assert!(has("▸ link/"), "a link to a directory stays closed: {rows:?}");
    assert!(has("▸ other/"), "and a link to a directory elsewhere: {rows:?}");
    assert!(
        !rows.iter().any(|l| l.contains("debug") || l.contains("pkg") || l.contains("y.o")),
        "{rows:?}"
    );
    // the directory it starts from is opened whatever it is: Z on target/ opens it and nothing in it
    r.keys("gg");
    let y = r.panel().iter().position(|l| l.trim().starts_with("▸ target/")).unwrap();
    r.keys(&"j".repeat(y));
    r.keys("Z");
    let rows = r.panel();
    assert!(rows.iter().any(|l| l.trim().starts_with("▾ target/")), "{rows:?}");
    assert!(rows.iter().any(|l| l.trim().starts_with("▸ debug/")), "{rows:?}");
}

#[test]
fn tree_4_big_z_does_nothing_in_the_narrowed_tree_and_the_changes_view() {
    let mut r = rig("t4-bigz-narrow", &DEEP, &["a/h.txt"]);
    r.keys("<C-n>.");
    let before = r.panel();
    r.defer = true;
    r.keys("Z");
    assert_eq!(r.panel(), before);
    assert!(r.waiting.is_empty(), "and asks for nothing");
    assert!(!r.status().contains("opened"), "{}", r.status());
}
