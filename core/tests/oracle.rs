//! Differential test against Neovim (`nvim --headless`) as the oracle for common vi commands.
//! Ignored by default: `SIM_ORACLE_CASES=20000 SIM_ORACLE_SEED=1 cargo test --test oracle -- --ignored --nocapture`.
//! `SIM_ORACLE_MODE=single` (one command per case, the default) or `seq` (2-3 commands), `ex` (`:s` and
//! searches) or `reg` (VI-15: yanks, deletes and puts with register names, 3-5 commands, compared by
//! the text, the cursor and what every register holds afterwards).
//! Every divergence is printed grouped by the first command; the test only fails with
//! `SIM_ORACLE_STRICT=1`. Not every divergence is a bug: several are SPEC "Decided" items (see the
//! report), which `known()` filters.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::process::Command;

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> String {
    let b: Vec<u8> =
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect();
    String::from_utf8(b).unwrap()
}

const LUA: &str = r#"
local REGS = { '-', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9' }
for c = string.byte('a'), string.byte('z') do table.insert(REGS, string.char(c)) end
local function dump()
  local out = {}
  for _, r in ipairs({ '"', unpack(REGS) }) do
    local t = vim.fn.getreg(r, 1)
    if t ~= '' then out[#out + 1] = r .. '\31' .. vim.fn.getregtype(r) .. '\31' .. t end
  end
  return table.concat(out, '\30')
end
local function unhex(s) return (s:gsub('..', function(h) return string.char(tonumber(h, 16)) end)) end
local function hex(s) return (s:gsub('.', function(c) return string.format('%02x', c:byte()) end)) end
vim.o.compatible = false
vim.o.more = false
vim.o.cmdheight = 30
vim.o.shortmess = 'aoOstTWIcFS'
vim.o.startofline = true
vim.o.expandtab = true
vim.o.autoindent = true
vim.o.ignorecase = true
vim.o.smartcase = true
vim.o.wrapscan = true
vim.o.whichwrap = ''
vim.o.virtualedit = ''
vim.o.matchpairs = '(:),{:},[:]'
vim.o.undolevels = 1000
local out = io.open(arg[2], 'w')
for line in io.lines(arg[1]) do
  local f = vim.split(line, ' ', { plain = true })
  local text, keys = unhex(f[1]), unhex(f[2])
  local ln, col, sw = tonumber(f[3]), tonumber(f[4]), tonumber(f[5])
  vim.cmd('silent! enew!')
  vim.bo.shiftwidth = sw
  vim.bo.tabstop = 8
  -- Setting the lines is not an undoable change of the "file".
  vim.bo.undolevels = -1
  vim.api.nvim_buf_set_lines(0, 0, -1, false, vim.split(text, '\n', { plain = true }))
  vim.bo.undolevels = 1000
  vim.api.nvim_win_set_cursor(0, { ln + 1, col })
  vim.g.ok = 0
  -- `{}` empties a register; `''` leaves one that holds an empty line, and an append (`"Ayy`) adds to that.
  for _, r in ipairs(REGS) do vim.fn.setreg(r, {}) end
  vim.fn.setreg('"', {})
  vim.fn.setreg('/', '')
  local ok, err = pcall(vim.api.nvim_feedkeys, vim.api.nvim_replace_termcodes(keys .. ':let g:ok=1<CR>', true, true, true), 'xn', false)
  vim.cmd('silent! stopinsert')
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local pos = vim.api.nvim_win_get_cursor(0)
  out:write(hex(table.concat(lines, '\n')), ' ', pos[1] - 1, ' ', pos[2], ' ', vim.fn.mode(), ' ', vim.g.ok, ' ', hex(dump()), '\n')
  out:flush()
end
out:close()
vim.cmd('qa!')
"#;

struct Case {
    text: String,
    line: usize,
    col: usize,
    keys: String,
    sw: usize,
    /// One plain command with no typed text: a failing command flushes nothing that matters.
    simple: bool,
    /// The commands of a register case, one by one (empty for the other modes).
    parts: Vec<String>,
}

const WORDS: &[&str] = &[
    "foo",
    "bar",
    "baz_1",
    "x",
    "a.b",
    "(a, b)",
    "[1]",
    "{k: v}",
    "\"q s\"",
    "'t'",
    "foo(bar)",
    "qux",
    "if",
    "a-b",
    "ab12",
    "...",
    "f(g(h))",
    "\"a\" \"b\"",
    "<t>",
    "one,two",
    "end;",
    "a",
];

fn gen_text(r: &mut Rng, indent: bool) -> String {
    let n = 1 + r.below(4);
    let mut lines = vec![];
    for i in 0..n {
        if r.below(6) == 0 {
            lines.push(String::new());
            continue;
        }
        let mut l = String::new();
        if indent && r.below(2) == 0 {
            l.push_str(&" ".repeat(if i == 0 { 4 } else { 4 * r.below(3) }));
        }
        for k in 0..1 + r.below(4) {
            if k > 0 {
                l.push_str(["", " ", " ", "  "][r.below(4)].max(" "));
            }
            l.push_str(WORDS[r.below(WORDS.len())]);
        }
        if r.below(8) == 0 {
            l.push(' ');
        }
        lines.push(l);
    }
    lines.join("\n")
}

fn pick<'a>(r: &mut Rng, v: &[&'a str]) -> &'a str {
    v[r.below(v.len())]
}

