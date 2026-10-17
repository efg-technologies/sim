//! MD-10
//! A link that says one place and goes to another (pure): what the text of a link names and where its
//! target goes, as a browser and a renderer read them; the links of a Markdown line, of the paragraph
//! around it, of raw HTML and of the references of the document (`Refs`); and what is not judged because
//! it is past a limit, which is marked and not trusted. The walk of `layout` asks `line_marks` for every
//! line it draws; the read view of `md.rs` hides none of what is marked.
use crate::paint::Look;
use crate::syn::MAX_LINE;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

/// MD-10: the top-level domains that make a dotted name a host (a file or a product has a dot too).
#[rustfmt::skip]
const TLDS: &[&str] = &[
    "com", "org", "net", "edu", "gov", "mil", "int", "io", "dev", "app", "ai", "co", "jp", "uk", "de", "fr",
    "info", "xyz", "me", "us", "eu", "ru", "cn", "in", "br", "ca", "au", "nl", "se", "ch", "example",
    "biz", "name", "pro", "mobi", "asia", "tel", "travel", "jobs", "page", "cloud", "online", "site",
    "store", "tech", "shop", "blog", "live", "news", "link", "click", "top", "vip", "icu", "club", "space",
    "website", "world", "today", "one", "fun", "life", "email", "works", "wiki", "onion", "local",
    "internal", "lan", "corp", "home", "systems", "services", "solutions", "network", "digital", "agency",
    "design", "studio", "media", "finance", "money", "bank", "pay", "security", "support", "host",
    "download", "software", "games", "chat", "bot",
];

/// MD-10: the country codes (ISO 3166-1, as the top-level domains that are made of them): a name that ends
/// with one is a host, unless it is one of `EXTS`. `selection.on` and `x.zz` are not hosts.
#[rustfmt::skip]
const CCTLDS: &str = "ac ad ae af ag ai al am ao aq ar as at au aw ax az ba bb bd be bf bg bh bi bj bm bn bo br bs bt bw by bz \
    ca cc cd cf cg ch ci ck cl cm cn co cr cu cv cw cx cy cz de dj dk dm do dz ec ee eg er es et eu fi fj fk fm fo fr \
    ga gd ge gf gg gh gi gl gm gn gp gq gr gs gt gu gw gy hk hm hn hr ht hu id ie il im in io iq ir is it je jm jo jp \
    ke kg kh ki km kn kp kr kw ky kz la lb lc li lk lr ls lt lu lv ly ma mc md me mg mh mk ml mm mn mo mp mq mr ms mt \
    mu mv mw mx my mz na nc ne nf ng ni nl no np nr nu nz om pa pe pf pg ph pk pl pm pn pr ps pt pw py qa re ro rs ru \
    rw sa sb sc sd se sg sh si sk sl sm sn so sr ss st su sv sx sy sz tc td tf tg th tj tk tl tm tn to tr tt tv tw tz \
    ua ug uk us uy uz va vc ve vg vi vn vu wf ws ye yt za zm zw";

/// MD-10: `CCTLDS` as a table by the two letters (`a` is 0).
const COUNTRY: [bool; 676] = {
    let (b, mut t, mut i) = (CCTLDS.as_bytes(), [false; 676], 0);
    while i + 1 < b.len() {
        if b[i] != b' ' && b[i + 1] != b' ' && (i == 0 || b[i - 1] == b' ') {
            t[(b[i] - b'a') as usize * 26 + (b[i + 1] - b'a') as usize] = true;
        }
        i += 1;
    }
    t
};

/// MD-10: the two-letter last labels that end the name of a source file far more often than a
/// country's host (`main.rs`, `README.md`, `Makefile.am`, `Node.js`): a name that ends so is a file's.
const EXTS: &[&str] = &[
    "rs", "md", "py", "sh", "pl", "ps", "cc", "so", "mk", "mm", "am", "ac", "ml", "pm", "tf", "gd",
    "kt", "sv", "hs", "ts", "js", "cs", "rb", "go", "el", "ex", "jl", "vb",
];

/// MD-10: how many lines and chars of a document are looked through for its reference links (once for a
/// version of its text), how much of a line is read (the highlighter's own limit, SYN-3), the longest
/// link text and the longest target that are judged, and the target that no use can be judged by (it
/// makes every use of its label lie).
pub const REF_LINES: usize = 2_000;
const REF_CHARS: usize = 120_000;
const LINE: usize = MAX_LINE;
const TEXT: usize = 1_000;
const DEST: usize = 2_048;
const UNJUDGED: &str = "javascript:";
/// How many lines after a line with an `<a` that is not closed on it the paragraph reaches for its `</a`.
const AFTER_TAG: usize = 12;

/// MD-10: what a frame may judge: how many links, and how many chars of them. What is past the budget,
/// like a text or a target past its limit, is not judged and is not trusted: it is marked.
pub struct Budget {
    judged: usize,
    chars: usize,
    /// the last line that the walk asked about, and whether it ends inside a tag of raw HTML that is not closed
    tag: (usize, bool),
}

impl Budget {
    pub fn frame() -> Budget {
        Budget::new(512, 200_000)
    }

    pub fn new(judged: usize, chars: usize) -> Budget {
        Budget { judged, chars, tag: (usize::MAX, false) }
    }

    /// Takes `n` chars of the budget, if it has them.
    fn spend(&mut self, n: usize) -> bool {
        let ok = self.chars >= n;
        self.chars -= n * usize::from(ok);
        ok
    }

    /// Whether a link of a text of `text` chars and a target of `target` may be judged: it is not too
    /// long, and the frame has the budget for it (a judgment reads the text and the target a few times).
    fn allow(&mut self, text: usize, target: usize) -> bool {
        let ok =
            text <= TEXT && target <= DEST && self.judged > 0 && self.spend(8 * (text + target));
        self.judged -= usize::from(ok);
        ok
    }

    /// Whether the link lies, or cannot be judged (`md`: its target is a Markdown one, which a renderer
    /// writes into an href; a target of raw HTML is not).
    fn judge(&mut self, text: &[char], target: &[char], md: bool) -> bool {
        !self.allow(text.len(), target.len())
            || lies(&text.iter().collect::<String>(), &target.iter().collect::<String>(), md)
    }
}

/// MD-10: what a word of a link's text names: a host (as it is compared, and as it is written), a URL
/// with an authority that cannot be read, or a path (and whether it is absolute).
enum Names {
    Host(String, String),
    /// a host with letters that are not ASCII (as lower case, without `www.`)
    Idn(String),
    Odd,
    Path(String, bool),
}

/// MD-10: where a target goes: to a host, to a host as written with letters that are not ASCII, to a host
/// that cannot be read or is not known, to the hosts of the boxes of a `mailto:`, to nothing that a host
/// names (a scheme that is neither special nor followed by `//`: `tel:`, `search-ms:`), or nowhere
/// (relative: it has no scheme).
enum Dest {
    Host(String),
    Wide(String),
    Odd,
    Mail(Vec<String>),
    Opaque,
    Relative,
}

/// MD-10: `s` with the fullwidth forms of ASCII (U+FF01 to U+FF5E) as ASCII, and the full stops of other
/// scripts as dots: what a browser makes of them in a host, and what a reader reads.
fn map_wide(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            '\u{3002}' | '\u{ff61}' | '\u{2024}' | '\u{fe52}' => '.',
            _ => c,
        })
        .collect()
}

/// MD-10: `l` with the letters of other scripts that look like an ASCII one as that one (Cyrillic and Greek
/// letters that stand in for `a c d e g h i j k l m o p q s t v w x y`): so that a top-level domain with
/// one of them in it is read for what a reader sees. Letters that look like none stay as they are.
fn skeleton(l: &str) -> String {
    const LIKE: &str = "а:a с:c е:e о:o р:p х:x у:y і:i ѕ:s ј:j ԁ:d һ:h ԛ:q ԝ:w ӏ:l к:k м:m т:t ο:o ν:v ρ:p ι:i κ:k α:a ɑ:a ɡ:g ı:i";
    l.chars()
        .map(|c| {
            LIKE.split(' ')
                .find_map(|p| p.strip_prefix(c).and_then(|r| r.strip_prefix(':')))
                .and_then(|a| a.chars().next())
                .unwrap_or(c)
        })
        .collect()
}

/// MD-10: whether `l` is a country code (not one that ends a source file: `EXTS`).
fn cc_tld(l: &str) -> bool {
    let b = l.as_bytes();
    b.len() == 2
        && b.iter().all(u8::is_ascii_lowercase)
        && COUNTRY[(b[0] - b'a') as usize * 26 + (b[1] - b'a') as usize]
        && !EXTS.contains(&l)
}

/// MD-10: the second labels of the countries' suffixes of the shape `co.uk`, `com.au`, `or.jp`.
const SECOND: &str = "co com net org edu gov ac ad or ne go gr lg ed mil nom ltd plc sch gob gouv";

/// MD-10: suffixes under which anyone can register a name, so that a subdomain of one is no one owner's:
/// chosen from the private section of the Public Suffix List, by how often a platform's name is linked to
/// (a README names its host, a page of its docs, a project's page); not the whole list, which is some
/// 9,000 names and has no place in the editor. A host on it, a host of the shape `co.uk`, and a host under
/// `amazonaws.com` are trusted for themselves only.
const SHARED: &str = "github.io githubusercontent.com gitlab.io vercel.app netlify.app herokuapp.com \
    pages.dev workers.dev web.app firebaseapp.com appspot.com blogspot.com azurewebsites.net cloudfront.net \
    readthedocs.io wordpress.com ngrok.io ngrok-free.app onrender.com fly.dev glitch.me surge.sh bitbucket.io \
    translate.goog myshopify.com wixsite.com notion.site substack.com r2.dev deno.dev now.sh repl.co gitpod.io \
    run.app cloudfunctions.net googleusercontent.com pythonanywhere.com tumblr.com weebly.com";

