//! The three command-driven operations: read a process's tier-2 fields,
//! terminate it, and hand a URL to the shell.
//!
//! Everything here is reached from a Tauri command rather than from the
//! sampler, so two rules apply throughout:
//!
//!   * **Verify identity before acting.** Windows recycles PIDs. Every entry
//!     point re-opens the PID, re-reads its creation time and compares it with
//!     the identity the caller supplied. A mismatch is a refusal, and
//!     docs/ARCHITECTURE.md § 3 calls that "the difference between a tool
//!     people trust with kill rights and one they do not".
//!   * **No panics.** docs/BACKEND.md forbids `unwrap`/`expect` on any path a
//!     command can reach.
//!
//! Least privilege is chosen per operation: reading details asks only for
//! `PROCESS_QUERY_LIMITED_INFORMATION`; terminating adds `PROCESS_TERMINATE`
//! and nothing else. No privilege is enabled, `SeDebugPrivilege` least of all,
//! and the app never elevates.

use windows::core::{HSTRING, PWSTR};
use windows::Wdk::System::Threading::{NtQueryInformationProcess, PROCESSINFOCLASS};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, FILETIME, HANDLE, MAX_PATH,
    UNICODE_STRING,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::errors::SystemError;
use crate::logic::url;
use crate::models::{FieldState, ProcessDetail, ProcessId, TerminateResult};
use crate::time;

/// `ProcessCommandLineInformation`.
///
/// Not exposed as a named constant by the `windows` crate. Windows 8.1 and
/// later answer this class with the target's command line as a
/// `UNICODE_STRING`, which docs/BACKEND.md calls "much saner" than walking the
/// PEB by hand — it needs no `PROCESS_VM_READ`, no `ReadProcessMemory`, and it
/// is not bitness-fragile.
const PROCESS_COMMAND_LINE_INFORMATION: PROCESSINFOCLASS = PROCESSINFOCLASS(60);

/// What opening a process for a verified operation produced.
enum Opened {
    /// The handle, and the caller may proceed.
    Verified(OwnedHandle),
    /// The PID is alive but is not the process the caller meant.
    IdentityMismatch { actual: String },
    /// The process is gone, or never existed.
    Gone,
    /// The process exists but this user may not open it.
    Denied,
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // Closing cannot meaningfully fail here: the handle came from a
        // successful open and is closed once. Unwrapping would be a panic on a
        // command-reachable path.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Open a PID and prove it is still the process the identity describes.
///
/// This is the whole safety model in one function. Callers get a handle only
/// when the creation time read *now* matches the one recorded when the
/// snapshot was taken.
fn open_verified(pid: u32, expected_started_at: &str, rights: u32) -> Opened {
    // SAFETY: no pointers are passed; the handle is taken into OwnedHandle
    // immediately so every path below closes it.
    let handle = match unsafe {
        OpenProcess(
            windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS(rights),
            false,
            pid,
        )
    } {
        Ok(h) => OwnedHandle(h),
        Err(e) => {
            let code = e.code();
            if code == ERROR_ACCESS_DENIED.to_hresult() {
                return Opened::Denied;
            }
            if code == ERROR_INVALID_PARAMETER.to_hresult() {
                return Opened::Gone;
            }
            log::debug!("OpenProcess({pid}) failed: {e}");
            return Opened::Gone;
        }
    };

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: four live FILETIMEs owned by this frame; the handle carries
    // PROCESS_QUERY_LIMITED_INFORMATION, which GetProcessTimes requires.
    if let Err(e) =
        unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) }
    {
        log::debug!("GetProcessTimes({pid}) failed: {e}");
        return Opened::Gone;
    }

    let actual = time::to_iso8601(time::filetime_to_unix_millis(
        created.dwLowDateTime,
        created.dwHighDateTime,
    ));

    // String equality is exact: both sides are produced by `to_iso8601`, so
    // there is no date parsing, no timezone and no precision question.
    if actual != expected_started_at {
        return Opened::IdentityMismatch { actual };
    }

    Opened::Verified(handle)
}

// ------------------------------------------------------------------- details

