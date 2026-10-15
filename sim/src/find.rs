//! FIND-1, FIND-2
//! Fuzzy matching of a query against relative paths (pure): the score, the matched chars, the
//! directories leading to matches, and the ranking.
//!
//! A path matches when the query's chars occur in it in order; the query is case-insensitive
//! unless it has an uppercase letter (smartcase, as in si). The match is taken in the file name
//! if the query occurs there, else in the whole path: the tightest one that ends first (the
//! earliest end, then the latest chars before it). That is a heuristic, not the best alignment,
//! which costs a table a path, too slow for 100,000 paths a key. Its score follows fzf's scale
//! (16 a char, -3 to start a gap and -1 more a char in it, 4 for chars that follow each other,
//! 8 for a char after a word edge, 9 after `/`, 7 for a camelCase hump; a run keeps the bonus
//! of its first char), plus 12 when the match lies in the file name, minus a quarter of the
//! path's length, so that shorter paths win a tie. These weights are fzf's, not tuned here.
use std::cmp::Ordering;
use std::collections::HashSet;

pub struct Query {
    chars: Vec<char>,
    exact: bool,
}

/// One matching path: its index in the list searched and its score. Where its chars matched is
/// [`Found::positions`].
pub struct Hit {
    pub index: usize,
    pub score: i32,
    at: usize,
}

/// Everything one search found, in the order of the list. A hit has one position per char of
/// the query.
#[derive(Default)]
pub struct Found {
    pub hits: Vec<Hit>,
    /// The directories (relative, `/`-separated) that lead to a hit, to be shown bold too.
    pub dirs: HashSet<String>,
    pos: Vec<usize>,
}

fn bonus(prev: char, c: char) -> i32 {
    match prev {
        '/' => 9,
        ' ' | '_' | '-' | '.' => 8,
        p if p.is_lowercase() && c.is_uppercase() => 7,
        _ => 0,
    }
}

/// A path as bytes (when it is ASCII, which is nearly always) or as chars.
trait Ch: Copy + Into<char> {}
impl<C: Copy + Into<char>> Ch for C {}

fn score<C: Ch>(cs: &[C], pos: &[usize], name: usize) -> i32 {
    let at = |i: usize| -> char { cs[i].into() };
    let (mut s, mut run) = (0, 0);
    for (k, &p) in pos.iter().enumerate() {
        let b = bonus(if p == 0 { '/' } else { at(p - 1) }, at(p));
        s += 16;
        if k > 0 && pos[k - 1] + 1 == p {
            s += b.max(run).max(4);
        } else {
            s += b - if k > 0 { (p - pos[k - 1] + 1) as i32 } else { 0 };
            run = b;
        }
    }
    s + if pos[0] >= name { 12 } else { 0 } - cs.len() as i32 / 4
}

impl Query {
    pub fn new(query: &str) -> Query {
        let exact = query.chars().any(char::is_uppercase);
        Query { chars: query.chars().map(|c| fold(c, exact)).collect(), exact }
    }

    /// The tightest match in `cs[from..]` that ends first: the earliest end, then the latest
    /// chars before it. Left in `pos` as positions in `cs`; none if there is none.
    fn tight<C: Ch>(&self, cs: &[C], from: usize, pos: &mut Vec<usize>) -> Option<()> {
        let is = |q: char| move |&c: &C| fold(c.into(), self.exact) == q;
        let mut end = from;
        for &q in &self.chars {
            end += cs[end..].iter().position(is(q))? + 1;
        }
        pos.clear();
        for &q in self.chars.iter().rev() {
            end = from + cs[from..end].iter().rposition(is(q))?;
            pos.push(end);
        }
        pos.reverse();
        Some(())
    }

    /// The score of a path, with the positions of its match left in `pos`: the match is in the
    /// file name if the query occurs there, else in the whole path. None if it does not match.
    fn locate<C: Ch>(&self, cs: &[C], pos: &mut Vec<usize>) -> Option<i32> {
        let name = cs.iter().rposition(|&c| c.into() == '/').map_or(0, |i| i + 1);
        let hit = (name > 0).then(|| self.tight(cs, name, pos)).flatten();
        hit.or_else(|| self.tight(cs, 0, pos)).map(|()| score(cs, pos, name))
    }

    /// FIND-1: every path of `paths` that matches, in the order of the list.
    pub fn search(&self, paths: &[String]) -> Found {
        let mut found = Found::default();
        if self.chars.is_empty() {
            return found;
        }
        let (mut cs, mut pos, mut last) = (vec![], vec![], "");
        for (index, path) in paths.iter().enumerate() {
            let score = if path.is_ascii() {
                self.locate(path.as_bytes(), &mut pos)
            } else {
                cs.clear();
                cs.extend(path.chars());
                self.locate(&cs, &mut pos)
            };
            let Some(score) = score else { continue };
            found.hits.push(Hit { index, score, at: found.pos.len() });
            found.pos.extend(&pos);
            let mut dir = path.rsplit_once('/').map_or("", |d| d.0);
            if dir == last {
                continue;
            }
            last = dir;
            while !dir.is_empty() && !found.dirs.contains(dir) {
                found.dirs.insert(dir.to_string());
                dir = dir.rsplit_once('/').map_or("", |d| d.0);
            }
        }
        found
    }
}

/// The chars compared: lowercased unless the query is case-sensitive.
fn fold(c: char, exact: bool) -> char {
    if exact { c } else { c.to_lowercase().next().unwrap_or(c) }
}

impl Found {
    /// The char positions of a hit's match, for bold.
    pub fn positions(&self, hit: &Hit) -> &[usize] {
        &self.pos[hit.at..hit.at + self.pos.len() / self.hits.len().max(1)]
    }

    /// FIND-2: the better hit comes first: by score, then by `lik`, the likelihood of the path
    /// (MARK-2; asked only when the scores tie), then by path, so that the order never depends
    /// on the order of the list.
    fn rank_cmp(paths: &[String], lik: &impl Fn(&str) -> f64, a: &Hit, b: &Hit) -> Ordering {
        let (pa, pb) = (&paths[a.index], &paths[b.index]);
        b.score.cmp(&a.score).then_with(|| lik(pb).total_cmp(&lik(pa))).then_with(|| pa.cmp(pb))
    }

    /// The best hit (the cursor goes there); none if nothing matched.
    pub fn best(&self, paths: &[String], lik: impl Fn(&str) -> f64) -> Option<&Hit> {
        self.hits.iter().min_by(|a, b| Self::rank_cmp(paths, &lik, a, b))
    }
}
