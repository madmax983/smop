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

/// The Win32 flag that puts a child in a new process group.
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

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
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

/// Kills the process group of `child` with `SIGKILL` (Unix), or its process
/// tree (Windows). Does not reap `child`.
///
/// Call this before the reap. Until the reap, the child holds its process
/// id, so no other group can get the same id.
///
/// Returns `Ok(false)` when the kill tool reports a failure, for example
/// when no process of the group is left.
///
/// # Errors
///
/// The kill tool does not start.
pub(super) fn kill(child: &Child) -> io::Result<bool> {
    let status = tool(&child.id().to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Makes the command that kills the group of `pid`.
///
/// The tool has its own process group, so a second Ctrl-C does not stop
/// it. It gets no environment, so an exported shell function can not
/// replace `kill`. `command` also skips such functions.
#[cfg(not(windows))]
fn tool(pid: &str) -> Command {
    let mut command = Command::new(super::SH);
    // With `--`, `kill` reads `-PGID` as a group, not as a signal name.
    command
        .args(["-c", r#"command kill -s KILL -- "-$1""#, "sh", pid])
        .env_clear()
        .current_dir("/");
    isolate(&mut command);
    command
}

/// Makes the command that kills the tree of `pid`.
///
/// The full path stops a `taskkill.exe` next to the program from running.
/// The tool has its own process group and no console window.
#[cfg(windows)]
fn tool(pid: &str) -> Command {
    use std::os::windows::process::CommandExt;
    /// The Win32 flag that starts a console program with no window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let program = std::env::var_os("SystemRoot").map_or_else(
        || std::path::PathBuf::from("taskkill"),
        |root| std::path::Path::new(&root).join(r"System32\taskkill.exe"),
    );
    let mut command = Command::new(program);
    command
        .args(["/T", "/F", "/PID", pid])
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    command
}