/// One command (possibly with an operator, a count and typed text). `typed`: text for inserts.
fn command(r: &mut Rng, text: &str, seeded: &mut bool) -> String {
    let chars: Vec<char> = text.chars().filter(|c| *c != '\n').collect();
    let ch = |r: &mut Rng| -> char {
        if !chars.is_empty() && r.below(5) != 0 {
            chars[r.below(chars.len())]
        } else {
            ['a', 'o', 'z', '(', ' '][r.below(5)]
        }
    };
    let count = |r: &mut Rng| -> String {
        match r.below(8) {
            0 => "2".into(),
            1 => "3".into(),
            2 => "1".into(),
            3 => "9".into(),
            _ => String::new(),
        }
    };
    let find = |r: &mut Rng| -> String {
        let c = ch(r);
        format!(
            "{}{}",
            pick(r, &["f", "F", "t", "T"]),
            if c == '<' { "<lt>".to_string() } else { c.to_string() }
        )
    };
    let motion = |r: &mut Rng| -> String {
        let m = pick(
            r,
            &[
                "h", "j", "k", "l", "w", "b", "e", "W", "B", "E", "0", "^", "$", "gg", "G", "%",
                "find", "find", "w", "e", "b", "w", "$", "j", "k",
            ],
        );
        let c = count(r);
        if m == "find" { format!("{c}{}", find(r)) } else { format!("{c}{m}") }
    };
    let obj = |r: &mut Rng| -> String {
        format!(
            "{}{}",
            pick(r, &["i", "a"]),
            pick(r, &["w", "W", "\"", "'", "(", ")", "[", "{", "}", "<lt>", "b", "B"])
        )
    };
    let typed = |r: &mut Rng| -> String {
        let t = pick(
            r,
            &[
                "x",
                "xy",
                " z",
                "hello",
                "a b",
                "(",
                "x<CR>y",
                "x<BS>y",
                "ab<C-w>c",
                "<CR>",
                "x<C-u>y",
                "foo bar<C-w>",
            ],
        );
        format!("{t}<Esc>")
    };
    match r.below(30) {
        0..=5 => {
            // operator + motion / object / doubled
            let op = pick(r, &["d", "c", "y", ">", "<", "d", "c", "d"]);
            let op = if op == "<" { "<lt>" } else { op };
            let c = count(r);
            let tgt = match r.below(10) {
                0 | 1 => obj(r),
                2 => op.to_string(),
                3 => pick(r, &["w", "e", "b", "$", "0", "^", "j", "k", "G", "gg"]).to_string(),
                _ => motion(r),
            };
            let mut s = format!("{c}{op}{tgt}");
            if op == "c" {
                s += &typed(r);
            }
            s
        }
        6 => format!("{}x", count(r)),
        7 => format!("{}r{}", count(r), {
            let c = ch(r);
            if c == '<' { "<lt>".into() } else { c.to_string() }
        }),
        8 => format!("{}J", count(r)),
        9 => format!("{}{}", count(r), pick(r, &["p", "P"])),
        10 => "u".into(),
        11 => "<C-r>".into(),
        12 => format!(
            "{}.",
            pick(
                r,
                &[
                    "dw",
                    "x",
                    "dd",
                    "cwz<Esc>",
                    "ix<Esc>",
                    "J",
                    "<lt>j",
                    ">>",
                    "p",
                    "A!<Esc>",
                    "ciwq<Esc>",
                    "2x",
                    "o<Esc>"
                ]
            )
        ),
        13 => format!("{}{}{}", count(r), pick(r, &["i", "a", "I", "A", "o", "O"]), typed(r)),
        14 => pick(r, &["D", "C", "yy", "yw", "y$", "yiw"]).to_string().replace("C", "Cq<Esc>"),
        15 => format!("{}{}", count(r), pick(r, &["dd", "cc", "yy", ">>", "<lt><lt>"]))
            .replace("cc", "ccq<Esc>"),
        16..=19 => motion(r),
        20 => format!(
            "{}{}{}",
            pick(r, &["v", "V"]),
            motion(r),
            pick(r, &["d", "y", "c", ">", "<lt>"])
        )
        .replace("c", "cq<Esc>"),
        21 => format!("{}{}{}", pick(r, &["v", "V"]), obj(r), pick(r, &["d", "y", "c"]))
            .replace("c", "cq<Esc>"),
        22 => format!("v{}o{}{}", motion(r), motion(r), pick(r, &["d", "y"])),
        23 => {
            *seeded = true;
            format!("{};{}", find(r), pick(r, &[";", ",", "d;", "d,", "c;q<Esc>"]))
        }
        24 => {
            *seeded = true;
            let w = WORDS[r.below(WORDS.len())]
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(2)
                .collect::<String>()
                .to_lowercase();
            format!("/{w}<CR>{}", pick(r, &["n", "N", "dn", "cnq<Esc>", ""]))
        }
        25 => {
            *seeded = true;
            format!("{}{}", pick(r, &["*", "#"]), pick(r, &["", "n", "N", "dn"]))
        }
        26 => format!("{}%", count(r)),
        27 => format!(
            "{}{}",
            pick(r, &["d", "y", "c"]),
            pick(r, &["%", "iw", "aw", "i(", "a(", "i\"", "a\""])
        )
        .replace("c", "cq<Esc>"),
        28 => format!(":{}<CR>", 1 + r.below(4)),
        _ => format!("{}dd{}", count(r), pick(r, &["p", "P", "u", ""])),
    }
}

