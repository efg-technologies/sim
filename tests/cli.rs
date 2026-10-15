//! CLI-1, CLI-2: how the arguments become a file and the keys typed after the first render.
use si::core::cli::parse;

fn open(args: &[&str], exists: &[&str]) -> Option<(Option<String>, String)> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    parse(&args, |f| exists.contains(&f)).map(|c| (c.file, c.keys))
}

fn at(file: &str, keys: &str) -> Option<(Option<String>, String)> {
    Some((Some(file.into()), keys.into()))
}

#[test]
fn cli_1_plus_command_becomes_keys() {
    assert_eq!(open(&["+42", "f"], &[]), at("f", ":42\n"));
    assert_eq!(open(&["+/re", "f"], &[]), at("f", "/re\n"));
    assert_eq!(open(&["f", "+s/a/b/"], &[]), at("f", ":s/a/b/\n"));
    assert_eq!(open(&[], &[]), Some((None, String::new())));
}

#[test]
fn cli_1_usage_errors_and_the_double_dash() {
    for bad in [&["-x"][..], &["a", "b"], &["+1", "+2"], &["+"], &["-", "a"], &["--", "a", "b"]] {
        assert_eq!(open(bad, &[]), None, "{bad:?}");
    }
    assert_eq!(open(&["--", "+x"], &[]), at("+x", ""));
    assert_eq!(open(&["+1", "--", "-x"], &[]), at("-x", ":1\n"));
}

#[test]
fn cli_2_file_line_and_column() {
    assert_eq!(open(&["a.rs:12"], &[]), at("a.rs", ":12\n"));
    assert_eq!(open(&["a.rs:12:5", "+q"], &[]), at("a.rs", ":12\n4l:q\n"));
    assert_eq!(open(&["a:b:3"], &[]), at("a:b", ":3\n"));
    for plain in ["a.rs", "a.rs:", "a.rs:x", "a.rs:1x", ":12", "a.rs:+3"] {
        assert_eq!(open(&[plain], &[]), at(plain, ""), "{plain}");
    }
}

#[test]
fn cli_2_an_existing_file_with_that_name_wins() {
    assert_eq!(open(&["a.rs:12"], &["a.rs:12"]), at("a.rs:12", ""));
    assert_eq!(open(&["a.rs:12:5"], &["a.rs:12"]), at("a.rs", ":12\n4l"));
}
