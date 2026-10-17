//! UI-2, UI-10, SYN-4
//! The Nerd Font icon of a tree row (pure): a closed or an open folder, and a file by its whole
//! name or its extension, from a table of about forty entries, with the accent it is drawn in.
use crate::color::Accent::{self, Amber, Blue, Green, Purple, Red, Teal};

/// The folder, closed and open.
pub const CLOSED: char = '\u{f07b}';
pub const OPEN: char = '\u{f07c}';
const FILE: char = '\u{f15b}';

/// The names (lower case, whole) and the extensions that have an icon of their own. `None` is the
/// secondary gray.
const NAMES: &[(&str, char, Option<Accent>)] = &[
    ("dockerfile", '\u{f308}', Some(Blue)),
    ("makefile", '\u{e779}', None),
    (".gitignore", '\u{e702}', Some(Red)),
    (".gitattributes", '\u{e702}', Some(Red)),
    (".gitconfig", '\u{e702}', Some(Red)),
    (".zshrc", '\u{e795}', Some(Green)),
    (".zshenv", '\u{e795}', Some(Green)),
    (".bashrc", '\u{e795}', Some(Green)),
    (".profile", '\u{e795}', Some(Green)),
    (".editorconfig", '\u{e615}', None),
    ("license", '\u{e60a}', Some(Amber)),
];

const EXTS: &[(&str, char, Option<Accent>)] = &[
    ("rs", '\u{e7a8}', Some(Amber)),
    ("md", '\u{e73e}', Some(Blue)),
    ("toml", '\u{e615}', Some(Teal)),
    ("json", '\u{e60b}', Some(Amber)),
    ("yaml", '\u{e6a8}', Some(Purple)),
    ("yml", '\u{e6a8}', Some(Purple)),
    ("sh", '\u{e795}', Some(Green)),
    ("zsh", '\u{e795}', Some(Green)),
    ("bash", '\u{e795}', Some(Green)),
    ("lua", '\u{e620}', Some(Blue)),
    ("py", '\u{e73c}', Some(Amber)),
    ("js", '\u{e781}', Some(Amber)),
    ("ts", '\u{e628}', Some(Blue)),
    ("tsx", '\u{e7ba}', Some(Teal)),
    ("jsx", '\u{e7ba}', Some(Teal)),
    ("go", '\u{e627}', Some(Teal)),
    ("c", '\u{e61e}', Some(Blue)),
    ("h", '\u{e61e}', Some(Purple)),
    ("cpp", '\u{e61d}', Some(Blue)),
    ("hpp", '\u{e61d}', Some(Purple)),
    ("html", '\u{e736}', Some(Red)),
    ("css", '\u{e749}', Some(Blue)),
    ("scss", '\u{e74b}', Some(Purple)),
    ("sql", '\u{e706}', Some(Amber)),
    ("lock", '\u{f023}', None),
    ("png", '\u{f1c5}', Some(Purple)),
    ("jpg", '\u{f1c5}', Some(Purple)),
    ("svg", '\u{f1c5}', Some(Purple)),
    ("pdf", '\u{f1c1}', Some(Red)),
    ("txt", '\u{f15c}', None),
];

/// What chezmoi's names stand for: `dot_zshrc.tmpl` is `.zshrc`. The attributes in front of a name
/// and the `.tmpl` after it are taken off, and `dot_` is a dot (lower case). The icons look a name
/// up by it, and so does the highlighter (SYN-4).
pub fn target(name: &str) -> String {
    let mut n = name.to_ascii_lowercase();
    n = n.strip_suffix(".tmpl").map_or(n.clone(), str::to_string);
    const ATTRS: [&str; 6] =
        ["private_", "readonly_", "executable_", "exact_", "empty_", "encrypted_"];
    while let Some(rest) = ATTRS.iter().find_map(|a| n.strip_prefix(a)) {
        n = rest.to_string();
    }
    n.strip_prefix("dot_").map_or(n.clone(), |r| format!(".{r}"))
}

/// The icon of a folder (open or closed) and the accent it is drawn in, or of a file by name.
pub fn of(name: &str, is_dir: bool, open: bool) -> (char, Option<Accent>) {
    if is_dir {
        return (if open { OPEN } else { CLOSED }, Some(Blue));
    }
    let n = target(name);
    if n.starts_with("readme") {
        return ('\u{f02d}', Some(Blue));
    }
    if n.starts_with("license") {
        return ('\u{e60a}', Some(Amber));
    }
    let named = NAMES.iter().find(|e| e.0 == n);
    let ext = n.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()).map(|(_, e)| e);
    let by_ext = ext.and_then(|e| EXTS.iter().find(|x| x.0 == e));
    named.or(by_ext).map_or((FILE, None), |e| (e.1, e.2))
}

/// UI-10: the file type the status line names, as Neovim does (`rust`, `markdown`, `zsh`), from
/// the whole name or the extension of what the file stands for (chezmoi's names are read as such).
pub fn filetype(name: &str) -> Option<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        ("dockerfile", "dockerfile"),
        ("makefile", "make"),
        (".zshrc", "zsh"),
        (".zshenv", "zsh"),
        (".bashrc", "bash"),
        (".profile", "sh"),
        (".gitignore", "gitignore"),
        (".gitattributes", "gitattributes"),
        (".gitconfig", "gitconfig"),
        (".editorconfig", "editorconfig"),
    ];
    const EXTS: &[(&str, &str)] = &[
        ("rs", "rust"),
        ("md", "markdown"),
        ("toml", "toml"),
        ("json", "json"),
        ("yaml", "yaml"),
        ("yml", "yaml"),
        ("sh", "sh"),
        ("zsh", "zsh"),
        ("bash", "bash"),
        ("lua", "lua"),
        ("py", "python"),
        ("js", "javascript"),
        ("ts", "typescript"),
        ("tsx", "typescriptreact"),
        ("jsx", "javascriptreact"),
        ("go", "go"),
        ("c", "c"),
        ("h", "c"),
        ("cpp", "cpp"),
        ("hpp", "cpp"),
        ("html", "html"),
        ("css", "css"),
        ("scss", "scss"),
        ("sql", "sql"),
        ("conf", "conf"),
        ("lock", "lock"),
        ("txt", "text"),
    ];
    let file = name.rsplit('/').next().unwrap_or(name);
    let n = target(file);
    let ext = n.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()).map(|(_, e)| e);
    let named = NAMES.iter().find(|e| e.0 == n);
    let by_ext = ext.and_then(|e| EXTS.iter().find(|x| x.0 == e));
    named.or(by_ext).map(|e| e.1)
}