/// `:s` and search commands, in the regex subset where vim (magic) and regex-lite agree.
fn ex_command(r: &mut Rng) -> String {
    let pats = [
        "a", "o", "foo", "b.", "^", "$", "x*", "[ab]", "\\.", "ba[rz]", "o*", "^f", "r$", " ",
        "  ", "..", "a*b", ".*", "[^ ]*", "qu",
    ];
    let reps = ["X", "&&", "[&]", "\\&", "", " ", "\\/", "a\\/b", "-&-", "&\\&", "\\\\", "ab"];
    let range = pick(r, &["", "", "%", ".", "1,2", "2,3", "$", "1", ",$", "3,1", ".,$", "2"]);
    match r.below(8) {
        0..=4 => format!(
            ":{range}s/{}/{}/{}<CR>",
            pick(r, &pats),
            pick(r, &reps),
            pick(r, &["", "g", "g"])
        ),
        5 => format!(":{range}s/{}/{}<CR>", pick(r, &pats), pick(r, &reps)),
        6 => format!(
            "/{}<CR>{}",
            pick(r, &["a", "foo", "o", "^f", "r$", "ba", "x", "qux", "b.r", "$", "^", "[ab]"]),
            pick(r, &["", "n", "N", "nn", "dn", "x", "cnQ<Esc>"])
        ),
        _ => format!(
            "?{}<CR>{}",
            pick(r, &["a", "foo", "o", "ba", "x", "qux", "$", "^"]),
            pick(r, &["", "n", "N", "nn", "dn", "x"])
        ),
    }
}

/// The registers a case has written so far (their names as typed after `"`), and whether it has changed
/// the text: a read names one of the first, and `.` follows the second (nvim's `.` is the last change of
/// any case before, so a `.` with no change of this case's own would repeat a stranger's).
#[derive(Default)]
struct Reg {
    filled: Vec<&'static str>,
    changed: bool,
}

