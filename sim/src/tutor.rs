//! TUTOR-1, TUTOR-2, TUTOR-3, TUTOR-4
//! `:tutor` (pure): the lessons of the ten languages, packed; the language the locale asks for; the
//! text and the exercises of one language's file; and the grading of the exercises against the lines.
use flate2::bufread::DeflateDecoder;
use std::io::Read;

/// The languages of the README, in the order `scripts/tutor-pack.sh` packs them.
pub const LANGS: [&str; 10] = ["en", "ja", "zh-cn", "zh-tw", "ko", "es", "pt-br", "fr", "de", "ru"];

/// What the editor can be seen to do (the exercises of the last lesson, `@! name`), one bit each.
const EVENTS: [&str; 5] = ["finder", "grep", "tree", "look", "kept"];

/// `sim/tutor/<lang>.txt` for each language, each after a line `\u{1}<lang>`, deflated (raw: the
/// decoder is the one that syntect's syntaxes are read with, so it adds no code of its own).
pub const PACKED: &[u8] = include_bytes!("../tutor/tutor.deflate");

/// An exercise: its id, the lines under its row as they are given and as they are to be, or the
/// event that is to be seen instead.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Goal {
    pub id: String,
    pub work: Vec<String>,
    pub done: Vec<String>,
    pub event: Option<String>,
}

/// The text of the buffer, and its exercises in the order of the text.
#[derive(Clone, Debug, Default)]
pub struct Tutor {
    pub text: String,
    pub goals: Vec<Goal>,
}

/// An exercise's row: the goal (an index), the line it is on, and whether it is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    pub goal: usize,
    pub line: usize,
    pub ok: bool,
}

/// The bit of an event (none for a name that is not one).
pub fn bit(event: &str) -> u8 {
    EVENTS.iter().position(|e| *e == event).map_or(0, |i| 1 << i)
}

/// TUTOR-3: the language of `LC_ALL`, else `LC_MESSAGES`, else `LANG` (the first that is not empty:
/// `C` and `POSIX` are a choice, and English), English for any other.
pub fn lang(var: &dyn Fn(&str) -> Option<String>) -> &'static str {
    let set =
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| var(k).filter(|v| !v.is_empty()));
    let low = set.unwrap_or_default().to_ascii_lowercase();
    let head = low.split(['.', '@']).next().unwrap_or_default();
    let (code, rest) = head.split_once(['_', '-']).unwrap_or((head, ""));
    match code {
        "ja" | "ko" | "es" | "fr" | "de" | "ru" => {
            LANGS.iter().find(|l| **l == code).unwrap_or(&"en")
        }
        "pt" => "pt-br",
        "zh" if ["tw", "hk", "mo", "hant"].iter().any(|r| rest.contains(r)) => "zh-tw",
        "zh" => "zh-cn",
        _ => "en",
    }
}

/// TUTOR-4: every language's text, unpacked (empty if the packed file is damaged: a test says so).
pub fn unpack() -> String {
    let mut all = String::new();
    DeflateDecoder::new(PACKED).read_to_string(&mut all).map_or_else(|_| String::new(), |_| all)
}

/// TUTOR-3: the lessons of `lang` (English for a code that is none), for plain mode or not.
pub fn load(lang: &str, plain: bool) -> Tutor {
    let all = unpack();
    let of = |l: &str| all.split_once(&format!("\u{1}{l}\n")).map(|(_, r)| r.split('\u{1}').next());
    parse(of(lang).or_else(|| of("en")).flatten().unwrap_or_default(), plain)
}

/// `\u{200B}` in a file is that char: the files hold no hidden char of their own.
fn unescape(s: &str) -> String {
    let (mut out, mut rest) = (String::new(), s);
    while let Some((head, tail)) = rest.split_once("\\u{") {
        out.push_str(head);
        let char_of = |hex: &str| char::from_u32(u32::from_str_radix(hex, 16).ok()?);
        match tail.split_once('}').and_then(|(hex, after)| Some((char_of(hex)?, after))) {
            Some((c, after)) => (out.push(c), rest = after),
            None => (out.push_str("\\u{"), rest = tail),
        };
    }
    out + rest
}

/// TUTOR-1, TUTOR-2: a language's file as the buffer's text and its exercises. A line that starts with
/// `[ ] ` and an id is an exercise's row, the lines up to the first `@` are its work, `@ line`
/// is a line the work is to become (not in the text) and `@! name` an event to be seen. `%editor` to
/// `%end` is for the editor only, and has no place in plain mode.
pub fn parse(src: &str, plain: bool) -> Tutor {
    let (mut t, mut skip, mut work) = (Tutor::default(), false, false);
    for raw in src.lines() {
        match raw {
            "%editor" => skip = plain,
            "%end" => skip = false,
            _ if skip => {}
            _ => t.line(&unescape(raw), &mut work),
        }
    }
    t
}

/// Whether `line` is the row of the exercise `id`.
fn row_of(line: &str, id: &str) -> bool {
    line.strip_prefix("[ ] ").and_then(|r| r.strip_prefix(id)).is_some_and(|r| r.starts_with(' '))
}

impl Tutor {
    /// One line of a file: a solution line or an event is the last exercise's and is not in the text; any
    /// other is, and is the exercise's work if it comes under its row and before the first solution.
    fn line(&mut self, line: &str, work: &mut bool) {
        if let Some(event) = line.strip_prefix("@! ") {
            self.goals.last_mut().iter_mut().for_each(|g| g.event = Some(event.trim().into()));
            *work = false;
        } else if let Some(done) = line.strip_prefix("@ ") {
            self.goals.last_mut().iter_mut().for_each(|g| g.done.push(done.into()));
            *work = false;
        } else {
            if let Some(id) = line.strip_prefix("[ ] ").and_then(|r| r.split(' ').next()) {
                self.goals.push(Goal { id: id.into(), ..Goal::default() });
                *work = true;
            } else if let (true, Some(g)) = (*work, self.goals.last_mut()) {
                g.work.push(line.into());
            }
            self.text.push_str(line);
            self.text.push('\n');
        }
    }

    /// TUTOR-2: the exercises whose rows are in `lines`, and whether each is done: the lines under the
    /// row are the ones it is to become (blanks at the end of a line do not count), or its event has
    /// been seen (`seen`, the bits of `bit`). A row that is not found has no mark.
    pub fn grade(&self, lines: &[String], seen: u8) -> Vec<Mark> {
        let mut out = vec![];
        for (goal, g) in self.goals.iter().enumerate() {
            let Some(line) = lines.iter().position(|l| row_of(l, &g.id)) else { continue };
            let under = lines.get(line + 1..).unwrap_or_default();
            let same = under.len() >= g.done.len()
                && g.done.iter().zip(under).all(|(a, b)| a.trim_end() == b.trim_end());
            let ok = g.event.as_deref().map_or(same, |e| seen & bit(e) != 0);
            out.push(Mark { goal, line, ok });
        }
        out
    }
}