/// Read the tier-2 fields for a verified process.
///
/// Never called from the sampler — docs/ARCHITECTURE.md § 4 puts these behind
/// a detail-panel open precisely because they are expensive and awkward.
///
/// Infallible by design. Every way this can go wrong is a *state of a field*,
/// not an error: a refused process yields `denied`, a vanished or recycled one
/// yields `unavailable`. That is exactly what `FieldState` is for, and it means
/// the panel always has something honest to render.
pub fn process_detail(process_id: &ProcessId, pid: u32, started_at: &str) -> ProcessDetail {
    let handle = match open_verified(pid, started_at, PROCESS_QUERY_LIMITED_INFORMATION.0) {
        Opened::Verified(h) => h,
        Opened::Denied => return ProcessDetail::all(process_id.clone(), FieldState::Denied),
        Opened::Gone => {
            log::debug!("process detail for {process_id}: process is gone");
            return ProcessDetail::all(process_id.clone(), FieldState::Unavailable);
        }
        Opened::IdentityMismatch { actual } => {
            // Refused, and loudly: this is a recycled PID, which is the case
            // the identity model exists for. Reading on would have returned
            // another process's command line.
            log::warn!(
                "process detail for {process_id} refused: PID {pid} now started at {actual}"
            );
            return ProcessDetail::all(process_id.clone(), FieldState::Unavailable);
        }
    };

    ProcessDetail {
        process_id: process_id.clone(),
        executable: read_executable(&handle, pid),
        command_line: read_command_line(&handle, pid),
        // Deferred, not forgotten. The only route to a process's current
        // directory is walking its PEB with PROCESS_VM_READ and
        // ReadProcessMemory, which docs/BACKEND.md rates "bitness-fragile" and
        // recommends deciding on "when V2 forces the issue, not before". It
        // also needs a wider handle than anything else here. `Unavailable` is
        // the honest answer until then — see docs/ROADMAP.md.
        working_directory: FieldState::Unavailable,
    }
}

/// The command line of a verified process, for the Developer classifier.
///
/// A deliberate, bounded amendment to the two-tier rule in
/// docs/ARCHITECTURE.md § 4, which otherwise keeps command lines out of the
/// scan loop. Three things keep it honest:
///
///   * **Only services, and only undecidable ones.** `classify::needs_command_line`
///     asks for a handle only where the answer could change the verdict —
///     a general-purpose runtime. A dedicated program is already decided by its
///     name; an excluded one is already refused. On the machine this was
///     measured against that is a single process, out of ~390.
///   * **Once per process lifetime.** The sampler caches the result against the
///     process identity, so a service that lives for an hour is read once, not
///     3,600 times. A failure is cached too, so an unreadable process is not
///     retried every tick.
///   * **Same verification as everything else.** It goes through
///     `open_verified`, so a recycled PID yields nothing rather than the wrong
///     process's command line — which would mean the wrong classification.
///
/// `None` for every failure. The classifier distinguishes "no command line"
/// from "no signature in it", and both are reported honestly to the user.
pub fn command_line_for(pid: u32, started_at: &str) -> Option<String> {
    let handle = match open_verified(pid, started_at, PROCESS_QUERY_LIMITED_INFORMATION.0) {
        Opened::Verified(h) => h,
        _ => return None,
    };
    match read_command_line(&handle, pid) {
        FieldState::Ok { value } => Some(value),
        _ => None,
    }
}

/// Full path of the running image.
fn read_executable(handle: &OwnedHandle, pid: u32) -> FieldState<String> {
    let mut buffer = [0u16; MAX_PATH as usize];
    let mut size = buffer.len() as u32;

    // SAFETY: `buffer` is a live MAX_PATH-wide array and `size` reports its
    // length; the handle carries PROCESS_QUERY_LIMITED_INFORMATION, which is
    // what this call requires.
    match unsafe {
        QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    } {
        Ok(()) => text(&buffer[..size as usize]),
        Err(e) => {
            log::debug!("QueryFullProcessImageNameW({pid}) failed: {e}");
            FieldState::Unavailable
        }
    }
}

