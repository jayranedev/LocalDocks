//! Machine-wide telemetry: the three readings Windows exposes cheaply and
//! honestly to an unelevated process.
//!
//! # What is here, and why only this
//!
//! | Reading | Call | Cost |
//! |---|---|---|
//! | Total CPU | `GetSystemTimes` | one call, no handle |
//! | Per-logical-processor CPU | `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` | one call, one small buffer |
//! | Physical memory | `GlobalMemoryStatusEx` | one call, no handle |
//!
//! All three are counters or levels the kernel already maintains. None opens a
//! handle, none needs a privilege, and the whole set runs once per tick beside
//! a scan that already opens ~400 process handles — so it is not measurably
//! part of the tick's cost.
//!
//! # What is deliberately absent
//!
//! docs/BACKEND.md forbids fabricating a value to fill a slot, and each of
//! these would require exactly that, or a cost V1 has not accepted:
//!
//! * **Network throughput.** `GetIfTable2` gives per-adapter byte counters, so
//!   a machine-wide rate is reachable — but *per-process* network attribution,
//!   which is the number a developer would actually want beside a service, needs
//!   an ETW session (`Microsoft-Windows-Kernel-Network`), and starting one needs
//!   elevation. Showing a machine-wide rate on a per-service dashboard would
//!   invite it to be read as the service's. Deferred to V2 with a decision to
//!   make, not silently dropped.
//! * **Disk I/O.** `GetProcessIoCounters` is per-process and cheap, but it
//!   counts *all* I/O — file, network and device — so it cannot answer "how much
//!   is this touching the disk". Reporting it as disk activity would be wrong in
//!   a way the user could not detect.
//! * **GPU utilisation.** Only through vendor SDKs (NVML, ADL) or the
//!   performance-counter path Task Manager uses, neither of which is a Win32
//!   call and both of which are vendor-specific. A monitoring tool that shipped
//!   an NVIDIA dependency to read one number is not the V1 trade.
//! * **Temperature.** WMI's `MSAcpi_ThermalZoneTemperature` is unimplemented on
//!   most consumer hardware and returns a fixed or absent value where it exists.
//!   A field that is wrong on most machines is worse than no field.
//!
//! Every one of these is `None` in the contract rather than absent from it, and
//! docs/ROADMAP.md records them as DEFERRED with these reasons.

use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
// GetSystemTimes lives beside GetProcessTimes in the Threading namespace, not
// in SystemInformation where its documentation files it.
use windows::Win32::System::Threading::GetSystemTimes;

use crate::logic::telemetry::{CpuTimes, MemoryStatus};

/// `SystemProcessorPerformanceInformation`.
///
/// Not named by the `windows` crate. Answers with one record per logical
/// processor, in the kernel's own enumeration order.
const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION: SYSTEM_INFORMATION_CLASS =
    SYSTEM_INFORMATION_CLASS(8);

/// One record of `SystemProcessorPerformanceInformation`.
///
/// Declared here because the `windows` crate does not expose the struct for
/// this class. The layout is documented and stable; the trailing `Reserved1`
/// array is what the header calls it, and only the first three fields are read.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcessorPerformance {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
}

/// Machine-wide idle/kernel/user counters.
///
/// `None` rather than an error: telemetry is decoration on a process
/// dashboard, and a tick that failed to read the CPU must still publish its
/// processes. The UI renders "—".
pub fn cpu_times() -> Option<CpuTimes> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: three live FILETIMEs owned by this frame. The call needs no
    // handle and no privilege.
    if let Err(e) = unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) } {
        log::debug!("GetSystemTimes failed: {e}");
        return None;
    }

    Some(CpuTimes {
        idle_100ns: filetime_u64(idle),
        kernel_100ns: filetime_u64(kernel),
        user_100ns: filetime_u64(user),
    })
}

/// Per-logical-processor idle/kernel/user counters, in enumeration order.
///
/// `None` when the class is unavailable or answers with nothing, which is a
/// different fact from an empty machine and is kept distinct all the way to the
/// UI.
pub fn per_core_times() -> Option<Vec<CpuTimes>> {
    // Ask for the size first. The count is the machine's logical processor
    // count, which can change while running (parked or hot-added cores), so it
    // is read fresh rather than assumed.
    let mut needed: u32 = 0;
    // SAFETY: a null buffer of length 0 is the documented size probe; `needed`
    // is a live u32 owned by this frame.
    unsafe {
        let _ = NtQuerySystemInformation(
            SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }

    let record = std::mem::size_of::<ProcessorPerformance>();
    if needed == 0 || (needed as usize) < record {
        log::debug!("SystemProcessorPerformanceInformation reported {needed} bytes");
        return None;
    }

    let count = (needed as usize) / record;
    let mut buffer = vec![ProcessorPerformance::default(); count];
    let capacity = (count * record) as u32;

    // SAFETY: `buffer` is a live allocation of exactly `capacity` bytes, laid
    // out as the array of records this class writes.
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
            buffer.as_mut_ptr() as *mut _,
            capacity,
            &mut needed,
        )
    };
    if status.is_err() {
        log::debug!("SystemProcessorPerformanceInformation read failed: {status:?}");
        return None;
    }

    // Windows may answer with fewer records than the probe suggested.
    let written = (needed as usize) / record;
    if written == 0 {
        return None;
    }

    Some(
        buffer
            .iter()
            .take(written.min(count))
            .map(|p| CpuTimes {
                idle_100ns: p.idle_time.max(0) as u64,
                kernel_100ns: p.kernel_time.max(0) as u64,
                user_100ns: p.user_time.max(0) as u64,
            })
            .collect(),
    )
}

