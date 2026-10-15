//! MARK-2
//! How likely a file is to be the next one opened, from the history of opening files (pure).
//!
//! likelihood(path) = the sum, over the times it was opened, of (age in seconds, at least 60)^-1/2.
//! That is ACT-R's base-level activation (Anderson & Schooler 1991), a power-law decay measured
//! on real access streams: one open a minute ago beats many from last week, yet opens still
//! add up, so recency and frequency count together, as in AccessRank (Fitchett & Cockburn 2012).
//! The history is bounded: 16 opens a path (older ones add almost nothing, and a cap keeps a
//! file opened daily for years from outweighing today's work) and 1,000 paths.
use std::collections::HashMap;

const MAX_OPENS: usize = 16;
const MAX_PATHS: usize = 1000;
/// Opens within a minute count the same, so that the weight stays finite.
const MIN_AGE: u64 = 60;

/// The times (seconds, oldest first) each path was opened. Keys are absolute paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History(HashMap<String, Vec<u64>>);

fn weight(opens: &[u64], now: u64) -> f64 {
    opens.iter().map(|&t| (now.saturating_sub(t).max(MIN_AGE) as f64).sqrt().recip()).sum()
}

impl History {
    /// Notes that `path` was opened at `now`. A path with a line break in it cannot be saved
    /// (one line a path), so it is not remembered.
    pub fn record(&mut self, path: &str, now: u64) {
        if path.contains(['\n', '\r']) {
            return;
        }
        let opens = self.0.entry(path.to_string()).or_default();
        opens.push(now);
        opens.sort_unstable();
        opens.drain(..opens.len().saturating_sub(MAX_OPENS));
        self.trim(now);
    }

    pub fn likelihood(&self, path: &str, now: u64) -> f64 {
        self.0.get(path).map_or(0.0, |opens| weight(opens, now))
    }

    /// The `n` likeliest paths under `root`, relative to it, with their likelihood, best first.
    pub fn top(&self, root: &str, n: usize, now: u64) -> Vec<(String, f64)> {
        let prefix = format!("{}/", root.trim_end_matches('/'));
        let rel = |(p, o): (&String, &Vec<u64>)| {
            Some((p.strip_prefix(&prefix)?.to_string(), weight(o, now)))
        };
        let mut v: Vec<_> = self.0.iter().filter_map(rel).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    /// Forgets the least likely paths beyond the bound.
    fn trim(&mut self, now: u64) {
        let Some(extra) = self.0.len().checked_sub(MAX_PATHS).filter(|&e| e > 0) else { return };
        let mut v: Vec<_> = self.0.iter().map(|(p, o)| (weight(o, now), p.clone())).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        v.into_iter().take(extra).for_each(|(_, p)| drop(self.0.remove(&p)));
    }

    /// One line a path: the times in hex, a tab, the path.
    pub fn serialize(&self) -> String {
        let line = |(p, opens): (&String, &Vec<u64>)| {
            let times: Vec<String> = opens.iter().map(|t| format!("{t:x}")).collect();
            format!("{}\t{p}\n", times.join(","))
        };
        let mut lines: Vec<String> = self.0.iter().map(line).collect();
        lines.sort_unstable();
        lines.concat()
    }

    /// The inverse of `serialize`. A line that does not parse is skipped, and the bounds are
    /// applied, so a damaged or edited file cannot do harm.
    pub fn parse(text: &str) -> History {
        let mut h = History::default();
        for line in text.lines() {
            let Some((times, path)) = line.split_once('\t') else { continue };
            let opens = times.split(',').map(|t| u64::from_str_radix(t, 16).ok());
            if let Some(mut opens) =
                opens.collect::<Option<Vec<u64>>>().filter(|_| !path.is_empty())
            {
                opens.sort_unstable();
                opens.drain(..opens.len().saturating_sub(MAX_OPENS));
                h.0.insert(path.to_string(), opens);
            }
        }
        let newest = h.0.values().filter_map(|o| o.last()).max().copied().unwrap_or(0);
        h.trim(newest);
        h
    }
}