/// The command line the process was started with.
fn read_command_line(handle: &OwnedHandle, pid: u32) -> FieldState<String> {
    // Ask for the size first: command lines have no useful upper bound, and
    // guessing one would silently truncate the field V2 project detection is
    // going to depend on.
    let mut needed: u32 = 0;
    // SAFETY: a null buffer with length 0 is the documented way to ask for the
    // required size; `needed` is a live u32 owned by this frame.
    let probe = unsafe {
        NtQueryInformationProcess(
            handle.0,
            PROCESS_COMMAND_LINE_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };

    // Anything other than "you need a bigger buffer" means this Windows build
    // does not answer the class, or the process went away.
    if needed == 0 {
        log::debug!("NtQueryInformationProcess({pid}) command line size query: {probe:?}");
        return FieldState::Unavailable;
    }

    // A UNICODE_STRING header followed by its characters; aligned by using a
    // Vec<u64> rather than a Vec<u8>, which would be aligned only by luck.
    let words = (needed as usize).div_ceil(8) + 1;
    let mut buffer = vec![0u64; words];
    let capacity = (words * 8) as u32;

    // SAFETY: `buffer` is a live, correctly aligned allocation of `capacity`
    // bytes, which is at least the size Windows just asked for.
    let status = unsafe {
        NtQueryInformationProcess(
            handle.0,
            PROCESS_COMMAND_LINE_INFORMATION,
            buffer.as_mut_ptr() as *mut _,
            capacity,
            &mut needed,
        )
    };
    if status.is_err() {
        log::debug!("NtQueryInformationProcess({pid}) command line read: {status:?}");
        return FieldState::Unavailable;
    }

    // SAFETY: on success Windows has written a UNICODE_STRING at the start of
    // the buffer, whose Buffer points inside that same allocation.
    let unicode = unsafe { &*(buffer.as_ptr() as *const UNICODE_STRING) };
    if unicode.Buffer.is_null() || unicode.Length == 0 {
        return FieldState::Unavailable;
    }

    // `Length` is in bytes, not characters.
    let len = (unicode.Length / 2) as usize;
    // SAFETY: Buffer and Length come from the call that just succeeded, and
    // both describe memory inside `buffer`, which outlives this borrow.
    let chars = unsafe { std::slice::from_raw_parts(unicode.Buffer.0, len) };
    text(chars)
}

/// Wide characters to a field, refusing to call emptiness a success.
///
/// A blank command line renders as a blank row, which reads as "this process
/// has no command line" rather than "this could not be read". `Unavailable`
/// says the true thing.
fn text(chars: &[u16]) -> FieldState<String> {
    let end = chars.iter().position(|&c| c == 0).unwrap_or(chars.len());
    let value = String::from_utf16_lossy(&chars[..end]);
    if value.trim().is_empty() {
        return FieldState::Unavailable;
    }
    FieldState::Ok { value }
}

// --------------------------------------------------------------- termination

/// Force-terminate a verified process.
///
/// V1 is force terminate and says so (docs/ARCHITECTURE.md § 6). Windows has no
/// graceful equivalent of SIGTERM for an arbitrary process, so pretending to
/// offer one would be the dishonest option.
///
/// Nothing is removed from the snapshot here. The next sampler tick stops
/// seeing the process and its rows disappear on their own — fabricating that
/// state would mean the UI could show a process as dead that is still running.
pub fn terminate(pid: u32, started_at: &str) -> TerminateResult {
    let handle = match open_verified(
        pid,
        started_at,
        PROCESS_QUERY_LIMITED_INFORMATION.0 | PROCESS_TERMINATE.0,
    ) {
        Opened::Verified(h) => h,
        Opened::Denied => {
            log::info!("terminate refused for PID {pid}: access denied");
            return TerminateResult::Denied;
        }
        Opened::Gone => {
            return TerminateResult::Stale {
                message: format!(
                    "PID {pid} is no longer running. It exited before this action reached it."
                ),
            }
        }
        Opened::IdentityMismatch { actual } => {
            // The success path for the safety model, not a failure.
            log::warn!("terminate refused for PID {pid}: now started at {actual}");
            return TerminateResult::Stale {
                message: format!(
                    "PID {pid} now belongs to a different process, started at {actual}. \
                     Nothing was terminated."
                ),
            };
        }
    };

    // SAFETY: the handle carries PROCESS_TERMINATE and was verified to be the
    // intended process moments ago.
    match unsafe { TerminateProcess(handle.0, 1) } {
        Ok(()) => {
            log::info!("terminated PID {pid} (started {started_at})");
            TerminateResult::Terminated
        }
        Err(e) => {
            if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
                return TerminateResult::Denied;
            }
            log::error!("TerminateProcess({pid}) failed: {e}");
            TerminateResult::Failed {
                message: format!("Windows refused to terminate PID {pid}: {}", e.message()),
            }
        }
    }
}