/// MD-10: whether `h` is a suffix that many share (`SHARED`, `SECOND` under a country, `amazonaws.com`).
fn shared(h: &str) -> bool {
    let two: Vec<&str> = h.split('.').collect();
    let country =
        two.len() == 2 && SECOND.split_whitespace().any(|g| g == two[0]) && cc_tld(two[1]);
    let numbers = h.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    country
        || numbers
        || h == "amazonaws.com"
        || h.ends_with(".amazonaws.com")
        || SHARED.split_whitespace().any(|x| x == h)
}

/// MD-10: whether `h` is, or is under, a suffix of `SHARED` (`surge.sh` is a host though `sh` ends a source file).
fn on_shared(h: &str) -> bool {
    let h = h.to_ascii_lowercase();
    SHARED
        .split_whitespace()
        .any(|x| h.strip_suffix(x).is_some_and(|p| p.is_empty() || p.ends_with('.')))
}

/// MD-10: the punycode (RFC 3492) of `label`, which is lower case; without its `xn--`.
fn punycode(label: &str) -> Option<String> {
    let adapt = |mut delta: u32, n: u32, first: bool| {
        delta = if first { delta / 700 } else { delta / 2 };
        delta += delta / n;
        let mut k = 0;
        while delta > 35 * 26 / 2 {
            (delta, k) = (delta / 35, k + 36);
        }
        k + 36 * delta / (delta + 38)
    };
    let digit = |d: u32| char::from(if d < 26 { b'a' + d as u8 } else { b'0' + (d - 26) as u8 });
    let cps: Vec<u32> = label.chars().map(u32::from).collect();
    let mut out: String = label.chars().filter(char::is_ascii).collect();
    let basic = out.len() as u32;
    let (mut h, mut n, mut delta, mut bias) = (basic, 128u32, 0u32, 72u32);
    out.extend(['-'].into_iter().filter(|_| basic > 0));
    while (h as usize) < cps.len() {
        let m = cps.iter().copied().filter(|&c| c >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(h + 1)?)?;
        n = m;
        for &c in &cps {
            delta += u32::from(c < n);
            if c == n {
                let (mut q, mut k) = (delta, 36);
                loop {
                    let t = if k <= bias {
                        1
                    } else if k >= bias + 26 {
                        26
                    } else {
                        k - bias
                    };
                    if q < t {
                        break;
                    }
                    out.push(digit(t + (q - t) % (36 - t)));
                    (q, k) = ((q - t) / (36 - t), k + 36);
                }
                out.push(digit(q));
                bias = adapt(delta, h + 1, h == basic);
                (delta, h) = (0, h + 1);
            }
        }
        (delta, n) = (delta + 1, n + 1);
    }
    Some(out)
}

/// MD-10: whether the host `target`, in ASCII, is the host `u` as written (lower case, letters that are not
/// ASCII in some labels): every label the same, or the `xn--` of its punycode.
fn encodes(u: &str, target: &str) -> bool {
    let (ul, tl): (Vec<&str>, Vec<&str>) = (u.split('.').collect(), target.split('.').collect());
    ul.len() == tl.len()
        && ul.iter().zip(&tl).all(|(a, b)| {
            if a.is_ascii() {
                a == b
            } else {
                punycode(a).is_some_and(|p| *b == format!("xn--{p}"))
            }
        })
}

fn scheme_ok(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// MD-10: the host of an authority as a browser reads it: what follows the last `@`, then an optional
/// port (digits), in lower case without a trailing dot or `www.`. Only ASCII letters, digits, `-`, `_`
/// and `.` (or an IPv6 address in brackets) are a host: a `%`, a `\`, a control char, a blank, an
/// entity or a char that is not ASCII is odd, and an odd host is never taken for another.
fn authority(a: &str) -> Dest {
    let hp = a.rsplit('@').next().unwrap_or_default();
    let (host, port) = match hp.rsplit_once(':') {
        Some((h, p)) if !hp.starts_with('[') || h.ends_with(']') => (h, Some(p)),
        _ => (hp, None),
    };
    if port.is_some_and(|p| !p.bytes().all(|b| b.is_ascii_digit())) {
        return Dest::Odd;
    }
    if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        let ok =
            !v6.is_empty() && v6.bytes().all(|b| b.is_ascii_hexdigit() || matches!(b, b':' | b'.'));
        return if ok { Dest::Host(host.to_ascii_lowercase()) } else { Dest::Odd };
    }
    let h = host.strip_suffix('.').unwrap_or(host);
    let valid = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.');
    if h.is_empty() {
        return Dest::Odd;
    }
    if !h.bytes().all(valid) {
        // (a host as written with letters that are not ASCII: judged only against a text that names such a
        // host; to any other it is odd)
        let m = map_wide(h).to_lowercase();
        let label =
            |l: &str| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || "-_".contains(c));
        return if !m.is_ascii() && m.split('.').all(label) {
            Dest::Wide(m.strip_prefix("www.").map_or(m.clone(), str::to_string))
        } else {
            Dest::Odd
        };
    }
    let h = h.to_ascii_lowercase();
    Dest::Host(h.strip_prefix("www.").map_or(h.clone(), str::to_string))
}

/// MD-10: `raw` without the tab and newline a browser drops, and without the controls and blanks it
/// trims at the ends.
fn cleaned(raw: &str) -> String {
    let t: String = raw.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    t.trim_matches(|c: char| c.is_control() || c == ' ').to_string()
}

/// MD-10: a URL as the WHATWG standard reads it, so that sim compares the host a browser goes to: the
/// special schemes (http, https, ws, wss, ftp, file) take any number of `/` and `\` after the colon, or
/// none, and end the authority at `/`, `\`, `?` or `#`; a target with no scheme that starts with two
/// slashes of either kind is scheme-relative; the userinfo is what precedes the last `@`. Where it goes,
/// and the path after the authority (the whole target where it goes nowhere).
fn url(raw: &str) -> (Dest, String) {
    let t = cleaned(raw.trim().trim_matches(['<', '>']));
    let (scheme, rest) = match t.split_once(':') {
        Some((s, r)) if scheme_ok(s) => (Some(s.to_ascii_lowercase()), r),
        _ => (None, t.as_str()),
    };
    let special = scheme
        .as_deref()
        .is_some_and(|s| matches!(s, "http" | "https" | "ws" | "wss" | "ftp" | "file"));
    let slash = |c: char| c == '/' || c == '\\';
    let slashes = rest.chars().take_while(|&c| slash(c)).count();
    // (a special scheme with fewer than two slashes is resolved against the base: the document's own host,
    // or the next one where there is none; a scheme that is neither special nor followed by `//` is opaque;
    // only a target with no scheme is relative)
    let (from, back) = match (special, scheme.as_deref(), slashes >= 2) {
        // (a file: has an authority after exactly two slashes; one slash or none, and three (an empty
        // authority), are a path of the machine; four or more are a share of a host, as Windows reads them)
        (true, Some("file"), _) => match slashes {
            0 | 1 | 3 => return (Dest::Relative, rest.trim_start_matches(slash).to_string()),
            2 => (&rest[2..], true),
            _ => (rest.trim_start_matches(slash), true),
        },
        (true, _, true) => (rest.trim_start_matches(slash), true),
        (true, ..) => return (Dest::Odd, rest.to_string()),
        (false, None, true) => (&rest[2..], true),
        (false, Some(_), _) if rest.starts_with("//") => (&rest[2..], false),
        (false, Some("mailto"), _) => return (Dest::Mail(mailboxes(rest)), rest.to_string()),
        (false, Some(_), _) => return (Dest::Opaque, rest.to_string()),
        _ => return (Dest::Relative, rest.to_string()),
    };
    let end = from
        .find(|c: char| matches!(c, '/' | '?' | '#') || (back && c == '\\'))
        .unwrap_or(from.len());
    let (auth, path) = (&from[..end], from[end..].to_string());
    match authority(auth) {
        Dest::Odd if scheme.as_deref() == Some("file") && matches!(auth, "" | "localhost") => {
            (Dest::Relative, path)
        }
        d => (d, path),
    }
}

