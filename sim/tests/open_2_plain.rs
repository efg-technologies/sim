//! OPEN-2, PLAIN-1: once the switch that forbids every program is on, the opener is not started, whatever
//! asks (a process of its own: the switch is one way and global).
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::io;
use sim_core::io::filter;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

#[test]
fn open_2_with_programs_forbidden_the_opener_is_never_started() {
    let dir = temp_dir("o2-forbid");
    let (opener, witness) = (dir.0.join(io::OPENER), dir.0.join("ran"));
    std::fs::write(&opener, format!("#!/bin/sh\ntouch '{}'\n", witness.display())).unwrap();
    std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755)).unwrap();
    filter::forbid_programs();
    let said = io::browse(Some(&opener), "https://example.com").unwrap_err();
    assert_eq!(said, "plain mode runs no program: not opened");
    std::thread::sleep(Duration::from_millis(200));
    assert!(!witness.exists(), "the opener ran");
}