/// One command of the register mode: a yank, a delete, a change or a put, with or without a register
/// name and a count (before the name or after it), or `u` and `.`.
fn reg_command(r: &mut Rng, st: &mut Reg) -> String {
    let write = |r: &mut Rng| {
        pick(r, &["", "", "", "\"a", "\"b", "\"A", "\"B", "\"_", "\"0", "\"1", "\"-"])
    };
    let count = |r: &mut Rng| pick(r, &["", "", "", "", "2", "3"]);
    // A yank with a motion that goes up or to the ends of the file leaves vim's cursor at the start of
    // the lines, and sim's in its column: that is not the registers' (and the cursor drives what comes
    // next), so a yank has no such motion here.
    let target = |r: &mut Rng, yank: bool| {
        let t = pick(
            r,
            &[
                "w", "e", "b", "W", "$", "j", "k", "G", "gg", "%", "iw", "l", "w", "e", "iw", "n",
                "N",
            ],
        );
        if yank && matches!(t, "k" | "G" | "gg") { "w" } else { t }
    };
    // A count before `%`, `*`, `G` and the like is another thing in vim (a place in the file, or the nth
    // match): VI-1 decided to ignore it; a count before `$` reaches the next lines.
    let counted = |c: &'static str, t: &str| {
        if matches!(t, "w" | "e" | "b" | "W" | "l" | "j" | "iw" | "aw") { c } else { "" }
    };
    let note = |st: &mut Reg, name: &str, yank: bool, small: bool| {
        let names: &[&'static str] = match (name, yank, small) {
            ("", true, _) => &["", "\"0"],
            ("", false, true) => &["", "\"-"],
            ("", false, false) => &["", "\"1"],
            ("\"A", ..) | ("\"a", ..) => &["\"a"],
            ("\"B", ..) | ("\"b", ..) => &["\"b"],
            ("\"0", ..) => &["\"0"],
            ("\"1", ..) => &["\"1"],
            ("\"-", ..) => &["\"-"],
            _ => &[],
        };
        st.filled.extend(names);
        if !yank && !small {
            st.filled.push("\"1");
        }
    };
    let (w, c, yank) = (write(r), count(r), r.below(2) == 0);
    let t = target(r, yank);
    let c = counted(c, t);
    let (n, nn) = (st.filled.len(), st.filled.len().max(1));
    match r.below(26) {
        0..=3 if yank => {
            note(st, w, true, false);
            format!("{w}{c}y{t}")
        }
        4 | 5 => {
            note(st, w, true, false);
            format!("{w}{c}yy")
        }
        0..=3 | 6..=9 => {
            note(st, w, false, !matches!(t, "j" | "k" | "G" | "gg" | "%" | "n" | "N"));
            st.changed = true;
            format!("{w}{c}d{t}")
        }
        10 | 11 => {
            note(st, w, false, false);
            st.changed = true;
            format!("{w}{c}dd")
        }
        12 => {
            note(st, w, false, true);
            st.changed = true;
            format!("{w}cw{}<Esc>", pick(r, &["q", "xy", "a b"]))
        }
        13 => {
            note(st, w, false, true);
            st.changed = true;
            format!("{w}{}", pick(r, &["x", "D", "dl"]))
        }
        14 => {
            note(st, w, true, false);
            format!("{w}yl")
        }
        15..=21 if n > 0 => {
            let name = st.filled[r.below(nn)];
            let p = pick(r, &["p", "P"]);
            st.changed = true;
            if r.below(2) == 0 { format!("{c}{name}{p}") } else { format!("{name}{c}{p}") }
        }
        22 if st.changed => ".".into(),
        23 if st.changed => ".".into(),
        24 if n > 0 => {
            st.changed = true;
            pick(r, &["\"1pu.u.", "\"1p..", "\"2pu.", "\"1Pu.u.u."]).into()
        }
        _ => {
            note(st, w, true, false);
            format!("{w}yy")
        }
    }
}

