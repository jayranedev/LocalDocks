//! Process enumeration via the Toolhelp snapshot API.
//!
//! Two passes, because no single Win32 call gives both halves of what a
//! process identity needs:
//!
//!   1. `CreateToolhelp32Snapshot` + `Process32FirstW`/`Process32NextW` walk
//!      every process and give PID, parent PID, executable name and thread
//!      count without opening a single handle.
//!   2. `OpenProcess` + `GetProcessTimes` read the creation time, which is the
//!      other half of the identity `{pid}-{startedAt}`. This is the pass that
//!      can be refused, and refusal is expected — see `CreationTime`.
//!
//! The handle is requested with `PROCESS_QUERY_LIMITED_INFORMATION`, the
//! narrowest right that answers the question. LocalDocks runs unelevated by
//! design (docs/ARCHITECTURE.md), so asking for more would turn a working
//! read into an access-denied on processes we can otherwise see.

use std::mem::size_of;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, FILETIME,
    HANDLE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::errors::SystemError;
use crate::time;

/// When a process started, or why we do not know.
///
/// docs/BACKEND.md: "AccessDenied is a value, not an error." A protected
/// process refusing to be opened is a fact about that process, not a failure of
/// the scan, so it is modelled here as data the caller must handle rather than
/// as an `Err` that would abort enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationTime {
    /// Read from `GetProcessTimes`, as Unix milliseconds.
    Known(i64),
    /// `OpenProcess` was refused. Expected for protected processes — the Idle
    /// process, System, Registry, csrss, and anti-malware services all refuse
    /// even `PROCESS_QUERY_LIMITED_INFORMATION` to an unelevated caller.
    AccessDenied,
    /// The process exited between the Toolhelp snapshot and the open. A scan
    /// is not atomic, so this is a normal race, not an error.
    Gone,
}

/// One process, shaped the way the OS reported it.
///
/// Deliberately not `models::ProcessRow`: this is platform data, and the
/// mapping to the IPC contract belongs in `logic`, which can be tested without
/// Windows. Nothing here is invented — every field is a value Win32 returned.
#[derive(Debug, Clone)]
pub struct RawProcess {
    pub pid: u32,
    pub parent_pid: u32,
    /// Executable file name, e.g. `node.exe`. Toolhelp gives the file name
    /// only, never a full path; the full path needs a second call and is not
    /// part of this milestone.
    pub name: String,
    pub thread_count: u32,
    pub created_at: CreationTime,
}

/// A handle that closes itself.
///
/// Enumeration runs once per sampler tick. A handle leaked on an early return
/// would be a leak measured in handles per second, and the paths below have
/// several early returns. Making the close structural removes the question.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // The only documented failure is an invalid handle, which cannot occur
        // here: this type is only constructed from a successful open and Drop
        // runs once. Discarding the result is correct; unwrapping would be a
        // panic on a system call in a command-reachable path.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Enumerate every process visible to the current user.
///
/// Returns `Err` only when the snapshot itself could not be taken — that is a
/// real scan failure. Per-process refusals are carried in `CreationTime` and
/// never abort the walk.
pub fn enumerate() -> Result<Vec<RawProcess>, SystemError> {
    // SAFETY: TH32CS_SNAPPROCESS with PID 0 is the documented way to snapshot
    // all processes; it takes no pointers and cannot alias.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|e| to_system_error("CreateToolhelp32Snapshot", &e))?;
    let snapshot = OwnedHandle(snapshot);

    let mut entry = PROCESSENTRY32W {
        // Win32 rejects the call outright if this is not set. It is how the
        // API versions its own struct.
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    // SAFETY: `entry` is a live, correctly sized PROCESSENTRY32W owned by this
    // frame, and `snapshot.0` came from a successful CreateToolhelp32Snapshot.
    if let Err(e) = unsafe { Process32FirstW(snapshot.0, &mut entry) } {
        // An empty process list is impossible on a running system, so unlike
        // Process32NextW below there is no "normal end" reading of this.
        return Err(to_system_error("Process32FirstW", &e));
    }

    let mut processes = Vec::with_capacity(256);

    loop {
        processes.push(RawProcess {
            pid: entry.th32ProcessID,
            parent_pid: entry.th32ParentProcessID,
            name: exe_name(&entry.szExeFile),
            thread_count: entry.cntThreads,
            created_at: creation_time(entry.th32ProcessID),
        });

        // SAFETY: same invariants as the Process32FirstW call above.
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => continue,
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(e) => {
                // Partial results beat no results: the processes already
                // collected are real. Logged rather than swallowed so a
                // truncated walk is visible in the log, not just in a count
                // that happens to look low.
                log::warn!(
                    "Process32NextW stopped early after {} processes: {e}",
                    processes.len()
                );
                break;
            }
        }
    }

    Ok(processes)
}

