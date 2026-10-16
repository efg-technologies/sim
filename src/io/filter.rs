//! PROP-5, PROP-6
//! Running `sh -c` in its own process group, and cancelling the whole group.
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};

pub struct Output {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> io::Result<JoinHandle<T>> {
    Builder::new().spawn(f)
}

// Everything until EOF, in chunks, so that what was read is not lost when the waiter gives up.
fn pump(pipe: Option<impl Read>, out: Sender<Vec<u8>>) {
    let (Some(mut p), mut buf) = (pipe, [0; 1 << 16]) else { return };
    while let Some(n) = p.read(&mut buf).ok().filter(|&n| n > 0) {
        drop(out.send(buf[..n].to_vec())); // nobody listens any more: the rest is let go
    }
}

/// Runs `sh -c cmd` in its own process group, writes `input` to its stdin from a separate thread
/// (so a large input can't deadlock), collects stdout and stderr, and calls `done` once it exits.
/// Returns the process-group id for `cancel`.
pub fn spawn(
    cmd: &str,
    input: Vec<u8>,
    done: impl FnOnce(Output) + Send + 'static,
) -> io::Result<u32> {
    let mut child = Command::new("sh")
        .args(["-c", cmd])
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pgid = child.id();
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let ((otx, orx), (etx, erx)) = (mpsc::channel(), mpsc::channel());
    let threads = (|| {
        bg(move || stdin.map(|mut s| s.write_all(&input)))?; // EPIPE: it did not want the input
        bg(move || pump(stdout, otx))?;
        bg(move || pump(stderr, etx))?;
        bg(move || {
            let status = child.wait().ok().and_then(|s| s.code()); // also reaps it
            // The output may take a second more to end once the shell has: a child it left behind can hold the
            // pipes open for ever (PROP-5). What the pumps sent until they are done, or `end`:
            let end = Instant::now() + Duration::from_secs(1);
            let all = |rx: Receiver<Vec<u8>>| {
                let left = || end.saturating_duration_since(Instant::now());
                std::iter::from_fn(move || rx.recv_timeout(left()).ok()).flatten().collect()
            };
            done(Output { status, stdout: all(orx), stderr: all(erx) });
        })
    })();
    if let Err(e) = threads {
        cancel(pgid); // no threads left to wait for it
        return Err(e);
    }
    Ok(pgid)
}

/// Stops the whole process group (PROP-5 Decided): SIGTERM via `kill -TERM -- -<pgid>`
/// (std::process::Command; no libc), so children such as `claude -p` stop too.
/// Only for a group `spawn` returned whose `done` has not been called: the id may be reused after.
pub fn cancel(pgid: u32) {
    if pgid > 1 {
        // 0 and 1 would mean our own group and every process. `output` gives it no input and takes its words.
        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pgid}")]).output();
    }
}
