//! Whether this process is running from an MSIX package.
//!
//! One question, one syscall, and it decides who owns updates.
//!
//! LocalDocks ships through two channels that update in incompatible ways:
//!
//!   * **GitHub** — an NSIS installer. The app checks a release feed and can
//!     replace itself.
//!   * **Microsoft Store** — an MSIX. The Store owns updates entirely. A
//!     packaged app that downloads and runs its own installer is against Store
//!     policy, and it would not work anyway: an MSIX install is immutable, and
//!     the NSIS installer it would fetch has nothing there to update.
//!
//! Rather than maintain two build configurations that can drift apart, one
//! binary asks Windows which one it is. `GetCurrentPackageFullName` answers
//! `APPMODEL_ERROR_NO_PACKAGE` for an ordinary process and succeeds — or asks
//! for a bigger buffer — for one running with package identity.
//!
//! Deliberately cheap and deliberately not cached at module level: it is
//! called once at startup, and a `OnceLock` here would buy nothing but a
//! lifetime.

#[cfg(windows)]
pub fn is_packaged() -> bool {
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

    // Query the length only. Passing a null name buffer with a zero length is
    // the documented way to ask "how big?", and it is all we need: we care
    // which error comes back, not what the package is called.
    let mut length: u32 = 0;
    let result = unsafe { GetCurrentPackageFullName(&mut length, None) };

    // 15700 (APPMODEL_ERROR_NO_PACKAGE) means no package identity: an ordinary
    // desktop process, which is the GitHub install. Anything else — success,
    // or ERROR_INSUFFICIENT_BUFFER because we asked with no buffer — means the
    // process has package identity.
    //
    // The constant is named rather than compared as a bare integer, and any
    // *unexpected* error is treated as "not packaged": the failure that keeps
    // the updater working is far less bad than one that silently disables it
    // for every GitHub user.
    const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;
    result.0 != APPMODEL_ERROR_NO_PACKAGE
}

#[cfg(not(windows))]
pub fn is_packaged() -> bool {
    false
}
