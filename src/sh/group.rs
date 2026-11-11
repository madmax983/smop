//! Process groups for [`CommandBuilder::spawn_guarded`](super::CommandBuilder::spawn_guarded).
//!
//! std has no stable call that kills a process group. smop has no `unsafe`
//! code, and `sh` adds no crate. Thus smop runs the kill tool of the
//! platform (ADR 0013):
//!
//! - Unix: the `kill` builtin of `/bin/sh`, as `kill -s KILL -- -PGID`.
//! - Windows: `taskkill /T /F /PID PID`. It kills the child and the
//!   processes that it started.

use std::io;
use std::process::{Child, Command, Stdio};

/// Puts the child of `command` in a new process group.
///
/// On Unix, the group id is then the process id of the child. On Windows,
/// the flag `CREATE_NEW_PROCESS_GROUP` does the same for console events.
/// In both cases, Ctrl-C at the terminal does not reach the child.
pub(super) fn isolate(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// See `CREATE_NEW_PROCESS_GROUP` in the Win32 process creation flags.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

/// Kills the process group of `child` with `SIGKILL` (Unix), or its process
/// tree (Windows). Does not reap `child`.
///
/// Call this before the reap. Until the reap, the child holds its process
/// id, so no other group can get the same id.
///
/// # Errors
///
/// The kill tool does not start, or reports a failure. It reports a
/// failure when no process of the group is left.
pub(super) fn kill(child: &Child) -> io::Result<()> {
    let pid = child.id().to_string();
    let status = tool(&pid)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "could not kill the process group {pid} ({status})"
        )))
    }
}

/// Makes the command that kills the group or tree of `pid`.
#[cfg(not(windows))]
fn tool(pid: &str) -> Command {
    // The `--` stops `-PGID` from being read as a signal name.
    let mut command = Command::new(super::SH);
    command.args(["-c", r#"kill -s KILL -- "-$1""#, "sh", pid]);
    command
}

/// Makes the command that kills the group or tree of `pid`.
#[cfg(windows)]
fn tool(pid: &str) -> Command {
    let mut command = Command::new("taskkill");
    command.args(["/T", "/F", "/PID", pid]);
    command
}
