//! UI-14
//! The matches of a search, found as the core finds them (pure): the regex of its pattern, with its
//! smartcase rule, and the char ranges of the matches in a line.
use regex_lite::Regex;
use std::ops::Range;

/// The most matches of one line that are lit: a minified file is one line of megabytes.
const MOST: usize = 1000;

/// A pattern compiled the way the core's `/` does: smartcase (`folds`), the regex of `regex-lite`.
pub struct Matcher(Regex);

impl Matcher {
    /// `cs`: the search was asked to be case-sensitive. None if the pattern does not compile (the core
    /// says so when it is searched).
    pub fn new(pat: &str, cs: bool) -> Option<Matcher> {
        let ci = sim_core::core::ex::folds(pat, cs);
        Regex::new(&if ci { format!("(?i){pat}") } else { pat.to_string() }).ok().map(Matcher)
    }

    /// The char ranges of the matches in `line`, left to right; an empty match is none.
    pub fn ranges(&self, line: &str) -> Vec<Range<usize>> {
        let (mut out, mut chars, mut at) = (vec![], 0, 0);
        for m in self.0.find_iter(line).filter(|m| !m.is_empty()).take(MOST) {
            chars += line[at..m.start()].chars().count();
            let n = line[m.start()..m.end()].chars().count();
            out.push(chars..chars + n);
            (chars, at) = (chars + n, m.end());
        }
        out
    }
}