fn gen_case(r: &mut Rng, multi: bool) -> Case {
    let indent = r.below(3) == 0;
    let text = gen_text(r, indent);
    if std::env::var("SIM_ORACLE_MODE").is_ok_and(|m| m == "reg") {
        // (no empty line: a yank that takes nothing is the Decided rule of VI-8, see `degenerate`)
        let text = text
            .split('\n')
            .map(|l| if l.is_empty() { "x" } else { l })
            .collect::<Vec<_>>()
            .join("\n");
        let lines: Vec<&str> = text.split('\n').collect();
        let line = r.below(lines.len());
        let col = if lines[line].is_empty() { 0 } else { r.below(lines[line].len()) };
        let mut st = Reg::default();
        let parts: Vec<String> = (0..3 + r.below(3)).map(|_| reg_command(r, &mut st)).collect();
        let keys = parts.concat() + "<Esc>";
        return Case { text, line, col, keys, sw: 4, simple: false, parts };
    }
    if std::env::var("SIM_ORACLE_MODE").is_ok_and(|m| m == "ex") {
        let lines: Vec<&str> = text.split('\n').collect();
        let line = r.below(lines.len());
        let col = if lines[line].is_empty() { 0 } else { r.below(lines[line].len()) };
        let keys = format!("{}<Esc>", ex_command(r));
        let simple = keys.matches("<CR>").count() == 1 && keys.starts_with(':');
        return Case { text, line, col, keys, sw: 4, simple, parts: vec![] };
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let line = r.below(lines.len());
    let col = if lines[line].is_empty() { 0 } else { r.below(lines[line].len()) };
    let n = if multi { 2 + r.below(2) } else { 1 };
    let mut keys = String::new();
    let mut seeded = false;
    for _ in 0..n {
        let c = command(r, &text, &mut seeded);
        keys += &c;
    }
    // Repeat only after a change in the same case: put a change first when `.` is wanted.
    if r.below(10) == 0 {
        keys = format!("{}{keys}.", pick(r, &["dw", "x", "cwq<Esc>", "A!<Esc>", "dd", "J", ">>"]));
    }
    // The first indented line decides si's shift unit (VI-6); vim gets the same shiftwidth.
    let sw = lines
        .iter()
        .find(|l| l.starts_with(' ') && l.chars().any(|c| c != ' '))
        .map_or(4, |l| l.chars().take_while(|&c| c == ' ').count());
    let simple = !multi
        && !keys.contains("<Esc>")
        && !keys.contains("<CR>")
        && !keys.starts_with(['v', 'V'])
        && !keys.contains(';')
        && !keys.contains(',')
        && !keys.ends_with('.');
    keys += "<Esc>";
    Case { text, line, col, keys, sw, simple, parts: vec![] }
}

type Mine = (String, usize, usize, String);

fn run_si(c: &Case) -> Result<Mine, String> {
    // A watchdog: a case that spins is reported as HANG instead of stalling the run.
    let (text, line, col, keys, parts) =
        (c.text.clone(), c.line, c.col, c.keys.clone(), c.parts.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let c = Case { text, line, col, keys, sw: 0, simple: false, parts };
        let _ = tx.send(run_si_inner(&c));
    });
    rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap_or_else(|_| Err("HANG".to_string()))
}

/// What every register holds, as the Lua's `dump` writes it: the unnamed one, `-`, `0`-`9`, `a`-`z`; each
/// as name, type (`V` whole lines, `v` chars) and text; the ones that hold nothing left out.
fn dump_regs(a: &mut sim_core::core::app::App) -> String {
    let regs = a.take_regs();
    let dumped = dump_of(&regs);
    a.put_regs(regs);
    dumped
}

