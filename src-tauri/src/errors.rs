//! Error taxonomy for system operations.
//!
//! Two rules from docs/BACKEND.md shape this file:
//!
//!   * "AccessDenied is a value, not an error." A process the app may not open
//!     is a fact about that process, not a failure of the scan. It is modelled
//!     in the platform layer as data (see `platform::windows::process`), and it
//!     deliberately has no variant here.
//!   * Variants appear when something can return them. The taxonomy in
//!     docs/BACKEND.md also lists `ProcessGone`, `IdentityMismatch` and
//!     `InvalidPid`; those belong to `terminate_process`, which does not exist
//!     yet, so they are not written yet.
//!
//! `Serialize` is required by Tauri: a command returning `Result<T, E>` needs
//! `E: Serialize` to reject the JavaScript promise with a payload. The
//! internally-tagged representation gives the frontend a `kind` discriminant,
//! matching how the TypeScript contract models its unions.

use serde::Serialize;

/// A system operation that could not be completed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SystemError {
    /// A Win32 call failed. `call` names the function so a bug report says
    /// which one; `code` is the raw `HRESULT`, which is unambiguous in a way
    /// that a re-worded message is not.
    ApiFailure {
        call: &'static str,
        code: u32,
        message: String,
    },
}

impl SystemError {
    pub fn api_failure(call: &'static str, code: u32, message: impl Into<String>) -> Self {
        Self::ApiFailure {
            call,
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiFailure {
                call,
                code,
                message,
            } => write!(f, "{call} failed (0x{code:08X}): {message}"),
        }
    }
}

impl std::error::Error for SystemError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_failure_serialises_with_a_kind_discriminant() {
        let e = SystemError::api_failure("CreateToolhelp32Snapshot", 0x8007_0005, "Access denied");
        let v = serde_json::to_value(&e).unwrap();

        assert_eq!(v["kind"], "apiFailure");
        assert_eq!(v["call"], "CreateToolhelp32Snapshot");
        assert_eq!(v["code"], 0x8007_0005u32);
    }

    #[test]
    fn display_includes_the_call_and_the_code() {
        let e = SystemError::api_failure("OpenProcess", 0x8007_0005, "Access denied");
        assert_eq!(
            e.to_string(),
            "OpenProcess failed (0x80070005): Access denied"
        );
    }
}