/// MD-10: `s` with its `%XX` read.
fn pct(s: &str) -> String {
    let (b, mut out, mut i) = (s.as_bytes(), vec![], 0);
    while i < b.len() {
        let hex = |k: usize| b.get(k).and_then(|&x| (x as char).to_digit(16));
        match (b[i], hex(i + 1), hex(i + 2)) {
            (b'%', Some(h), Some(l)) => (out.push((h * 16 + l) as u8), i += 3).0,
            (x, ..) => (out.push(x), i += 1).0,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// MD-10: the host of each box that a `mailto:` writes, in its address and in the `to`, `cc` and `bcc` of
/// its query: what follows the last `@` of a recipient, which is empty where there is none or it cannot be
/// read, so that it is no host that a text names.
fn mailboxes(rest: &str) -> Vec<String> {
    let body = rest.split('#').next().unwrap_or_default();
    let (addrs, query) = body.split_once('?').unwrap_or((body, ""));
    let headers = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| ["to", "cc", "bcc"].contains(&pct(k).to_ascii_lowercase().as_str()))
        .map(|(_, v)| v);
    // (a header other than the body that has a line break in its value starts one: a recipient that is
    // not read)
    let injected = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .any(|(k, v)| !pct(k).eq_ignore_ascii_case("body") && pct(v).contains(['\r', '\n']))
        .then(String::new);
    let mut boxes: Vec<String> = [addrs]
        .into_iter()
        .chain(headers)
        .flat_map(|list| pct(list).split(',').map(str::to_string).collect::<Vec<_>>())
        .filter(|a| !a.trim().is_empty())
        .map(|a| {
            let domain =
                a.rsplit_once('@').map_or("", |p| p.1).trim().trim_end_matches('>').to_string();
            match authority(&domain) {
                Dest::Host(h) | Dest::Wide(h) => h,
                _ => String::new(),
            }
        })
        .collect();
    boxes.extend(injected);
    boxes
}

/// MD-10: whether a target runs code or carries data (`javascript:`, `data:`, `vbscript:`), through
/// tabs, blanks, controls and case, or has an entity before its first `/` that a browser would decode
/// into one (`javascript&colon;`) and that sim cannot.
fn runs_code(raw: &str) -> bool {
    let t = cleaned(raw).to_ascii_lowercase();
    if ["javascript:", "data:", "vbscript:"].iter().any(|s| t.starts_with(s)) {
        return true;
    }
    let head = t.split('/').next().unwrap_or_default();
    head.match_indices('&').any(|(i, _)| {
        let rest = &head[i + 1..];
        rest.split_once(';').is_some_and(|(e, _)| {
            !e.is_empty() && e.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'#')
        })
    })
}

/// MD-10: the part of `s` before a `?` or a `#`, without its leading `./`, `../`, `/` and `~/` and its
/// trailing `/`, as paths are compared.
fn trimmed(s: &str) -> &str {
    let s = s.split(['?', '#']).next().unwrap_or_default();
    s.trim_start_matches(['.', '/', '~']).trim_end_matches('/')
}

/// MD-10: the words of a link's text, without the brackets, quotes and markers that surround one and
/// the punctuation that ends one.
fn words(text: &str) -> impl Iterator<Item = &str> {
    // split at blanks, at punctuation and symbols that are not ASCII (quotes, dashes, full-width brackets)
    // and at the ASCII that no address has; trim what is not a letter, a digit, a slash, a tilde or a dot
    // at the start and what is not a letter, a digit or a slash at the end
    let apart = |c: char| {
        c.is_whitespace()
            || (!c.is_ascii() && !c.is_alphanumeric())
            || "|\"'<>()[]{}^`*;,".contains(c)
    };
    // (an ampersand ends a word that is no URL or path, `good.com&x`; in a URL it is the query's)
    text.split(apart)
        .flat_map(|w| {
            let query = w.contains('/');
            w.split(move |c: char| c == '&' && !query)
        })
        .map(|w| {
            let w = w.trim_start_matches(|c: char| {
                !(c.is_alphanumeric() || matches!(c, '/' | '~' | '.'))
            });
            w.trim_end_matches(|c: char| !(c.is_alphanumeric() || c == '/'))
        })
        .filter(|w| !w.is_empty())
}

/// MD-10: what a word of a link's text names, if it looks like an address: a URL, a `www.` name, a host
/// of a common top-level domain or of a country (any two letters but the ending of a source file), or a
/// path (`/…`, `~/…`, `./…`, `../…`, or a name with a `/` and an extension).
fn address(w: &str) -> Option<Names> {
    // (a word that follows a label: `site:good.com`, `url=good.com`, `mailto:a@good.com`)
    if let Some((label, rest)) = w.split_once(['=', ':']) {
        let key = !label.is_empty()
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
        if key
            && !rest.starts_with('/')
            && !rest.bytes().all(|b| b.is_ascii_digit())
            && let Some(n) = address(rest.trim_start_matches([':', '=']))
        {
            return Some(n);
        }
    }
    if w.split_once("://").is_some_and(|(s, _)| scheme_ok(s)) {
        return Some(match url(w).0 {
            Dest::Host(h) => Names::Host(h, w.to_string()),
            Dest::Wide(h) => Names::Idn(h),
            _ => Names::Odd,
        });
    }
    // a name with an at-sign (an address for a post box) names the host that follows its last `@`; a port is
    // no part of a host
    let boxed = (!w.contains("://"))
        .then(|| w.strip_prefix("mailto:").unwrap_or(w))
        .filter(|m| m.contains('@'));
    let body = boxed.and_then(|m| m.rsplit_once('@')).map_or(w, |p| p.1);
    let host = &body[..body.find(['/', '\\', '?', '#']).unwrap_or(body.len())];
    let host = host
        .rsplit_once(':')
        .filter(|p| !p.1.is_empty() && p.1.bytes().all(|b| b.is_ascii_digit()))
        .map_or(host, |p| p.0);
    // (a host with letters that are not ASCII, whose last label is a top-level domain: a name that a reader
    // reads, and a browser maps)
    if !host.is_ascii() {
        let wide = map_wide(host).to_lowercase();
        let ls: Vec<&str> = wide.split('.').collect();
        let label =
            |l: &&str| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || "-_".contains(c));
        let tld = ls.last().is_some_and(|l| {
            let k = skeleton(l);
            TLDS.contains(l) || cc_tld(l) || TLDS.contains(&k.as_str()) || cc_tld(&k)
        });
        if !wide.is_ascii() && ls.len() > 1 && tld && ls.iter().all(label) {
            return Some(Names::Idn(
                wide.strip_prefix("www.").map_or(wide.clone(), str::to_string),
            ));
        }
    }
    let labels: Vec<&str> = host.split('.').collect();
    let label =
        |l: &&str| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let last = labels.last().map(|l| l.to_ascii_lowercase()).unwrap_or_default();
    let www = host.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("www."));
    let country = cc_tld(&last);
    let ip = labels.len() == 4 && labels.iter().all(|l| l.parse::<u8>().is_ok());
    let known = labels.len() > 1
        && (www || country || ip || TLDS.contains(&last.as_str()) || on_shared(host));
    if let (true, Dest::Host(h)) = (
        (known && labels.iter().all(label)) || host.eq_ignore_ascii_case("localhost"),
        authority(host),
    ) {
        return Some(Names::Host(h, boxed.unwrap_or(w).to_string()));
    }
    // (a path is rooted, or has a `/` and a file with a name and a lower case extension that has a letter:
    // not `HTTP/1.1`, not `owner/.github`, not `math/rand.Seed`)
    let (stem, ext) = w.rsplit('/').next().unwrap_or_default().rsplit_once('.').unwrap_or_default();
    let abs = w.starts_with('/') || w.starts_with("~/");
    let rooted = abs || w.starts_with("./") || w.starts_with("../");
    let named = w.contains('/')
        && !stem.is_empty()
        && (1..=8).contains(&ext.len())
        && ext.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && ext.bytes().any(|b| b.is_ascii_lowercase());
    ((rooted || named) && !trimmed(w).is_empty()).then(|| Names::Path(trimmed(w).to_string(), abs))
}

/// MD-10: whether a link of `text` to `target` says one place and goes to another: some word of the
/// text looks like an address that the target is not (another host, a host that cannot be read, no host
/// where the text has one unless the target is a file that ends with it; an absolute path and a target
/// with a host; a path that the target's does not end with, or that neither ends with where the target
/// has no host), or the target runs code or carries data, or the text cannot be read (an entity sim has
/// no table for, or a char that shows nothing, next to what could be an address). The text is read as a
/// reader sees it (`views`) and the target as a renderer makes it: written, and with the escapes and
/// entities of CommonMark decoded; every form of each is judged.
pub fn deceptive(text: &str, target: &str) -> bool {
    lies(text, target, false)
}

/// MD-10: `deceptive`, for the target of a Markdown link (`md`: a renderer writes its `\` into the href as
/// `%5C`, so `https://good.com\@evil.com/` is userinfo and evil.com to a browser) or of raw HTML.
fn lies(text: &str, target: &str, md: bool) -> bool {
    let (texts, odd) = views(text);
    let mut targets = vec![target.to_string()];
    if target.contains(['\\', '&']) {
        let g = decode(target).0;
        targets.extend([g.replace('\\', if md { "%5C" } else { "\\" }), g]);
        targets.sort();
        targets.dedup();
    }
    odd || texts.iter().any(|a| targets.iter().any(|b| deceptive_as_written(a, b)))
}

/// MD-10: the forms of a link text that are judged: as it is seen (`shown`), as CommonMark reads its
/// escapes and entities, and without the markers of emphasis, strikethrough and code (`*`, `~`, the
/// backtick, and an `_` that emphasis can take); and whether it cannot be read.
fn views(text: &str) -> (Vec<String>, bool) {
    // (a text with no markup, escape, entity, control or char that is not ASCII reads as it is written)
    let plain = |b: &u8| {
        matches!(b, b'\t' | b'\n')
            || (b.is_ascii_graphic() || *b == b' ') && !b"<&\\*_`~[]".contains(b)
    };
    if text.bytes().all(|b| plain(&b)) {
        return (vec![text.to_string()], false);
    }
    let shown = map_wide(&shown(text));
    let (t, odd) = decode(&shown);
    // (and as a browser reads the tags that CommonMark does not: in a block of raw HTML, `<b title="]".>`
    // is a tag, and `<good.com>` is none)
    let (lenient, _) = decode(&unmarked(&shown));
    // (the fullwidth forms and the full stops of other scripts are what a browser maps them to in a host,
    // and what a reader reads; an entity can make one)
    let t = map_wide(&t);
    // (a tilde makes strikethrough unless it is a home: `~/`)
    let tc: Vec<char> = t.chars().collect();
    let stripped: String = (0..tc.len())
        .filter(|&i| !matches!(tc[i], '*' | '`') && (tc[i] != '~' || tc.get(i + 1) == Some(&'/')))
        .map(|i| tc[i])
        .collect();
    // (an underscore makes emphasis unless it has a letter or a digit on both sides: `no_alloc.rs` stays)
    let cs: Vec<char> = stripped.chars().collect();
    let word = |k: Option<&char>| k.is_some_and(|x| x.is_alphanumeric());
    let bare = (0..cs.len())
        .filter(|&i| cs[i] != '_' || (i > 0 && word(cs.get(i - 1)) && word(cs.get(i + 1))))
        .map(|i| cs[i])
        .collect();
    let mut v = vec![shown, t, bare, stripped, lenient];
    v.sort();
    v.dedup();
    (v, odd)
}

