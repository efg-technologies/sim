//! VI-23 in sim: `:r {file}` reads the file through the shell that opens files, below the line of the file
//! that is shown, and the files open in sim stay as they were.
mod common;
use common::*;

#[test]
fn vi_23_r_file_reads_another_file_s_lines_into_the_one_shown() {
    let mut r = rig("v23-r", &["a.txt=one\ntwo\n", "b.txt=x\ny\n"], &["a.txt"]);
    r.keys(":r b.txt\n");
    assert_eq!(r.text(), "one\nx\ny\ntwo");
    assert_eq!(r.open_files().len(), 1, "{:?}", r.open_files());
    assert!(r.status().contains("2 lines read from b.txt"), "{}", r.status());
    r.keys("u");
    assert_eq!(r.text(), "one\ntwo");
}
