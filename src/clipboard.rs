//! Clipboard adapter for the `y` (yank) keybinding.
//!
//! Picks the first working backend among `wl-copy`, `xclip`, `xsel`. The
//! picker only ever copies short session names so we do not bother with
//! fallbacks like a temp file or OSC 52 escape sequences — pasting a
//! session name into another terminal is the only intended use.

use std::io::Write;
use std::process::{Command, Stdio};

/// Copy `text` to the system clipboard. Returns the name of the backend
/// used on success, or a descriptive error suitable for a one-line UI
/// flash on failure.
pub fn copy(text: &str) -> Result<&'static str, String> {
    let candidates: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["-b", "-i"]),
    ];

    for (bin, args) in candidates {
        match try_copy(bin, args, text) {
            Ok(()) => return Ok(*bin),
            Err(CopyErr::NotFound) => continue,
            Err(CopyErr::Failed(e)) => return Err(format!("{bin}: {e}")),
        }
    }
    Err("no clipboard tool found (install wl-copy, xclip, or xsel)".into())
}

enum CopyErr {
    NotFound,
    Failed(String),
}

fn try_copy(bin: &str, args: &[&str], text: &str) -> Result<(), CopyErr> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(CopyErr::NotFound),
        Err(e) => return Err(CopyErr::Failed(e.to_string())),
    };
    if let Some(mut stdin) = child.stdin.take()
        && let Err(e) = stdin.write_all(text.as_bytes())
    {
        return Err(CopyErr::Failed(e.to_string()));
    }
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(CopyErr::Failed(format!("exited {status}"))),
        Err(e) => Err(CopyErr::Failed(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Basic smoke: `copy` returns Err when no clipboard tool is on PATH.
    /// We can't reliably know which tools are installed on a developer's
    /// box, so the test only asserts the "no tool" case by clearing PATH.
    #[test]
    fn copy_errors_when_no_tools_on_path() {
        // SAFETY: tests in this crate already mutate env vars in other
        // suites (HOME for ~/git resolution) without locking; this is the
        // same pattern. PATH = "" so every spawn fails with NotFound.
        let original = std::env::var_os("PATH");
        unsafe {
            std::env::set_var("PATH", "");
        }
        let result = copy("hello");
        unsafe {
            match original {
                Some(v) => std::env::set_var("PATH", v),
                None => std::env::remove_var("PATH"),
            }
        }
        let err = result.expect_err("expected no clipboard tool");
        assert!(err.contains("no clipboard tool"));
    }
}
