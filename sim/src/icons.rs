//! UI-2, UI-10, SYN-4, ICON-1
//! The Nerd Font icon of a tree row (pure): a closed or an open folder, and a file by its whole
//! name or its extension, from a table of about forty entries; the glyph only, for icons have no color.

/// The folder, closed and open.
pub const CLOSED: char = '\u{f07b}';
pub const OPEN: char = '\u{f07c}';
const FILE: char = '\u{f15b}';

/// The names (lower case, whole) and the extensions that have an icon of their own. Icons are
/// monochrome (UI-1): the secondary gray, whatever the type.
const NAMES: &[(&str, char)] = &[
    ("dockerfile", '\u{f308}'),
    ("makefile", '\u{e779}'),
    (".gitignore", '\u{e702}'),
    (".gitattributes", '\u{e702}'),
    (".gitconfig", '\u{e702}'),
    (".zshrc", '\u{e795}'),
    (".zshenv", '\u{e795}'),
    (".bashrc", '\u{e795}'),
    (".profile", '\u{e795}'),
    (".editorconfig", '\u{e615}'),
    ("license", '\u{e60a}'),
];

const EXTS: &[(&str, char)] = &[
    ("rs", '\u{e7a8}'),
    ("md", '\u{e73e}'),
    ("toml", '\u{e615}'),
    ("json", '\u{e60b}'),
    ("yaml", '\u{e6a8}'),
    ("yml", '\u{e6a8}'),
    ("sh", '\u{e795}'),
    ("zsh", '\u{e795}'),
    ("bash", '\u{e795}'),
    ("lua", '\u{e620}'),
    ("py", '\u{e73c}'),
    ("js", '\u{e781}'),
    ("ts", '\u{e628}'),
    ("tsx", '\u{e7ba}'),
    ("jsx", '\u{e7ba}'),
    ("go", '\u{e627}'),
    ("c", '\u{e61e}'),
    ("h", '\u{e61e}'),
    ("cpp", '\u{e61d}'),
    ("hpp", '\u{e61d}'),
    ("html", '\u{e736}'),
    ("css", '\u{e749}'),
    ("scss", '\u{e74b}'),
    ("sql", '\u{e706}'),
    ("lock", '\u{f023}'),
    ("png", '\u{f1c5}'),
    ("jpg", '\u{f1c5}'),
    ("svg", '\u{f1c5}'),
    ("pdf", '\u{f1c1}'),
    ("txt", '\u{f15c}'),
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

/// The icon of a folder (open or closed), or of a file by its name: the glyph only, a Nerd Font's.
pub fn of(name: &str, is_dir: bool, open: bool) -> char {
    if is_dir {
        return if open { OPEN } else { CLOSED };
    }
    let n = target(name);
    if n.starts_with("readme") {
        return '\u{f02d}';
    }
    if n.starts_with("license") {
        return '\u{e60a}';
    }
    let named = NAMES.iter().find(|e| e.0 == n);
    let ext = n.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()).map(|(_, e)| e);
    let by_ext = ext.and_then(|e| EXTS.iter().find(|x| x.0 == e));
    named.or(by_ext).map_or(FILE, |e| e.1)
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