fn dump_of(regs: &sim_core::core::reg::Regs) -> String {
    let names = ['"', '-', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
    let names: Vec<char> = names.into_iter().chain('a'..='z').collect();
    let one = |n: char| {
        let name = (n != '"').then_some(n);
        regs.get(name).map(|(t, lw)| format!("{n}\u{1f}{}\u{1f}{t}", if *lw { 'V' } else { 'v' }))
    };
    names.into_iter().filter_map(one).collect::<Vec<_>>().join("\u{1e}")
}

/// Whether a register case is one the two editors are not to agree on by a rule of ours: a command that
/// takes nothing (VI-8 Decided: the registers stay as they were, where vim's empty yank empties its
/// register and points the unnamed one to it), a buffer that has nothing left in it, or a `d{motion}`
/// over more than one line (vim makes it linewise when only blanks come before it and nothing after;
/// sim does not, and the register's type and the text follow from that: not the registers' doing).
fn degenerate(c: &Case) -> bool {
    let run = || {
        let mut a = app_with(None, format!("{}\n", c.text).as_bytes(), (80, 24));
        let idx: usize =
            c.text.split('\n').take(c.line).map(|l| l.chars().count() + 1).sum::<usize>() + c.col;
        a.buf.cursor = idx;
        for part in &c.parts {
            let before = (text(&a), dump_regs(&mut a));
            feed(&mut a, part);
            let after = (text(&a), dump_regs(&mut a));
            let edits = !part.contains(['p', 'P', '.']) && !part.contains("\"_");
            let over_lines = |d: &str| {
                d.split('\u{1e}').any(|e| e.contains("\u{1f}v\u{1f}") && e.contains('\n'))
            };
            // (a delete that took a line break with a motion that is no line motion: it went over lines,
            // into the black hole or into a register alike)
            let linewise = ["dd", "dj", "dk", "dG", "dgg"].iter().any(|k| part.contains(k));
            let lost = |t: &str| t.matches('\n').count();
            let d_over = part.contains('d')
                && !linewise
                && (over_lines(&after.1) || lost(&after.0) < lost(&before.0));
            if (edits && before == after) || d_over || text(&a).trim_matches('\n').is_empty() {
                return true;
            }
        }
        false
    };
    std::panic::catch_unwind(run).unwrap_or(true)
}

fn run_si_inner(c: &Case) -> Result<Mine, String> {
    let r = std::panic::catch_unwind(|| {
        // The file ends with a newline: its text is the lines joined by newlines (as nvim's).
        let mut a = app_with(None, format!("{}\n", c.text).as_bytes(), (80, 24));
        let idx: usize =
            c.text.split('\n').take(c.line).map(|l| l.chars().count() + 1).sum::<usize>() + c.col;
        a.buf.cursor = idx;
        feed(&mut a, &c.keys);
        let (t, regs) = (text(&a), dump_regs(&mut a));
        let b = &a.buf;
        let l = b.line_of(b.cursor);
        (t, l, b.cursor - b.start(l), regs)
    });
    r.map_err(|_| "PANIC".to_string())
}

/// Divergences that are SPEC "Decided" items or known vim extras, not defects:
/// counts on text objects and `%` are ignored (VI-1), a count on an insert command is not
/// implemented, Visual-mode operations are not repeated by `.` (VI-2).
fn known(keys: &str) -> Option<&'static str> {
    let k = keys.trim_end_matches("<Esc>");
    let b: Vec<char> = k.chars().collect();
    for i in 1..b.len() {
        if b[i - 1].is_ascii_digit() && b[i..].starts_with(&['%']) {
            return Some("count before % (VI-1 Decided)");
        }
        if b[i - 1].is_ascii_digit()
            && matches!(b[i], 'd' | 'c' | 'y' | '>')
            && matches!(b.get(i + 1), Some('i' | 'a'))
        {
            return Some("count on a text object (VI-1 Decided)");
        }
        if b[i - 1].is_ascii_digit()
            && matches!(b[i], 'i' | 'a' | 'I' | 'A' | 'o' | 'O')
            && (i < 2 || !b[i - 2].is_ascii_alphabetic())
        {
            return Some("count on an insert command");
        }
    }
    if k.contains(['v', 'V']) && (k.ends_with('.') || k.contains("<Esc>.")) {
        return Some("Visual op then . (VI-2 Decided)");
    }
    None
}

fn show(s: &str) -> String {
    s.replace('\n', "⏎")
}

/// The registers of a case that differs, for the report (only in the register mode).
fn regs_of(reg_mode: bool, regs: &str) -> String {
    if reg_mode {
        format!(" regs {:?}", show(&regs.replace('\u{1f}', ":").replace('\u{1e}', " | ")))
    } else {
        String::new()
    }
}