/// MD-10: the named entities that are ASCII punctuation; a name of another kind is left as it is (sim
/// has no table of them).
#[rustfmt::skip]
const NAMED: [(&str, char); 17] = [
    ("amp", '&'), ("lt", '<'), ("gt", '>'), ("quot", '"'), ("apos", '\''), ("period", '.'),
    ("sol", '/'), ("bsol", '\\'), ("colon", ':'), ("num", '#'), ("commat", '@'), ("percnt", '%'),
    ("quest", '?'), ("lowbar", '_'), ("hyphen", '-'), ("Tab", '\t'), ("NewLine", '\n'),
];

/// MD-10: the char an entity stands for, if it is a numeric one or one of `NAMED`.
fn entity(name: &str) -> Option<char> {
    let Some(n) = name.strip_prefix('#') else {
        return NAMED.iter().find(|p| p.0 == name).map(|p| p.1);
    };
    let v = match n.strip_prefix(['x', 'X']) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => n.parse().ok(),
    };
    v.and_then(char::from_u32).filter(|&c| c != '\0')
}

/// MD-10: whether `c` shows nothing: a control, a soft hyphen, a zero-width or joining char, a mark of
/// direction, a variation selector, a tag char.
fn invisible(c: char) -> bool {
    (c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
        || matches!(c as u32, 0xAD | 0x34F | 0x61C | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180F
            | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0
            | 0xE0000..=0xE0FFF)
}

/// MD-10: `s` as CommonMark reads it: a backslash before an ASCII punctuation char is dropped, and an
/// entity that `entity` knows is the char; and whether `s` cannot be read so: an entity that sim has no
/// table for, or a char that shows nothing, next to a letter, a digit, a dot or a hyphen (where it could
/// be breaking an address that a reader sees whole).
fn decode(s: &str) -> (String, bool) {
    let cs: Vec<char> = s.chars().collect();
    let near =
        |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    let (mut out, mut i, mut odd) = (String::new(), 0, false);
    while i < cs.len() {
        let rest = cs.get(i + 1..).unwrap_or_default();
        let end = (cs[i] == '&').then(|| rest.iter().take(12).position(|&c| c == ';')).flatten();
        let name: String = end.map(|e| rest[..e].iter().collect()).unwrap_or_default();
        let named = end.filter(|_| {
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'#')
        });
        match (cs[i], named) {
            ('\\', _) if cs.get(i + 1).is_some_and(char::is_ascii_punctuation) => {
                out.push(cs[i + 1]);
                i += 2;
            }
            (_, Some(e)) => {
                match entity(&name) {
                    Some(c) => out.push(c),
                    None => {
                        odd |= near(out.chars().last()) || near(cs.get(i + e + 2).copied());
                        out.extend(&cs[i..i + e + 2]);
                    }
                }
                i += e + 2;
            }
            (c, _) => {
                // (an invisible char, or a numeric entity that no `;` ends, which an HTML reader decodes)
                let bare = c == '&' && cs.get(i + 1) == Some(&'#') && near(out.chars().last());
                // (a char that shows nothing breaks an address when a piece of one is on both sides, and a
                // mark that turns the text around turns a word that is at its edge)
                let (before, after) = (near(out.chars().last()), near(cs.get(i + 1).copied()));
                let turns = matches!(c as u32, 0x202A..=0x202E | 0x2066..=0x2069);
                odd |=
                    bare || (invisible(c) && if turns { before || after } else { before && after });
                out.push(c);
                i += 1;
            }
        }
    }
    (out, odd)
}

/// MD-10: the tag of raw HTML, or the comment, that starts at `i` (a `<`) in `cs[..lim]`, as CommonMark
/// reads one: where it ends, and for an `<img>` where its `alt` is. None: it is text.
fn tag(cs: &[char], i: usize, lim: usize) -> Option<(usize, Option<Range<usize>>)> {
    let cs = &cs[..lim.min(cs.len())];
    // (a comment, a processing instruction, CDATA, a declaration: a browser shows none of them)
    let upto = |from: usize, end: &[char]| {
        let n = end.len();
        (from..cs.len().saturating_sub(n - 1))
            .find(|&j| cs[j..j + n] == *end)
            .map(|j| (j + n, None))
    };
    let at = &cs[i..];
    if at.starts_with(&['<', '!', '-', '-']) {
        return upto(i + 2, &['-', '-', '>']);
    } else if at.starts_with(&['<', '?']) {
        return upto(i + 2, &['?', '>']);
    } else if at.starts_with(&['<', '!', '[', 'C', 'D', 'A', 'T', 'A', '[']) {
        return upto(i + 9, &[']', ']', '>']);
    } else if at.starts_with(&['<', '!']) && cs.get(i + 2).is_some_and(char::is_ascii_alphabetic) {
        return upto(i + 2, &['>']);
    }
    let blanks = |j: usize| cs[j.min(cs.len())..].iter().take_while(|c| c.is_whitespace()).count();
    let run = |j: usize, more: &str| {
        let word = |c: &&char| c.is_ascii_alphanumeric() || more.contains(**c);
        cs[j.min(cs.len())..].iter().take_while(word).count()
    };
    let closing = cs.get(i + 1) == Some(&'/');
    let name = i + 1 + usize::from(closing);
    if !cs.get(name).is_some_and(char::is_ascii_alphabetic) {
        return None;
    }
    let mut j = name + run(name, "-");
    let img = !closing && cs[name..j].iter().collect::<String>().eq_ignore_ascii_case("img");
    if closing {
        j += blanks(j);
        return (cs.get(j) == Some(&'>')).then_some((j + 1, None));
    }
    let mut alt = None;
    loop {
        let gap = blanks(j);
        j += gap;
        match cs.get(j) {
            Some('>') => return Some((j + 1, alt)),
            Some('/') if cs.get(j + 1) == Some(&'>') => return Some((j + 2, alt)),
            Some(c) if gap > 0 && (c.is_ascii_alphabetic() || matches!(c, '_' | ':')) => {}
            _ => return None,
        }
        let key = j;
        j += run(j, "_.:-");
        let is_alt = img && cs[key..j].iter().collect::<String>().eq_ignore_ascii_case("alt");
        let eq = j + blanks(j);
        if cs.get(eq) != Some(&'=') {
            continue;
        }
        let v = eq + 1 + blanks(eq + 1);
        let (vs, ve, after) = match cs.get(v) {
            Some(&q) if q == '"' || q == '\'' => {
                let e = (v + 1..cs.len()).find(|&x| cs[x] == q)?;
                (v + 1, e, e + 1)
            }
            _ => {
                let plain = |c: &&char| !c.is_whitespace() && !"\"'=<>`".contains(**c);
                let e = v + cs[v.min(cs.len())..].iter().take_while(plain).count();
                (v, e, e)
            }
        };
        if ve == vs && after == ve {
            return None;
        }
        alt = if is_alt { Some(vs..ve) } else { alt };
        j = after;
    }
}

/// MD-10: `s` without anything that starts like a tag (`<b`, `</`, `<!`, `<?`) up to the next `>`.
fn unmarked(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let ends: Vec<usize> = (0..cs.len()).filter(|&j| cs[j] == '>').collect();
    let (mut out, mut i) = (String::new(), 0);
    while i < cs.len() {
        let open = cs[i] == '<'
            && cs.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || "/!?".contains(*c));
        match ends.get(ends.partition_point(|&e| e <= i)).filter(|_| open) {
            Some(&e) => i = e + 1,
            None => {
                out.push(cs[i]);
                i += 1;
            }
        }
    }
    out
}

/// MD-10: `s` as a reader sees it before the escapes and entities are read: an image is its alt text, a tag
/// of raw HTML is nothing (an `<img>`'s alt is its text), and so is a comment; what is none of them as
/// CommonMark reads it is text.
fn shown(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let by_open = |open, close| {
        let mut v = pairs(&cs, open, close).0;
        v.sort_unstable();
        v
    };
    let (square, round) = (by_open('[', ']'), by_open('(', ')'));
    let to =
        |v: &[(usize, usize)], o: usize| v.binary_search_by_key(&o, |p| p.0).ok().map(|k| v[k].1);
    let (mut out, mut i, mut run) = (String::new(), 0, 0usize);
    while i < cs.len() {
        let (escaped, c) = (run % 2 == 1, cs[i]);
        run = if c == '\\' { run + 1 } else { 0 };
        let end = match (escaped, c) {
            // (an image has a source or a label after its alt: `![x]` alone may be text)
            (false, '!') if cs.get(i + 1) == Some(&'[') => to(&square, i + 1).and_then(|cl| {
                let list = match cs.get(cl + 1) {
                    Some('(') => &round,
                    Some('[') => &square,
                    _ => return None,
                };
                to(list, cl + 1).map(|e| {
                    out.extend(&cs[i + 2..cl]);
                    e + 1
                })
            }),
            (false, '<') => tag(&cs, i, cs.len()).map(|(end, alt)| {
                out.extend(alt.map_or(&[][..], |a| &cs[a]));
                end
            }),
            _ => None,
        };
        match end {
            Some(e) => (i, run) = (e, 0),
            None => (out.push(c), i += 1).0,
        }
    }
    out
}

