//! UI-15: a char that is shown in another form (SAFE-8's `^[` and `<U+202E>`) is drawn as the alert's
//! block in flat and depth and in reverse video in calm, in every state a cell can be in: if one state
//! hid the signal, an attacker would choose that state. The same text typed in a file never is.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use si::core::view::Style;
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::paint::{Painter, Screen};
use sim::sim::SimEvent;

const BG: Rgb = (0x1e, 0x1e, 0x1e);
/// A control char, a bidirectional one, and the literal text of each (the file's own chars).
const TEXT: &str = "a\u{1b}b \u{202e}c <U+202E> ^[ end\n";

fn sized(name: &str, skin: Skin, files: &[&str], size: (u16, u16)) -> Rig {
    let first = files[0].split('=').next().unwrap().to_string();
    let mut r = rig_env(name, files, &[&first], size, |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

fn shown(name: &str, skin: Skin) -> Rig {
    sized(name, skin, &[&format!("h.txt={TEXT}")], (60, 10))
}

fn col(text: &str, needle: &str) -> u16 {
    text[..text.find(needle).unwrap()].chars().count() as u16
}

fn draw(s: Screen) -> String {
    let mut p = Painter::new(Vec::new());
    p.draw(s).unwrap();
    String::from_utf8(p.out).unwrap()
}

/// The SGR sequence in force at the first `what` of `out`.
fn seq(out: &str, what: &str) -> String {
    let i = out.find(what).unwrap();
    format!("\x1b[{}", out[..i].rsplit("\x1b[").next().unwrap())
}

/// The sequence a hidden char is drawn with: the alert's block in flat and depth (its surface and
/// its own text), reverse video of the primary in calm: nothing else, whatever the cell is in.
fn token(skin: Skin) -> String {
    let (t, p) = (Tones::new(&palette(BG)), palette(BG));
    let rgb = |c: Rgb| format!("{};{};{}", c.0, c.1, c.2);
    if skin == Skin::Calm {
        return format!("\x1b[0;7;38;2;{}m", rgb(p.primary));
    }
    let (back, text) = t.alert_block();
    format!("\x1b[0;38;2;{};48;2;{}m", rgb(text), rgb(back))
}

/// `what` of the Painter's output for `r` is drawn as a hidden char is, and its twin typed as plain
/// text is not.
fn assert_hidden(r: &Rig, skin: Skin, state: &str) {
    let out = draw(r.shot.clone());
    assert_eq!(seq(&out, "^["), token(skin), "{skin:?} {state}: the control char");
    assert_eq!(seq(&out, "<U+202E>"), token(skin), "{skin:?} {state}: the bidirectional one");
    // the literal text, the last of its kind on the line
    let lit = out.rfind("<U+202E>").unwrap();
    let plain = format!("\x1b[{}", out[..lit].rsplit("\x1b[").next().unwrap());
    assert_ne!(plain, token(skin), "{skin:?} {state}: the literal text is plain");
    assert!(!plain.contains(";7"), "{skin:?} {state}: not reversed: {plain:?}");
}

#[test]
fn ui_15_a_hidden_char_is_the_alert_s_block_and_the_same_text_typed_is_not() {
    for skin in [Skin::Flat, Skin::Depth, Skin::Calm] {
        let r = shown("hid-plain", skin);
        assert_hidden(&r, skin, "plain");
        // flat and depth took the spans out for the paint, calm keeps them for the Painter
        let y = r.shot.frame.rows.iter().position(|x| x.text.contains("^[b")).unwrap();
        let kept = r.shot.frame.rows[y].spans.iter().filter(|s| s.1 == Style::Hidden).count();
        assert_eq!(kept > 0, skin == Skin::Calm, "{skin:?}");
        assert_eq!(r.text(), TEXT.trim_end_matches('\n'), "the file's chars are the file's");
    }
}

#[test]
fn ui_15_it_holds_in_a_selection_a_search_match_and_on_the_cursor_line() {
    for skin in [Skin::Flat, Skin::Depth, Skin::Calm] {
        // a selection (gray) over the whole line
        let mut r = shown("hid-sel", skin);
        r.keys("v$");
        assert_hidden(&r, skin, "selected");
        // and where there is no gray for it, the selection is an underline
        let mut r = rig_env("hid-sel2", &[&format!("h.txt={TEXT}")], &["h.txt"], (60, 10), |e| {
            e.skin = skin;
        });
        r.keys("v$");
        let out = draw(r.shot.clone());
        assert!(!seq(&out, "^[").contains(";4"), "{skin:?}: not underlined: {:?}", seq(&out, "^["));
        // a match of the last search, the current one and the others
        let mut r = shown("hid-hit", skin);
        r.keys("/\\x1b\n");
        assert_hidden(&r, skin, "the control char is the current match");
        r.keys("/\\x{202e}\n");
        assert_hidden(&r, skin, "the bidirectional one is");
        r.keys("/[a-z]\n");
        assert_hidden(&r, skin, "matches around them");
        // the cursor's line in Insert mode, which has a surface of its own
        let mut r = shown("hid-ins", skin);
        r.keys("i");
        assert_hidden(&r, skin, "the cursor's line in Insert mode");
    }
}

#[test]
fn ui_15_it_holds_inside_a_change_from_outside_and_in_the_fog_of_a_far_row() {
    for skin in [Skin::Flat, Skin::Depth, Skin::Calm] {
        // someone else writes the overrides into a word of the file: the Trojan Source case
        let mut r = sized("hid-change", skin, &["h.txt=one\ntwo plain words\nthree\n"], (60, 10));
        let id = r.sim.bufs[0].id;
        r.send(SimEvent::App(
            id,
            disk("one\ntwo pl\u{1b}ain wo\u{202e}rds <U+202E> ^[ end\nthree\n".as_bytes()),
        ));
        assert_hidden(&r, skin, "inside a change from outside");
        let y = r.shot.frame.rows.iter().position(|x| x.text.contains("two pl")).unwrap();
        // the change is still there on the chars beside them (calm's blue, the tint of flat and depth)
        assert!(
            r.shot.frame.rows[y].spans.iter().any(|s| s.1 == Style::Change),
            "{skin:?}: {:?}",
            r.shot.frame.rows[y]
        );
        // a far row of the fog (depth): the token on line 5 and the cursor on line 40
        let filler = (1..5).map(|i| format!("line {i}\n"));
        let rest = (6..=40).map(|i| format!("line {i}\n"));
        let text: String = filler.chain(std::iter::once(TEXT.to_string())).chain(rest).collect();
        let mut r = sized("hid-fog", skin, &[&format!("h.txt={text}")], (60, 40));
        r.keys("G");
        assert_hidden(&r, skin, "in the fog of a row far from the cursor");
    }
}

#[test]
fn ui_15_it_holds_in_a_heading_band_and_in_a_code_fence() {
    for skin in [Skin::Flat, Skin::Depth, Skin::Calm] {
        let h1 = "# a\u{1b}b \u{202e}c <U+202E> end\n";
        let r = sized("hid-h1", skin, &[&format!("README.md={h1}")], (60, 10));
        assert_hidden(&r, skin, "in a first-level heading, on its band");
        let h2 = "## a\u{1b}b \u{202e}c <U+202E> end\n";
        let r = sized("hid-h2", skin, &[&format!("README.md={h2}")], (60, 10));
        assert_hidden(&r, skin, "in a second-level heading");
        let fence = "```bash\na\u{1b}b \u{202e}c <U+202E> end\n```\n";
        let r = sized("hid-fence", skin, &[&format!("README.md={fence}")], (60, 10));
        assert_hidden(&r, skin, "in a fenced block, on its surface");
        // and in a table, a quote, a done task: whatever the markdown makes of the line
        let md = "> a\u{1b}b \u{202e}c <U+202E> end\n";
        let r = sized("hid-quote", skin, &[&format!("README.md={md}")], (60, 10));
        assert_hidden(&r, skin, "in a quote");
        let task = "- [x] a\u{1b}b \u{202e}c <U+202E> end\n";
        let r = sized("hid-task", skin, &[&format!("README.md={task}")], (60, 10));
        assert_hidden(&r, skin, "in a done task");
    }
}

#[test]
fn ui_15_the_pickers_preview_draws_them_the_same_way() {
    let mut r = sized("hid-preview", Skin::Flat, &[&format!("h.txt={TEXT}")], (130, 40));
    r.keys("<C-f>end");
    let (w, t) = (r.win().unwrap(), Tones::new(&palette(BG)));
    let pane = r.pane();
    let at = pane.iter().position(|l| l.contains("^[b")).unwrap();
    let y = usize::from(w.pane.y) + at;
    let text = &r.shot.frame.rows[y].text;
    let (esc, bidi) = (col(text, "^[b"), col(text, "<U+202E>c"));
    let (back, on) = t.alert_block();
    let sealed = |x: u16| {
        r.shot.paints[y]
            .iter()
            .any(|(c, p)| c.contains(&x) && p.sealed && p.bg == Some(back) && p.fg == Some(on))
    };
    assert!(sealed(esc) && sealed(esc + 1) && sealed(bidi) && sealed(bidi + 7), "{text:?}");
    let literal = text[..text.rfind("<U+202E>").unwrap()].chars().count() as u16;
    assert!(!sealed(literal) && !sealed(literal + 7), "the same text typed is plain");
}