#[test]
#[ignore = "needs nvim; run on demand"]
fn vi_1_oracle_against_nvim() {
    if Command::new("nvim").arg("--version").output().is_err() {
        eprintln!("skipped: nvim is not installed");
        return;
    }
    let n: usize =
        std::env::var("SIM_ORACLE_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(5000);
    let seed: u64 = std::env::var("SIM_ORACLE_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let multi = std::env::var("SIM_ORACLE_MODE").is_ok_and(|m| m == "seq");
    let reg_mode = std::env::var("SIM_ORACLE_MODE").is_ok_and(|m| m == "reg");
    let mut r = Rng(0x9e37_79b9_7f4a_7c15 ^ seed.wrapping_mul(0x2545_f491_4f6c_dd1d));
    let cases: Vec<Case> = (0..n).map(|_| gen_case(&mut r, multi)).collect();
    let dir = temp_dir(&format!("oracle-{seed}-{multi}"));
    let (inp, outp, lua) = (dir.0.join("in.txt"), dir.0.join("out.txt"), dir.0.join("run.lua"));
    std::fs::write(&lua, LUA).unwrap();
    // nvim runs the cases in chunks; one that stalls (waiting for input) is skipped and reported.
    let mut out = String::new();
    let mut hangs = vec![];
    let mut from = 0;
    while from < cases.len() {
        let mut s = String::new();
        for c in &cases[from..] {
            writeln!(s, "{} {} {} {} {}", hex(&c.text), hex(&c.keys), c.line, c.col, c.sw).unwrap();
        }
        std::fs::write(&inp, s).unwrap();
        let _ = std::fs::remove_file(&outp);
        let st = Command::new("timeout")
            .args(["60", "nvim", "--headless", "-u", "NONE", "-i", "NONE", "-n", "-l"])
            .arg(&lua)
            .arg(&inp)
            .arg(&outp)
            .output()
            .unwrap();
        let part = std::fs::read_to_string(&outp).unwrap_or_default();
        let done = part.lines().count();
        out += &part;
        from += done;
        if (!st.status.success() || done == 0) && from < cases.len() {
            hangs.push(format!("{:?} on {:?}", cases[from].keys, show(&cases[from].text)));
            out += &format!("{} 0 0 n 0\n", hex("\u{1}HANG"));
            from += 1;
        }
    }
    eprintln!("nvim stalled on {} cases: {hangs:?}", hangs.len());
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), cases.len());
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut text_div, mut cur_div, mut panics, mut skipped) = (0, 0, 0, 0);
    let mut known_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (c, l) in cases.iter().zip(lines) {
        let f: Vec<&str> = l.split(' ').collect();
        if unhex(f[0]) == "\u{1}HANG" {
            continue;
        }
        let (vt, vl, vc) =
            (unhex(f[0]), f[1].parse::<usize>().unwrap(), f[2].parse::<usize>().unwrap());
        let vregs = unhex(f.get(5).copied().unwrap_or(""));
        let key = c.keys.trim_end_matches("<Esc>").to_string();
        if std::env::var_os("SIM_ORACLE_VISUAL").is_none() && c.keys.contains(['v', 'V']) {
            *known_counts.entry("visual (SIM_ORACLE_VISUAL=1 to include)").or_default() += 1;
            continue;
        }
        if let Some(why) = known(&c.keys) {
            *known_counts.entry(why).or_default() += 1;
            continue;
        }
        if reg_mode && degenerate(c) {
            *known_counts
                .entry("a command took nothing, or the buffer is empty (VI-8 Decided)")
                .or_default() += 1;
            continue;
        }
        let flushed = f[4] == "0";
        if flushed && !c.simple {
            skipped += 1;
            continue;
        }
        let mine = match run_si(c) {
            Ok(m) => m,
            Err(e) => {
                panics += 1;
                groups
                    .entry("PANIC".into())
                    .or_default()
                    .push(format!("{:?} keys {key} -> {e}", show(&c.text)));
                continue;
            }
        };
        let kind = if mine.0 != vt {
            text_div += 1;
            "TEXT"
        } else if (mine.1, mine.2) != (vl, vc) && !reg_mode {
            cur_div += 1;
            "CURSOR"
        } else if reg_mode && mine.3 != vregs {
            text_div += 1;
            "REGS"
        } else {
            continue;
        };
        let kind = if flushed { format!("{kind}(nvim aborted)") } else { kind.to_string() };
        let cat = key.clone();
        groups.entry(format!("{kind} {cat}")).or_default().push(format!(
            "text {:?} @({},{}) sw{} keys {key}\n      si   {:?} @({},{}){}\n      nvim {:?} @({},{}){}",
            show(&c.text), c.line, c.col, c.sw, show(&mine.0), mine.1, mine.2, regs_of(reg_mode, &mine.3), show(&vt), vl, vc, regs_of(reg_mode, &vregs)
        ));
    }
    eprintln!(
        "{n} cases ({}): {text_div} text divergences, {cur_div} cursor-only, {panics} si panics, {skipped} skipped (nvim flushed its keys after a failing command)",
        if multi { "seq" } else { "single" }
    );
    eprintln!("known/decided, not compared: {known_counts:?}");
    let show_n: usize =
        std::env::var("SIM_ORACLE_SHOW").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let _ = show_n;
    for (k, v) in &groups {
        for e in v {
            eprintln!("DIV {k} | {}", e.replace("\n      ", " || "));
        }
    }
    if std::env::var_os("SIM_ORACLE_STRICT").is_some() {
        assert_eq!(text_div + panics, 0);
    }
}
