//! Pure decision-making.
//!
//! Nothing below this module makes a system call. Everything here takes data
//! the platform layer already gathered and turns it into the shapes the IPC
//! contract defines — which is exactly the code worth unit-testing, and exactly
//! the code that must never grow a `use windows::...`.

pub mod classify;
pub mod cpu;
pub mod identity;
pub mod ports;
pub mod process;
pub mod registry;
pub mod release;
pub mod service;
pub mod telemetry;
pub mod url;
