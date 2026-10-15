//! CLI-1, CLI-2
//! The command line: `si [+cmd] [file]`, as the keys to type after the first render.
pub struct Cli {
    pub file: Option<String>,
    /// Keys to type after the first render (`\n` is Enter).
    pub keys: String,
}

/// Splits `file:line[:col]`, unless a file with that exact name exists.
fn split_pos<'a>(f: &'a str, exists: &impl Fn(&str) -> bool) -> (&'a str, Vec<usize>) {
    let num = |s: &str| if s.bytes().all(|b| b.is_ascii_digit()) { s.parse().ok() } else { None };
    let Some((rest, last)) = f.rsplit_once(':').filter(|_| !exists(f)) else { return (f, vec![]) };
    let Some(last) = num(last).filter(|_| !rest.is_empty()) else { return (f, vec![]) };
    if let Some((name, line)) = rest.rsplit_once(':')
        && let Some(line) = num(line)
        && !name.is_empty()
    {
        return (name, vec![line, last]);
    }
    (rest, vec![last])
}

/// `None` means a usage error.
pub fn parse(args: &[String], exists: impl Fn(&str) -> bool) -> Option<Cli> {
    let (mut cmd, mut file, mut opts) = (None, None, true);
    for a in args {
        match a.as_str() {
            "--" if opts => opts = false,
            s if opts && s.starts_with('-') => return None,
            s if opts && s.starts_with('+') => {
                if s.len() == 1 || cmd.replace(&s[1..]).is_some() {
                    return None;
                }
            }
            s => {
                if file.replace(s).is_some() {
                    return None;
                }
            }
        }
    }
    let (name, pos) = file.map_or(("", vec![]), |f| split_pos(f, &exists));
    let mut keys = pos.first().map_or(String::new(), |line| format!(":{line}\n"));
    if let Some(col) = pos.get(1).filter(|&&c| c > 1) {
        keys += &format!("{}l", col - 1);
    }
    if let Some(c) = cmd {
        keys += &format!("{}{c}\n", if c.starts_with('/') { "" } else { ":" });
    }
    Some(Cli { file: (!name.is_empty()).then(|| name.to_string()), keys })
}
