//! The platform layer.
//!
//! Every `use windows::...` in this crate lives below this module and nowhere
//! else. `logic` and `models` stay syscall-free, which is what makes them
//! testable on any machine — including the CI runner and the container this
//! was written in.
//!
//! There is deliberately no `ProcessSource` trait and no cross-platform
//! abstraction. LocalDocks is a Windows application; an abstraction over one
//! implementation buys nothing and hides where the syscalls are.

#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
compile_error!(
    "LocalDocks targets Windows. Process discovery is implemented directly \
     against the Win32 API by design (docs/ARCHITECTURE.md), and there is no \
     second backend. Build with a Windows target."
);