/// Physical memory: how much the machine has, and how much is usable.
pub fn memory() -> Option<MemoryStatus> {
    let mut status = MEMORYSTATUSEX {
        // Required: the call uses this to tell the struct versions apart, and
        // leaving it zero makes it fail.
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };

    // SAFETY: `status` is a live, correctly sized MEMORYSTATUSEX owned by this
    // frame, with dwLength set as the call requires.
    if let Err(e) = unsafe { GlobalMemoryStatusEx(&mut status) } {
        log::debug!("GlobalMemoryStatusEx failed: {e}");
        return None;
    }

    if status.ullTotalPhys == 0 {
        return None;
    }

    Some(MemoryStatus {
        total_bytes: status.ullTotalPhys,
        available_bytes: status.ullAvailPhys,
    })
}

/// A FILETIME's two halves as one 64-bit count of 100 ns units.
fn filetime_u64(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | (t.dwLowDateTime as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_machine_reports_cpu_counters_that_move_forward() {
        let first = cpu_times().expect("GetSystemTimes is available on every Windows build");
        // Busy-wait rather than sleep: the counters advance with processor
        // time, and a sleeping thread contributes none of it.
        let mut spin = 0u64;
        for i in 0..40_000_000u64 {
            spin = spin.wrapping_add(i);
        }
        std::hint::black_box(spin);

        let second = cpu_times().unwrap();
        assert!(
            second.kernel_100ns >= first.kernel_100ns,
            "kernel time went backwards"
        );
        assert!(
            second.user_100ns >= first.user_100ns,
            "user time went backwards"
        );
        assert!(
            second.kernel_100ns + second.user_100ns > first.kernel_100ns + first.user_100ns,
            "no processor time elapsed across a busy loop"
        );
    }

    #[test]
    fn per_core_counters_match_the_machine_s_processor_count() {
        let cores = per_core_times().expect("per-processor performance information is available");
        let expected = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        assert_eq!(cores.len(), expected, "one record per logical processor");
        assert!(
            cores.iter().all(|c| c.kernel_100ns > 0),
            "a live core has kernel time"
        );
    }

    /// The invariant the percentage maths depends on.
    #[test]
    fn kernel_time_includes_idle_time_on_every_core() {
        for (i, core) in per_core_times().unwrap().iter().enumerate() {
            assert!(
                core.kernel_100ns >= core.idle_100ns,
                "core {i}: idle {} exceeds kernel {}",
                core.idle_100ns,
                core.kernel_100ns
            );
        }
    }

    #[test]
    fn per_core_totals_are_consistent_with_the_machine_wide_reading() {
        let total = cpu_times().unwrap();
        let cores = per_core_times().unwrap();
        let summed: u64 = cores.iter().map(|c| c.kernel_100ns + c.user_100ns).sum();
        let machine = total.kernel_100ns + total.user_100ns;
        // The two are sampled a moment apart, so they cannot be equal; they
        // must agree to well within one percent.
        let drift = (summed as f64 - machine as f64).abs() / machine as f64;
        assert!(drift < 0.01, "per-core sum {summed} vs machine {machine}");
    }

    #[test]
    fn memory_reports_a_plausible_machine() {
        let m = memory().expect("GlobalMemoryStatusEx is available on every Windows build");
        assert!(
            m.total_bytes > 512 * 1024 * 1024,
            "implausible total: {}",
            m.total_bytes
        );
        assert!(
            m.available_bytes <= m.total_bytes,
            "available exceeds total"
        );
        assert!(m.available_bytes > 0, "no memory available at all");
        let percent = m.percent().unwrap();
        assert!((0.0..=100.0).contains(&percent));
    }

    #[test]
    fn a_filetime_reassembles_both_halves() {
        let t = FILETIME {
            dwLowDateTime: 0x8000_0001,
            dwHighDateTime: 0x0000_0002,
        };
        assert_eq!(filetime_u64(t), 0x0000_0002_8000_0001);
    }
}