/// Read a process's creation time, or record why we could not.
fn creation_time(pid: u32) -> CreationTime {
    // PID 0 is the Idle "process", which is a scheduler bookkeeping entry
    // rather than a process. OpenProcess rejects it with a parameter error;
    // naming that here keeps it out of the access-denied count, which should
    // mean "refused", not "does not exist".
    if pid == 0 {
        return CreationTime::Gone;
    }

    // SAFETY: no pointers are passed; the returned handle is taken into
    // OwnedHandle on the next line so it is closed on every path below.
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => OwnedHandle(h),
        Err(e) => {
            let code = e.code();
            if code == ERROR_ACCESS_DENIED.to_hresult() {
                return CreationTime::AccessDenied;
            }
            if code == ERROR_INVALID_PARAMETER.to_hresult() {
                // The documented result for a PID that no longer exists.
                return CreationTime::Gone;
            }
            log::debug!("OpenProcess({pid}) failed unexpectedly: {e}");
            return CreationTime::AccessDenied;
        }
    };

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: all four out-params are live FILETIMEs owned by this frame, and
    // the handle carries PROCESS_QUERY_LIMITED_INFORMATION, which is the right
    // GetProcessTimes requires.
    match unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) } {
        Ok(()) => CreationTime::Known(time::filetime_to_unix_millis(
            created.dwLowDateTime,
            created.dwHighDateTime,
        )),
        Err(e) => {
            log::debug!("GetProcessTimes({pid}) failed: {e}");
            CreationTime::Gone
        }
    }
}

/// `szExeFile` is a fixed 260-wide buffer; the name is everything before the
/// first NUL. Decoding lossily rather than failing: a name we cannot decode is
/// still a process the user should see, and Windows does not guarantee that
/// every file name is valid UTF-16.
fn exe_name(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

fn to_system_error(call: &'static str, e: &windows::core::Error) -> SystemError {
    SystemError::api_failure(call, e.code().0 as u32, e.message())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_name_stops_at_the_first_nul() {
        let mut buffer = [0u16; 260];
        for (i, c) in "node.exe".encode_utf16().enumerate() {
            buffer[i] = c;
        }
        assert_eq!(exe_name(&buffer), "node.exe");
    }

    #[test]
    fn exe_name_handles_a_completely_full_buffer() {
        // No NUL terminator at all: the whole buffer is the name.
        let buffer = [b'a' as u16; 8];
        assert_eq!(exe_name(&buffer), "aaaaaaaa");
    }

    #[test]
    fn exe_name_of_an_empty_buffer_is_empty() {
        assert_eq!(exe_name(&[0u16; 260]), "");
    }

    /// An invariant test rather than a count test: the number of processes on
    /// a machine is not a fact a test may assert. What must hold is that the
    /// walk succeeds, finds the caller, and reports plausible values.
    #[test]
    fn enumerate_finds_at_least_the_test_process_itself() {
        let processes = enumerate().expect("process enumeration must succeed");

        assert!(
            processes.len() > 1,
            "a running Windows system always has more than one process"
        );

        let me = std::process::id();
        let found = processes
            .iter()
            .find(|p| p.pid == me)
            .expect("the enumerating process must appear in its own enumeration");

        assert!(!found.name.is_empty(), "the caller's name must be readable");
        assert!(found.thread_count >= 1, "a live process has threads");
        assert!(
            matches!(found.created_at, CreationTime::Known(ms) if ms > 0),
            "a process can always read its own creation time, got {:?}",
            found.created_at
        );
    }

    /// PIDs are unique at any instant. A duplicate means the walk re-read an
    /// entry, which would double-count every process downstream.
    #[test]
    fn enumerated_pids_are_unique() {
        let processes = enumerate().expect("process enumeration must succeed");
        let mut seen = std::collections::HashSet::new();
        for p in &processes {
            assert!(seen.insert(p.pid), "pid {} enumerated twice", p.pid);
        }
    }

    /// Access denial is expected and must stay a value. This asserts the shape
    /// of the outcome, not how many processes land in each bucket — that
    /// depends on what is running and on whether the run is elevated.
    #[test]
    fn every_process_reports_a_creation_time_outcome() {
        let processes = enumerate().expect("process enumeration must succeed");
        for p in &processes {
            match p.created_at {
                CreationTime::Known(ms) => {
                    assert!(ms > 0, "pid {} reported a known but zero start", p.pid)
                }
                CreationTime::AccessDenied | CreationTime::Gone => {}
            }
        }
    }
}
