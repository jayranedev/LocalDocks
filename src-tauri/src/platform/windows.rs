//! Win32-backed system access.
//!
//! Every `use windows::...` in the codebase lives under this module. The
//! providers below split by what they read rather than by which API they use,
//! so a metric's cost, its failure mode and its documentation sit together.

pub mod control;
pub mod gpu;
pub mod network;
pub mod pdh;
pub mod ports;
pub mod process;
pub mod storage;
pub mod system;