fn deceptive_as_written(text: &str, target: &str) -> bool {
    if runs_code(target) {
        return true;
    }
    let (dest, path) = url(target);
    let q = trimmed(&path);
    if cleaned(target.trim().trim_matches(['<', '>'])).is_empty() {
        return false;
    }
    let ws: Vec<&str> = words(text).take(129).collect();
    ws.len() > 128
        || ws.into_iter().filter_map(address).any(|n| match n {
            Names::Odd => true,
            // (a subdomain of the host in the text is that host's owner's)
            Names::Host(h, raw) => match &dest {
                Dest::Host(t) => {
                    *t != h
                        && !(t.len() > h.len() + 1 && t.ends_with(&format!(".{h}")) && !shared(&h))
                }
                Dest::Odd | Dest::Wide(_) | Dest::Opaque => true,
                Dest::Mail(boxes) => boxes.is_empty() || boxes.iter().any(|b| *b != h),
                Dest::Relative => !q.ends_with(raw.as_str()),
            },
            // (a host with letters that are not ASCII is honest as written, or as the punycode of each label)
            Names::Idn(u) => match &dest {
                Dest::Host(t) => !encodes(&u, t),
                Dest::Wide(w) => *w != u,
                Dest::Mail(boxes) => {
                    boxes.is_empty() || boxes.iter().any(|b| *b != u && !encodes(&u, b))
                }
                Dest::Relative => !q.ends_with(u.as_str()),
                _ => true,
            },
            Names::Path(p, abs) => match dest {
                Dest::Relative => q.is_empty() || !(p.ends_with(q) || q.ends_with(&p)),
                Dest::Opaque | Dest::Mail(_) => true,
                _ => q.is_empty() || abs || !q.ends_with(&p),
            },
        })
}

/// MD-10: a mark of the walk: the text of a link (with its brackets, or the tag) and its target; where a
/// link has no target on the line (a use of a reference) the target is empty.
type Mark = (Range<usize>, Range<usize>);

/// MD-10: the `open` and `close` chars of `cs` that close each other, inner pairs first (so by the closer):
/// one that a backslash escapes is none. And how many close nothing: a link whose text goes over lines.
fn pairs(cs: &[char], open: char, close: char) -> (Vec<(usize, usize)>, usize) {
    let (mut stack, mut out, mut run, mut seen) = (vec![], vec![], 0usize, 0usize);
    for (j, &c) in cs.iter().enumerate() {
        let escaped = run % 2 == 1;
        run = if c == '\\' { run + 1 } else { 0 };
        if escaped || (c != open && c != close) {
            continue;
        }
        seen += 1;
        if c == open {
            stack.push(j);
        } else {
            out.extend(stack.pop().map(|o| (o, j)));
        }
    }
    let stray = seen - 2 * out.len();
    (out, stray)
}

/// MD-10: the pairs of `[` and `]` of `cs`.
fn brackets(cs: &[char]) -> (Vec<(usize, usize)>, usize) {
    pairs(cs, '[', ']')
}

/// MD-10: whether the `!` before the bracket at `o` makes an image (a backslash before it makes it none).
fn bang(c: &[char], o: usize) -> bool {
    o > 0 && c[o - 1] == '!' && c[..o - 1].iter().rev().take_while(|&&x| x == '\\').count() % 2 == 0
}

/// MD-10: the target that follows the blanks after `from`: where it starts and ends, and whether it ends
/// where a target ends (not so: it is longer than a target is judged, or it runs past what is read). None:
/// nothing but blanks follows (the target is on another line), or an angle bracket that nothing closes.
fn dest(c: &[char], from: usize) -> Option<(usize, usize, bool)> {
    let start = (from..c.len()).find(|&j| !c[j].is_whitespace())?;
    let lim = c.len().min(start + DEST + 1);
    if c[start] == '<' {
        return match c[start..lim].iter().position(|&x| x == '>') {
            Some(e) => Some((start, start + e + 1, true)),
            None => (lim < c.len()).then_some((start, lim, false)),
        };
    }
    let (mut end, mut parens) = (start, 0);
    while end < lim {
        let x = c[end];
        if x.is_whitespace() || (x == ')' && parens == 0) {
            return Some((start, end, true));
        }
        parens += i32::from(x == '(') - i32::from(x == ')');
        end += 1;
    }
    Some((start, end, end == c.len() && end - start <= DEST))
}

/// MD-10: whether the text of a link names an address in some form of it (`views`), or cannot be read.
fn named(text: &str) -> bool {
    let (texts, odd) = views(text);
    odd || texts.iter().any(|t| words(t).any(|w| address(w).is_some()))
}

/// MD-10: whether the text names a host, or cannot be read (a path is not enough where nothing resolves the
/// use: `[ -d /tmp ]` is code far more often than a link).
fn names_host(text: &str) -> bool {
    let (texts, odd) = views(text);
    odd || texts.iter().any(|t| words(t).filter_map(address).any(|n| !matches!(n, Names::Path(..))))
}

/// MD-10: whether a label has a letter with a case that is not ASCII: CommonMark folds the case of Unicode
/// (`ß` is `SS`), sim lowercases.
fn folds(label: &str) -> bool {
    label.chars().any(|c| !c.is_ascii() && (c.to_lowercase().ne([c]) || c.to_uppercase().ne([c])))
}

/// MD-10: the links of `c` that lie or that cannot be judged, with the pairs of brackets `ps` of it: the
/// inline ones (in a Markdown file or where the highlighter found a link), and, in a Markdown file (`refs`),
/// the uses of reference links, resolved by the document's definitions, and the links of raw HTML. `window`:
/// `c` is the paragraph around a line, read as one line, where a `](` that no `[` opens is a link whose text
/// began out of reach (its target is marked, not trusted). Code is judged like the rest: a stray backtick,
/// or a fence that a renderer does not open, would otherwise take a live link for code.
fn scan(
    c: &[char],
    masked: &[char],
    ps: &[(usize, usize)],
    refs: Option<&dyn Fn() -> Arc<Refs>>,
    b: &mut Budget,
    window: bool,
) -> Vec<Mark> {
    let (mut out, mut dests, mut covered) = (vec![], vec![], 0);
    for &(o, cl) in ps {
        if c.get(cl + 1) != Some(&'(') || cl + 1 < covered {
            continue;
        }
        let Some((start, end, closed)) = dest(c, cl + 2) else { continue };
        // (a target that no `)` or title follows makes no link, and covers nothing: a link whose `](` is in
        // it is whole; reading it again is paid for)
        let next = c[end..].iter().find(|x| !x.is_whitespace());
        if !closed || next.is_some_and(|x| ")\"'(".contains(*x)) {
            dests.push((start, end));
            covered = end;
        } else if !b.spend(end - start + 1) {
            out.push((0..c.len(), c.len()..c.len()));
            break;
        }
        if start < end && !bang(c, o) && (!closed || b.judge(&c[o + 1..cl], &c[start..end], true)) {
            out.push((o..cl + 1, start..end));
        }
    }
    let inside = |x: usize, v: &[(usize, usize)]| {
        v.get(v.partition_point(|d| d.1 <= x)).is_some_and(|d| d.0 <= x)
    };
    let mut reach = 0;
    // (a `](` in what this reading makes inert is not one)
    for j in (0..c.len().saturating_sub(1))
        .filter(|&j| window && masked[j] == ']' && masked[j + 1] == '(')
    {
        let escaped = c[..j].iter().rev().take_while(|&&x| x == '\\').count() % 2 == 1;
        if j + 1 < reach
            || escaped
            || inside(j + 1, &dests)
            || ps.binary_search_by_key(&j, |p| p.1).is_ok()
        {
            continue;
        }
        if let Some((start, end, _)) = dest(c, j + 2).filter(|d| d.0 < d.1) {
            out.push((j..j + 1, start..end));
            reach = end;
        }
    }
    if let Some(refs) = refs {
        for u in uses_in(c, ps) {
            let at = u.at.clone();
            let Some(label) = u.label.filter(|_| b.spend(at.len() + 16)) else {
                out.push((at.clone(), at.end..at.end));
                continue;
            };
            let text: String = c[at.start + 1..at.end - 1].iter().collect();
            if !named(&text) {
                continue;
            }
            let r = refs();
            let lies =
                r.dests(&label).map_or((r.loose || folds(&label)) && names_host(&text), |ds| {
                    ds.iter().any(|d| !b.allow(text.len(), d.len()) || lies(&text, d, true))
                });
            if lies {
                out.push((at.clone(), at.end..at.end));
            }
        }
    }
    out
}