// ------------------------------------------------------------------- opening

/// Hand a validated URL to whatever Windows has registered for its scheme.
///
/// `ShellExecuteW` with a verb of `open` is the OS's own "open this the way the
/// user would" mechanism — it is not a shell, it does not parse arguments and
/// it cannot be talked into running a command. The dangerous part of this
/// operation is the string, and the string has already been through
/// `logic::url::validate`, which allows only `http://` and `https://` and
/// refuses anything containing whitespace, a control character or a NUL.
pub fn open_external(raw: &str) -> Result<(), SystemError> {
    let url = url::validate(raw).map_err(|rejection| {
        log::warn!("refused to open {raw:?}: {}", rejection.reason());
        SystemError::rejected_url(rejection.reason())
    })?;

    let wide = HSTRING::from(url);
    let verb = HSTRING::from("open");

    // SAFETY: both strings are live NUL-terminated wide strings owned by this
    // frame for the duration of the call. A null hwnd means "no owner window",
    // which is correct for opening a browser.
    let result = unsafe { ShellExecuteW(None, &verb, &wide, None, None, SW_SHOWNORMAL) };

    // ShellExecuteW returns a fake HINSTANCE: greater than 32 means success.
    // This is the documented contract, odd as it looks.
    let code = result.0 as usize;
    if code > 32 {
        log::info!("opened {url}");
        return Ok(());
    }

    log::error!("ShellExecuteW({url}) failed with {code}");
    Err(SystemError::api_failure(
        "ShellExecuteW",
        code as u32,
        "Windows could not open the URL. Is a browser registered for http links?",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identity check has to hold against a real process. This one is
    /// ours, so it is guaranteed to exist and to be openable.
    #[test]
    fn a_correct_identity_opens_and_a_wrong_one_does_not() {
        let pid = std::process::id();

        // Learn this process's real creation time by asking with a deliberately
        // wrong one and reading the mismatch back.
        let actual = match open_verified(
            pid,
            "definitely-not-the-start-time",
            PROCESS_QUERY_LIMITED_INFORMATION.0,
        ) {
            Opened::IdentityMismatch { actual } => actual,
            other => panic!("expected a mismatch, got {}", describe(&other)),
        };

        // The same PID with the right time must now verify.
        assert!(
            matches!(
                open_verified(pid, &actual, PROCESS_QUERY_LIMITED_INFORMATION.0),
                Opened::Verified(_)
            ),
            "the correct identity must verify"
        );
    }

    #[test]
    fn a_pid_that_does_not_exist_reports_gone() {
        // A PID Windows will not have assigned: PIDs are multiples of four and
        // this is far above any plausible live value.
        assert!(matches!(
            open_verified(
                0xFFFF_FFF0,
                "2026-08-28T09:00:00.000Z",
                PROCESS_QUERY_LIMITED_INFORMATION.0
            ),
            Opened::Gone
        ));
    }

    #[test]
    fn a_stale_identity_is_refused_rather_than_terminated() {
        // The important one: this asks to terminate a real, live process — this
        // very test process — with the wrong creation time. It must come back
        // stale, and the test surviving to its own assertion is the proof.
        let result = terminate(std::process::id(), "2000-01-01T00:00:00.000Z");
        match result {
            TerminateResult::Stale { message } => {
                assert!(message.contains("different process"), "got: {message}");
            }
            other => panic!("a stale identity must not terminate: {other:?}"),
        }
    }

    #[test]
    fn terminating_a_vanished_pid_reports_stale_not_success() {
        let result = terminate(0xFFFF_FFF0, "2026-08-28T09:00:00.000Z");
        assert!(matches!(result, TerminateResult::Stale { .. }));
    }

    #[test]
    fn details_for_this_process_read_back_something_true() {
        let pid = std::process::id();
        let actual = match open_verified(pid, "wrong", PROCESS_QUERY_LIMITED_INFORMATION.0) {
            Opened::IdentityMismatch { actual } => actual,
            other => panic!("expected a mismatch, got {}", describe(&other)),
        };
        let id = crate::models::make_process_id(pid, &actual);
        let detail = process_detail(&id, pid, &actual);

        assert_eq!(detail.process_id, id);

        match &detail.executable {
            FieldState::Ok { value } => {
                assert!(value.contains(':'), "expected a full path, got {value}");
                assert!(!value.trim().is_empty());
            }
            other => panic!("a process can always read its own image path: {other:?}"),
        }

        match &detail.command_line {
            FieldState::Ok { value } => assert!(!value.trim().is_empty()),
            // Acceptable on a Windows build that does not answer the class.
            FieldState::Unavailable => {}
            FieldState::Denied => panic!("a process is not denied its own command line"),
        }

        // Documented as deferred for V1.
        assert!(matches!(detail.working_directory, FieldState::Unavailable));
    }

    #[test]
    fn details_for_a_stale_identity_are_unavailable_rather_than_another_process_s() {
        let pid = std::process::id();
        let id = crate::models::make_process_id(pid, "2000-01-01T00:00:00.000Z");
        let detail = process_detail(&id, pid, "2000-01-01T00:00:00.000Z");

        assert!(matches!(detail.executable, FieldState::Unavailable));
        assert!(matches!(detail.command_line, FieldState::Unavailable));
        assert!(matches!(detail.working_directory, FieldState::Unavailable));
    }

    #[test]
    fn details_for_a_vanished_process_are_unavailable() {
        let id = crate::models::make_process_id(0xFFFF_FFF0, "2026-08-28T09:00:00.000Z");
        let detail = process_detail(&id, 0xFFFF_FFF0, "2026-08-28T09:00:00.000Z");
        assert!(matches!(detail.executable, FieldState::Unavailable));
    }

    #[test]
    fn a_refused_url_never_reaches_the_shell() {
        for bad in [
            "javascript:alert(1)",
            "file:///C:/Windows/System32/cmd.exe",
            "shell:startup",
            "http://localhost\0javascript:alert(1)",
            "",
        ] {
            let e = open_external(bad).expect_err("{bad} must be refused");
            let v = serde_json::to_value(&e).unwrap();
            assert_eq!(v["kind"], "rejectedUrl", "{bad} should be a URL rejection");
        }
    }

    #[test]
    fn empty_text_is_unavailable_rather_than_an_empty_success() {
        assert!(matches!(text(&[]), FieldState::Unavailable));
        assert!(matches!(text(&[0]), FieldState::Unavailable));
        assert!(matches!(text(&[32, 32, 0]), FieldState::Unavailable));
        match text(&[b'a' as u16, b'b' as u16, 0, b'c' as u16]) {
            FieldState::Ok { value } => assert_eq!(value, "ab"),
            other => panic!("expected ok, got {other:?}"),
        }
    }

    #[test]
    fn the_sampler_facing_command_line_read_verifies_identity_like_everything_else() {
        let pid = std::process::id();
        let actual = match open_verified(pid, "wrong", PROCESS_QUERY_LIMITED_INFORMATION.0) {
            Opened::IdentityMismatch { actual } => actual,
            other => panic!("expected a mismatch, got {}", describe(&other)),
        };

        // A correct identity reads something, or nothing on a Windows build
        // that does not answer the class — but never another process's line.
        if let Some(line) = command_line_for(pid, &actual) {
            assert!(!line.trim().is_empty());
        }

        // A stale identity and a dead PID both yield nothing.
        assert!(command_line_for(pid, "2000-01-01T00:00:00.000Z").is_none());
        assert!(command_line_for(0xFFFF_FFF0, &actual).is_none());
    }

    fn describe(o: &Opened) -> String {
        match o {
            Opened::Verified(_) => "verified".into(),
            Opened::Gone => "gone".into(),
            Opened::Denied => "denied".into(),
            Opened::IdentityMismatch { actual } => format!("mismatch({actual})"),
        }
    }
}