/// MD-10: `c` with what CommonMark reads before it reads a link made inert (its chars are NULs): the tags
/// of raw HTML, the comments and the autolinks, and, if `code`, the code spans of the line (a run of
/// backticks and the next of its length): a bracket in them pairs with none.
fn inert(c: &[char], code: bool) -> Vec<char> {
    let (mut v, mut i, mut run, mut runs) = (c.to_vec(), 0, 0usize, vec![]);
    while i < c.len() {
        let (escaped, ch) = (run % 2 == 1, c[i]);
        run = if ch == '\\' { run + 1 } else { 0 };
        let tag = (ch == '<' && !escaped).then(|| tag(c, i, c.len().min(i + DEST)).map(|t| t.0));
        if let Some(e) = tag.flatten().or_else(|| autolink(c, i).filter(|_| ch == '<' && !escaped))
        {
            v[i..e].fill('\0');
            (i, run) = (e, 0);
        } else if ch == '`' && code {
            let n = c[i..].iter().take_while(|&&x| x == '`').count();
            runs.extend(
                (n > usize::from(escaped))
                    .then_some((i + usize::from(escaped), n - usize::from(escaped))),
            );
            (i, run) = (i + n, 0);
        } else {
            i += 1;
        }
    }
    let (mut next, mut pair) = (HashMap::new(), vec![None; runs.len()]);
    for k in (0..runs.len()).rev() {
        pair[k] = next.insert(runs[k].1, k);
    }
    let mut k = 0;
    while k < runs.len() {
        k = match pair[k] {
            Some(m) => {
                v[runs[k].0..runs[m].0 + runs[m].1].fill('\0');
                m + 1
            }
            None => k + 1,
        };
    }
    v
}

/// MD-10: where the autolink `<scheme:…>` that starts at `i` ends.
fn autolink(c: &[char], i: usize) -> Option<usize> {
    let s = c.get(i + 1..)?;
    let scheme = |x: &&char| x.is_ascii_alphanumeric() || matches!(x, '+' | '.' | '-');
    let n = s.iter().take(34).take_while(scheme).count();
    let rest = s.get(n + 1..)?;
    let m = rest.iter().take_while(|x| !x.is_whitespace() && !matches!(x, '<' | '>')).count();
    let ok = (2..=32).contains(&n) && s[0].is_ascii_alphabetic() && s.get(n) == Some(&':');
    (ok && rest.get(m) == Some(&'>')).then_some(i + n + m + 3)
}

/// MD-10: the pairs of brackets of `c` in each reading of it (as written, with the tags and autolinks
/// inert, with the code spans of the line inert too), each once, and the most brackets that close nothing.
fn readings(c: &[char]) -> (Vec<Reading>, usize) {
    let (first, mut stray) = brackets(c);
    let mut v: Vec<Reading> = vec![(first, None)];
    // (a mask only takes brackets away: a line with none has one reading)
    if (c.contains(&'<') || c.contains(&'`')) && (stray > 0 || !v[0].0.is_empty()) {
        for code in [false, true] {
            let masked = inert(c, code);
            let (p, st) = brackets(&masked);
            stray = stray.max(st);
            if !v.iter().any(|r| r.0 == p) {
                v.push((p, Some(masked)));
            }
        }
    }
    (v, stray)
}

/// MD-10: a reading of a line: its pairs of brackets, and the line as that reading has it where it is not
/// as written (with what it makes inert blanked out).
type Reading = (Vec<(usize, usize)>, Option<Vec<char>>);

/// MD-10: the links of `c` that lie or that cannot be judged, in each reading of its brackets, and, if it
/// is a Markdown file (`refs`), the links of its raw HTML.
fn scan_all(
    c: &[char],
    rs: &[Reading],
    refs: Option<&dyn Fn() -> Arc<Refs>>,
    b: &mut Budget,
    window: bool,
) -> Vec<Mark> {
    let mut out = if refs.is_some() { html_marks(c, b, window) } else { vec![] };
    for (ps, masked) in rs {
        out.extend(scan(c, masked.as_deref().unwrap_or(c), ps, refs, b, window));
    }
    out
}

/// MD-10: the value of the attribute `want` among the attributes `a` of a tag, where it is in `a`.
fn attr(a: &[char], want: &str) -> Option<Range<usize>> {
    let mut i = 0;
    while i < a.len() {
        i = (i..a.len()).find(|&j| !a[j].is_whitespace() && a[j] != '/')?;
        let name_end =
            (i..a.len()).find(|&j| a[j].is_whitespace() || a[j] == '=').unwrap_or(a.len());
        let key: String = a[i..name_end].iter().collect();
        let eq = (name_end..a.len()).find(|&j| !a[j].is_whitespace()).filter(|&j| a[j] == '=');
        let Some(eq) = eq else {
            i = name_end.max(i + 1);
            continue;
        };
        let v = (eq + 1..a.len()).find(|&x| !a[x].is_whitespace()).unwrap_or(a.len());
        let (vs, ve) = match a.get(v) {
            Some(&q) if q == '"' || q == '\'' => {
                (v + 1, (v + 1..a.len()).find(|&x| a[x] == q).unwrap_or(a.len()))
            }
            _ => (v, (v..a.len()).find(|&x| a[x].is_whitespace()).unwrap_or(a.len())),
        };
        if key.eq_ignore_ascii_case(want) {
            return Some(vs..ve);
        }
        i = ve.max(i + 1);
    }
    None
}

/// MD-10: whether the char at `j` is a blank or a `/`, or there is none (after the name of a tag a browser
/// reads a `/` as a blank).
fn blank_or_slash(c: &[char], j: usize) -> bool {
    c.get(j).is_none_or(|x| x.is_whitespace() || *x == '/')
}

/// MD-10: the links of raw HTML in `c`: an `<a …>` tag with an `href`, whose text is what follows up to the
/// next `</a`. A tag that does not end where it is read, a link with no `</a` and an `href` that no tag
/// carries (its tag began out of reach) are not trusted: the value is marked.
fn html_marks(c: &[char], b: &mut Budget, window: bool) -> Vec<Mark> {
    // (a line with no `<` has no tag; an `href=` whose tag is out of reach is the paragraph's to find)
    if !window && !c.contains(&'<') {
        return vec![];
    }
    let is = |j: usize, s: &str| {
        s.chars().enumerate().all(|(k, x)| c.get(j + k).is_some_and(|y| y.eq_ignore_ascii_case(&x)))
    };
    let closers: Vec<usize> = (0..c.len()).filter(|&j| c[j] == '<' && is(j, "</a")).collect();
    let (mut out, mut at, mut k, mut tags) = (vec![], 0, 0, vec![]);
    while let Some(j) =
        (at..c.len()).find(|&j| c[j] == '<' && is(j, "<a") && blank_or_slash(c, j + 2))
    {
        let lim = c.len().min(j + DEST);
        // (the end of the tag as CommonMark and a browser read it, with a `>` in a quoted value; else the
        // first `>`, for a tag that only a browser reads)
        let found = tag(c, j, lim).map(|t| t.0 - 1);
        let Some(end) = found.or_else(|| c[j..lim].iter().position(|&x| x == '>').map(|p| j + p))
        else {
            out.extend(window.then_some((j..lim, lim..lim)));
            tags.push(j..lim);
            at = lim;
            continue;
        };
        tags.push(j..end + 1);
        at = end + 1;
        let Some(href) = attr(&c[j + 2..end], "href").map(|r| r.start + j + 2..r.end + j + 2)
        else {
            continue;
        };
        while closers.get(k).is_some_and(|&x| x < end) {
            k += 1;
        }
        let text = closers.get(k).map(|&x| end + 1..x);
        if (text.is_some() || window)
            && text.is_none_or(|t| b.judge(&c[t], &c[href.clone()], false))
        {
            out.push((href.clone(), href));
        }
    }
    if !window {
        return out;
    }
    let (mut p, nth) = (0, |ch: char| (0..c.len()).filter(|&j| c[j] == ch).collect::<Vec<_>>());
    let (lts, gts) = (nth('<'), nth('>'));
    while let Some(j) =
        (p..c.len()).find(|&j| is(j, "href") && (j == 0 || c[j - 1].is_whitespace()))
    {
        let value = attr(&c[j..c.len().min(j + DEST)], "href").map(|r| r.start + j..r.end + j);
        let outside = tags
            .get(tags.partition_point(|t: &Range<usize>| t.end <= j))
            .is_none_or(|t| t.start > j);
        // (an href in the open tag of another element is that element's, a `<link>`'s)
        let open = lts[..lts.partition_point(|&x| x < j)]
            .last()
            .filter(|&&x| gts[..gts.partition_point(|&g| g < j)].last().is_none_or(|&g| g < x));
        let other = open.is_some_and(|&x| !(is(x, "<a") && blank_or_slash(c, x + 2)));
        p = value.as_ref().map_or(j + 4, |v| v.end.max(j + 4));
        if let (true, false, Some(v)) = (outside, other, value) {
            out.push((v.clone(), v));
        }
    }
    out
}

/// MD-10: a use of a reference: where `[text]` is (the brackets included), and the label it is resolved
/// by, as it is matched; none where the text or the label is longer than a link text is judged.
struct Use {
    at: Range<usize>,
    label: Option<String>,
}

/// MD-10: a label as CommonMark matches it: case folded, runs of blanks one.
fn norm(label: &[char]) -> String {
    label.iter().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// MD-10: the references that the pairs of brackets `ps` of `c` use, in the three forms: `[text][label]`,
/// `[text][]` and `[text]` (the text is the label of the last two). Not an image's, a footnote's, an
/// inline link's, the label part of a full one, nor the label of a definition.
fn uses_in(c: &[char], ps: &[(usize, usize)]) -> Vec<Use> {
    let mut by_open = ps.to_vec();
    by_open.sort_unstable();
    let first = after_containers(c);
    // (the closers of the pairs that make an inline link: a reference cannot have a link in its text)
    let linked: Vec<usize> =
        ps.iter().filter(|p| c.get(p.1 + 1) == Some(&'(') && !bang(c, p.0)).map(|p| p.1).collect();
    let inner_link = |o: usize, cl: usize| {
        linked.get(linked.partition_point(|&x| x <= o)).is_some_and(|&x| x < cl)
    };
    ps.iter()
        .filter_map(|&(o, cl)| {
            let image = bang(c, o);
            let has_link = inner_link(o, cl);
            // (the label of a full reference, unless what comes before it is a text that a reference cannot
            // have, and then it is a use of its own)
            let label_part = o > 0
                && c[o - 1] == ']'
                && ps
                    .binary_search_by_key(&(o - 1), |p| p.1)
                    .ok()
                    .is_some_and(|k| bang(c, ps[k].0) || !inner_link(ps[k].0, ps[k].1));
            let def = c.get(cl + 1) == Some(&':') && o == first;
            if image
                || has_link
                || label_part
                || def
                || c.get(o + 1) == Some(&'^')
                || c.get(cl + 1) == Some(&'(')
            {
                return None;
            }
            let own = &c[o + 1..cl];
            let by = by_open.binary_search_by_key(&(cl + 1), |p| p.0).ok();
            let label = by
                .map(|i| &c[by_open[i].0 + 1..by_open[i].1])
                .filter(|l| !l.iter().all(|x| x.is_whitespace()));
            let label = label.unwrap_or(own);
            Some(Use {
                at: o..cl + 1,
                label: (own.len() <= TEXT && label.len() <= TEXT).then(|| norm(label)),
            })
        })
        .collect()
}

/// MD-10: where the text of a line starts after the markers of the containers it is in (a block quote's
/// `>`, a list item's `-`, `*`, `+`, `1.` or `1)`, nested), and the blanks around them.
fn after_containers(c: &[char]) -> usize {
    let mut i = 0;
    loop {
        let j = (i..c.len()).find(|&k| !matches!(c[k], ' ' | '\t')).unwrap_or(c.len());
        let digits = c[j..].iter().take_while(|x| x.is_ascii_digit()).count();
        let numbered = (1..=9).contains(&digits) && matches!(c.get(j + digits), Some('.' | ')'));
        let step = match c.get(j) {
            Some('>') => 1,
            Some('-' | '*' | '+') if matches!(c.get(j + 1), Some(' ' | '\t')) => 1,
            _ if numbered && matches!(c.get(j + digits + 1), Some(' ' | '\t')) => digits + 1,
            _ => return j,
        };
        i = j + step;
    }
}

/// MD-10: the definition a line is, if it is one: `[label]: destination` after the markers of its
/// containers (a definition in code is taken for one too: sim errs to the side of marking). Its label as it
/// is matched, and where its destination is in the line (none where it is on the next line).
fn def_in(c: &[char]) -> Option<(String, Option<Range<usize>>)> {
    let i = after_containers(c);
    if c.get(i) != Some(&'[') {
        return None;
    }
    let close = (i + 1..c.len().min(i + TEXT + 2)).find(|&j| c[j] == ']' && c[j - 1] != '\\')?;
    let label = &c[i + 1..close];
    let plain = label.first() != Some(&'^')
        && !label.contains(&'[')
        && !label.iter().all(|x| x.is_whitespace());
    if c.get(close + 1) != Some(&':') || !plain {
        return None;
    }
    let k = (close + 2..c.len()).find(|&j| !c[j].is_whitespace());
    Some((norm(label), k.and_then(|k| token(c, k))))
}

/// MD-10: whether a line that is no definition could be one: it has a `]:` that no `[` of the line opens
/// (the label goes over lines) or it starts with a `[`, and is no footnote's.
fn maybe_def(c: &[char]) -> bool {
    let i = after_containers(c);
    let Some(at) = c[i..].windows(2).position(|w| w == [']', ':']) else { return false };
    let note = c[i..].starts_with(&['[', '^']);
    // (the colon of a sentence follows a use: `install with [npm][]:` has its `[` before the `]:`)
    !note && (c.get(i) == Some(&'[') || !c[i..i + at].contains(&'['))
}

/// MD-10: the destination that starts at `k`: a `<…>` whole, or up to the next blank (one past what is
/// judged where it is longer).
fn token(c: &[char], k: usize) -> Option<Range<usize>> {
    let lim = c.len().min(k + DEST + 1);
    if c[k] == '<' {
        let e = c[k..lim].iter().position(|&x| x == '>')?;
        return Some(k..k + e + 1);
    }
    Some(k..c[k..lim].iter().position(|x| x.is_whitespace()).map_or(lim, |e| k + e))
}

/// MD-10: the reference links of a document, resolved by their definitions: the first `REF_LINES` lines and
/// `REF_CHARS` chars are looked through, once for a version of the text, and what is past that makes
/// the document loose (a use that no definition resolves is not trusted: the definition may be past it).
pub struct Refs {
    /// the document may have a definition that was not read (it was not looked through whole, a line that
    /// could be one was not read as one, or a label has a letter that sim does not fold): a use that no
    /// definition resolves is not trusted
    loose: bool,
    defs: HashMap<String, Vec<String>>,
    on: Lines,
}

/// MD-10: a line (or paragraph) of the document that has a bracket, and its pairs of brackets.
type Kept = (Vec<(usize, usize)>, Vec<char>);

/// MD-10: ranges of chars by the line they are on.
type Lines = HashMap<usize, Vec<Range<usize>>>;

impl Refs {
    /// The document of `n` lines, `line(l)` the text of line `l`. Every definition of a label counts (the
    /// first wins in CommonMark, and sim does not know which a reader's renderer takes).
    pub fn new(n: usize, line: &dyn Fn(usize) -> Option<String>) -> Refs {
        let (mut left, mut blind) = (REF_CHARS, n > REF_LINES);
        let (mut kept, mut defs, mut pending) = (vec![], vec![], None::<String>);
        let (mut suspect, mut para, mut crossing, mut from) = (false, vec![], false, 0);
        // (the lines of a paragraph that has a bracket that closes nothing, which a use may go over, are
        // read as one; the lines of any other are read by themselves)
        let flush =
            |para: &mut Vec<String>, crossing: &mut bool, kept: &mut Vec<Kept>, from: usize| {
                if *crossing {
                    let chars = |l: &String| l.chars().take(LINE).chain(['\n']).collect::<Vec<_>>();
                    let joined: Vec<char> = para.iter().flat_map(chars).collect();
                    kept.truncate(from);
                    kept.push((brackets(&joined).0, joined));
                }
                para.clear();
                *crossing = false;
            };
        for l in 0..n.min(REF_LINES) {
            let Some(s) = line(l) else { continue };
            let size = s.len().min(4 * LINE) + 1;
            if left < size {
                blind = true;
                break;
            }
            left -= size;
            // (a line past what is read of it with a `]` in it)
            blind |= s.len() > LINE && s.chars().skip(LINE).any(|x| x == ']');
            let (opens, closes) = (s.contains('['), s.contains(']'));
            // only a line with a `[` can be a definition or have a use, and the one after a definition
            // that has no destination is its destination
            if opens || pending.is_some() {
                let c: Vec<char> = s.chars().take(LINE).collect();
                let dest = |c: &[char], r: Range<usize>| {
                    if r.len() > DEST { UNJUDGED.to_string() } else { c[r].iter().collect() }
                };
                if let Some(label) = pending.take() {
                    let k = after_containers(&c);
                    defs.extend(
                        (k < c.len())
                            .then(|| token(&c, k))
                            .flatten()
                            .map(|r| (l, label, r.clone(), dest(&c, r))),
                    );
                }
                match def_in(&c) {
                    Some((label, Some(r))) => {
                        suspect |= folds(&label);
                        defs.push((l, label, r.clone(), dest(&c, r)));
                    }
                    Some((label, None)) => {
                        suspect |= folds(&label);
                        pending = Some(label);
                    }
                    None => suspect |= closes && s.contains("]:") && maybe_def(&c),
                }
                let (ps, stray) = brackets(&c);
                crossing |= stray > 0;
                if opens {
                    kept.push((ps, c));
                }
            } else {
                suspect |= closes && s.contains("]:");
                crossing |= closes;
            }
            if s.trim().is_empty() {
                flush(&mut para, &mut crossing, &mut kept, from);
                from = kept.len();
            } else {
                para.push(s);
            }
        }
        flush(&mut para, &mut crossing, &mut kept, from);
        let mut by: HashMap<String, Vec<String>> = HashMap::new();
        for (_, label, _, dest) in &defs {
            let v = by.entry(label.clone()).or_default();
            if v.len() < 8 {
                v.push(dest.clone());
            } else if v.len() == 8 {
                v.push(UNJUDGED.to_string());
            }
        }
        let mut b = Budget::new(1_000, REF_CHARS);
        let mut lying: HashMap<String, Vec<String>> = HashMap::new();
        for (ps, c) in &kept {
            for u in uses_in(c, ps) {
                let Some(label) = u.label else {
                    blind = true;
                    continue;
                };
                if !b.spend(u.at.len() + 16) {
                    blind = true;
                    continue;
                }
                let text: String = c[u.at.start + 1..u.at.end - 1].iter().collect();
                let Some(ds) = by.get(&label).filter(|_| named(&text)) else { continue };
                let lie = ds.iter().any(|d| {
                    let ok = b.allow(text.len(), d.len());
                    blind |= !ok;
                    !ok || lies(&text, d, true)
                });
                let texts = lying.entry(label).or_default();
                if lie && texts.len() < 16 && !texts.contains(&text) {
                    texts.push(text);
                }
            }
        }
        let mut on = Lines::new();
        for (l, label, r, dest) in defs {
            let lies = lying.get(&label).is_some_and(|ts| {
                ts.iter().any(|t| {
                    let ok = b.allow(t.len(), dest.len());
                    blind |= !ok;
                    !ok || lies(t, &dest, true)
                })
            });
            if lies {
                on.entry(l).or_default().push(r);
            }
        }
        Refs { loose: blind || suspect, defs: by, on }
    }

    /// The destinations that the definitions of `label` give, none if there is none.
    fn dests(&self, label: &str) -> Option<&Vec<String>> {
        self.defs.get(label)
    }

    /// The targets of the definitions on line `l` that a use of the part that was looked through makes lie
    /// (a use past it that a definition makes lie is marked where it is drawn).
    fn defs_on(&self, l: usize) -> Vec<Range<usize>> {
        self.on.get(&l).cloned().unwrap_or_default()
    }
}

/// MD-10: whether `c` has `s` at `j`, in either case.
fn tags_at(c: &[char], j: usize, s: &str) -> bool {
    s.chars().enumerate().all(|(k, x)| c.get(j + k).is_some_and(|y| y.eq_ignore_ascii_case(&x)))
}

/// MD-10: whether `c` ends inside a tag of raw HTML or a comment that is not closed (`inside`: it begins
/// inside one, as the line above ended).
fn tag_open(c: &[char], inside: bool) -> bool {
    let (mut open, mut quote, mut i) = (inside, None, 0);
    while i < c.len() {
        let ch = c[i];
        match (open, quote, ch) {
            (true, Some(q), _) if ch == q => quote = None,
            (true, None, '"' | '\'') => quote = Some(ch),
            (true, None, '>') => open = false,
            (false, _, '<') => {
                open = c.get(i + 1).is_some_and(|x| x.is_ascii_alphabetic() || "/!?".contains(*x));
            }
            _ => {}
        }
        i += 1;
    }
    open
}

/// MD-10: whether the line `l` begins inside a tag of raw HTML that the lines above it (up to three, none
/// blank) leave open.
fn opened_above(text: &dyn Fn(usize) -> Option<String>, l: usize) -> bool {
    let above =
        (1..=3).map_while(|k| l.checked_sub(k).and_then(text).filter(|s| !s.trim().is_empty()));
    let lines: Vec<String> = above.collect();
    lines.iter().rev().fold(false, |open, s| {
        let cs: Vec<char> = s.chars().take(LINE).collect();
        tag_open(&cs, open)
    })
}

/// MD-10: whether the line `c` may have a link that goes over lines: a bracket that closes nothing, a line
/// that ends with the `](` of a link whose target is on the next (`prev` is the line before: its target is
/// on this one), or a tag of raw HTML that is not closed on it.
fn crosses(prev: &str, c: &[char], stray: usize) -> (bool, bool) {
    let ends = |c: &[char]| {
        c.iter()
            .rposition(|x| !x.is_whitespace())
            .is_some_and(|i| i >= 1 && c[i] == '(' && c[i - 1] == ']')
    };
    let tags = |s: &str| {
        (0..c.len())
            .filter(|&j| {
                c[j] == '<'
                    && s.chars()
                        .enumerate()
                        .all(|(k, x)| c.get(j + k).is_some_and(|y| y.eq_ignore_ascii_case(&x)))
            })
            .count()
    };
    // (a line with no `<` has no tag, and an attribute has an `=`)
    let tagged = c.contains(&'<');
    let opens = (0..c.len())
        .filter(|&j| {
            tagged
                && c[j] == '<'
                && c.get(j + 1).is_some_and(|x| x.eq_ignore_ascii_case(&'a'))
                && blank_or_slash(c, j + 2)
        })
        .count();
    let href = c.contains(&'=')
        && (0..c.len()).any(|j| {
            tags_at(c, j, "href")
                && (j == 0 || c[j - 1].is_whitespace())
                && c[j + 4..].iter().find(|x| !x.is_whitespace()) == Some(&'=')
        });
    let closes = if tagged { tags("</a") } else { 0 };
    let any = stray > 0 || ends(c) || prev.trim_end().ends_with("](") || opens != closes || href;
    (any, opens > closes)
}

/// MD-10: the lines of the paragraph around line `l`, as chars (`cur`, which is read, for `l`): up to three
/// lines before it and two after (`after`: more where a tag `<a` is not closed on the line: a block that is
/// an image in a link has its `</a>` further down), none blank (a link does not go over a blank line), each
/// as much as is read of a line; and where `l` is among them. None where the budget `b` cannot read them: what is read
/// is paid for by the frame, so that a screen of lines that each send the walk to its paragraph is bounded.
fn paragraph(
    text: &dyn Fn(usize) -> Option<String>,
    l: usize,
    cur: &[char],
    b: &mut Budget,
    after: usize,
) -> Option<(Vec<Vec<char>>, usize)> {
    let line = |k: Option<usize>| k.and_then(text).filter(|s| !s.trim().is_empty());
    let mut before: Vec<Vec<char>> = vec![];
    for s in (1..=3).map_while(|k| line(l.checked_sub(k))) {
        let n = s.chars().count();
        b.spend(n.min(LINE)).then_some(())?;
        before.push(s.chars().skip(n.saturating_sub(LINE)).collect());
    }
    before.reverse();
    let at = before.len();
    b.spend(cur.len()).then_some(())?;
    before.push(cur.to_vec());
    for s in (1..=after).map_while(|k| line(Some(l + k))) {
        let take: Vec<char> = s.chars().take(LINE).collect();
        b.spend(take.len()).then_some(())?;
        before.push(take);
    }
    Some((before, at))
}

/// MD-10: the marks as the chars of the line that is `from..to` of what they were found in: the target
/// where it is on the line, else the part of the text that is.
fn on_line(marks: &[Mark], from: usize, to: usize) -> Vec<Range<usize>> {
    let on = |r: &Range<usize>| r.start < r.end && r.start >= from && r.end <= to;
    marks
        .iter()
        .filter_map(|(text, target)| {
            let r =
                if on(target) { target.clone() } else { text.start.max(from)..text.end.min(to) };
            (r.start < r.end).then(|| r.start - from..r.end - from)
        })
        .collect()
}

/// MD-10: the ranges sorted, the ones that overlap one.
fn merge(mut v: Vec<Range<usize>>) -> Vec<Range<usize>> {
    v.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<Range<usize>> = vec![];
    for r in v {
        match out.last_mut() {
            Some(p) if r.start < p.end => p.end = p.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// MD-10: the chars of line `l` (`cs`) that the walk marks as the target of a link that lies, the text
/// of a use of a reference that lies, or whatever of a link cannot be judged: in a Markdown file (`refs` is
/// given: it resolves the references of the document, and is asked only where a line has a use whose text
/// names an address or a definition), and in any file where the highlighter found a link (`sp`). Code is
/// judged like the rest. Only the first `LINE` chars of a line are read: a `]` past them marks all that
/// is past them. A link that goes over lines is found in the paragraph around the line (`text(k)` is line
/// `k`). `b` is what the frame may judge, shared by its lines.
pub fn line_marks(
    l: usize,
    cs: &[char],
    sp: &[(Range<usize>, Look)],
    text: &dyn Fn(usize) -> Option<String>,
    refs: Option<&dyn Fn() -> Arc<Refs>>,
    b: &mut Budget,
) -> Vec<Range<usize>> {
    if refs.is_none() && !sp.iter().any(|s| matches!(s.1, Look::Link | Look::Url)) {
        return vec![];
    }
    let n = cs.len().min(LINE);
    let (rs, stray) = readings(&cs[..n]);
    let mut out = on_line(&scan_all(&cs[..n], &rs, refs, b, false), 0, n);
    if cs.len() > LINE && cs[LINE..].contains(&']') {
        out.push(LINE..cs.len());
    }
    let prev = l.checked_sub(1).and_then(text).unwrap_or_default();
    // (a line inside a tag that goes over lines has its target as likely as the line of its `<a`)
    let inherited =
        refs.is_some() && if l > 0 && b.tag.0 == l - 1 { b.tag.1 } else { opened_above(text, l) };
    let ends =
        refs.is_some() && (inherited || cs[..n].contains(&'<')) && tag_open(&cs[..n], inherited);
    b.tag = (l, ends && cs.iter().any(|x| !x.is_whitespace()));
    let (mut cross, mut open_tag) = crosses(&prev, &cs[..n], stray);
    (cross, open_tag) = (cross || inherited || ends, open_tag || inherited || ends);
    if cross {
        if let Some((lines, at)) =
            paragraph(text, l, &cs[..n], b, if open_tag { AFTER_TAG } else { 2 })
        {
            let joined: Vec<char> =
                lines.iter().flat_map(|x| x.iter().copied().chain(['\n'])).collect();
            let from: usize = lines[..at].iter().map(|x| x.len() + 1).sum();
            out.extend(on_line(
                &scan_all(&joined, &readings(&joined).0, refs, b, true),
                from,
                from + n,
            ));
        } else {
            out.push(0..n);
        }
    }
    let dest_line = prev.trim_end().ends_with(':') && {
        let p: Vec<char> = prev.chars().take(LINE).collect();
        def_in(&p).is_some_and(|d| d.1.is_none())
    };
    if let Some(refs) = refs.filter(|_| dest_line || def_in(&cs[..n]).is_some()) {
        out.extend(refs().defs_on(l));
    }
    merge(out)
}

/// MD-10: the targets of the inline links of a line that lie (no reference, no HTML, no paragraph).
pub fn deceptive_targets(cs: &[char]) -> Vec<Range<usize>> {
    let n = cs.len().min(LINE);
    let rs = readings(&cs[..n]).0;
    merge(on_line(&scan_all(&cs[..n], &rs, None, &mut Budget::frame(), false), 0, n))
}
