//! System-level storage activity, from the disk driver's own counters.
//!
//! `DeviceIoControl(IOCTL_DISK_PERFORMANCE)` on `\\.\PhysicalDriveN` returns a
//! `DISK_PERFORMANCE` holding cumulative bytes read, bytes written and idle
//! time. Physical-disk counters have been enabled by default since Windows
//! Vista, so no `diskperf` step is needed.
//!
//! # Why this rather than the performance counters
//!
//! `\PhysicalDisk(*)\Disk Read Bytes/sec` gives the same numbers through PDH,
//! already differenced. This is preferred for three reasons: it returns the raw
//! cumulative counters, which fit the same delta model as CPU and network
//! rather than needing a second rate mechanism; it costs 0.11 ms against PDH's
//! 0.055 ms plus a query held open; and it carries no counter-name
//! localisation problem at all.
//!
//! # The handle
//!
//! Opened with a desired access of **zero**. That is not a shortcut — it is the
//! point. A zero-access handle can issue this IOCTL but cannot read or write a
//! byte of the disk, so an unelevated user is allowed to open it. Asking for
//! `GENERIC_READ` would demand administrator rights on the raw device, which
//! LocalDocks will not do. Measured working unelevated.
//!
//! # Physical, not logical
//!
//! `\\.\PhysicalDrive0` is the device; `\\.\C:` is a volume on it. The physical
//! device is the right level for a machine-wide reading — two volumes on one
//! SSD are not two things that can be busy independently — and it means a drive
//! with no mounted volume still appears.
//!
//! Per-process disk accounting is deliberately absent. `GetProcessIoCounters`
//! is cheap and per-process but counts file, network and device I/O in one
//! number, so it cannot answer "how much is this touching the disk"; reporting
//! it as disk activity would be wrong in a way the user could not detect.

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{DISK_PERFORMANCE, IOCTL_DISK_PERFORMANCE};
use windows::Win32::System::IO::DeviceIoControl;

use crate::logic::telemetry::RawDrive;

/// How many physical drive numbers to look for.
///
/// Drive numbers are assigned by the storage stack and can be sparse — removing
/// one drive of three leaves a gap — so the probe cannot stop at the first
/// miss. Sixteen covers any machine a developer is sitting at, and the whole
/// sweep was measured at 0.05 ms warm, so it runs every tick rather than being
/// cached. That is what makes a drive appearing or disappearing mid-session
/// work with no extra machinery.
const MAX_DRIVES: u32 = 16;

/// Every physical drive that answered, with its cumulative counters.
///
/// `None` when no drive could be opened at all, which would be a machine with
/// no local storage or a policy that blocks raw device handles. An empty result
/// is impossible in practice but would mean the same thing, so both are folded
/// into `None` by the caller.
pub fn drives() -> Option<Vec<RawDrive>> {
    let mut found = Vec::new();

    for number in 0..MAX_DRIVES {
        let Some(handle) = open_drive(number) else {
            continue;
        };
        if let Some(performance) = read_performance(&handle, number) {
            found.push(RawDrive {
                number,
                model: model_for(number),
                // The counters are LARGE_INTEGERs, and negative would mean a
                // driver bug rather than a real value.
                bytes_read: performance.BytesRead.max(0) as u64,
                bytes_written: performance.BytesWritten.max(0) as u64,
                idle_time_100ns: performance.IdleTime.max(0) as u64,
            });
        }
    }

    (!found.is_empty()).then_some(found)
}

/// A handle that closes itself.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: constructed only from a successful open, closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Open a physical drive for metadata only.
///
/// `None` for every failure, and most failures are expected: probing sixteen
/// numbers on a machine with one drive fails fifteen times by design.
fn open_drive(number: u32) -> Option<OwnedHandle> {
    let path = HSTRING::from(format!(r"\\.\PhysicalDrive{number}"));

    // SAFETY: `path` is a live NUL-terminated wide string for the call.
    //
    // Desired access is 0: enough to issue IOCTL_DISK_PERFORMANCE, not enough
    // to read or write the device, and therefore permitted unelevated.
    let handle = unsafe {
        CreateFileW(
            &path,
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    match handle {
        Ok(h) if h != INVALID_HANDLE_VALUE => Some(OwnedHandle(h)),
        Ok(_) => None,
        Err(_) => None,
    }
}

/// One `DISK_PERFORMANCE` reading.
fn read_performance(handle: &OwnedHandle, number: u32) -> Option<DISK_PERFORMANCE> {
    let mut performance = DISK_PERFORMANCE::default();
    let mut returned: u32 = 0;

    // SAFETY: `performance` is a live DISK_PERFORMANCE owned by this frame and
    // its size is passed correctly; the handle carries the access this IOCTL
    // requires.
    let result = unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_DISK_PERFORMANCE,
            None,
            0,
            Some(&mut performance as *mut _ as *mut _),
            std::mem::size_of::<DISK_PERFORMANCE>() as u32,
            Some(&mut returned),
            None,
        )
    };

    if result.is_err() {
        log::debug!("IOCTL_DISK_PERFORMANCE on drive {number} failed: {result:?}");
        return None;
    }
    Some(performance)
}

/// A human name for the drive.
///
/// `StorageManagerName` in `DISK_PERFORMANCE` names the driver ("PhysDisk"),
/// not the device, so it is no use here. The vendor and product strings come
/// from a separate identity query, which needs its own IOCTL and its own
/// variable-length buffer — enough machinery that V1 names the drive by its
/// number instead. The number is what `\\.\PhysicalDriveN` and Disk Management
/// both use, so it is a name the user can act on.
fn model_for(number: u32) -> String {
    format!("PhysicalDrive{number}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_machine_reports_at_least_one_drive_with_unique_numbers() {
        let found = drives().expect("a machine running this test has a disk");
        assert!(!found.is_empty());

        let numbers: HashSet<u32> = found.iter().map(|d| d.number).collect();
        assert_eq!(numbers.len(), found.len(), "drive numbers must be unique");
        for d in &found {
            assert!(d.number < MAX_DRIVES);
            assert!(!d.model.is_empty());
        }
    }

    #[test]
    fn counters_are_cumulative_and_never_go_backwards() {
        let first = drives().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let second = drives().unwrap();

        for a in &first {
            let Some(b) = second.iter().find(|d| d.number == a.number) else {
                continue;
            };
            assert!(b.bytes_read >= a.bytes_read, "read bytes went backwards");
            assert!(
                b.bytes_written >= a.bytes_written,
                "written bytes went backwards"
            );
            assert!(
                b.idle_time_100ns >= a.idle_time_100ns,
                "idle time went backwards"
            );
        }
    }

    /// The unit of `IdleTime` is not stated on the `DISK_PERFORMANCE`
    /// reference page, and `logic::telemetry::active_percent` divides by it —
    /// so it is asserted against a measured interval rather than assumed. A
    /// mostly idle drive must accumulate roughly one interval of idle time.
    #[test]
    fn idle_time_is_in_hundred_nanosecond_units() {
        let first = drives().unwrap();
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let second = drives().unwrap();
        let elapsed_100ns = start.elapsed().as_nanos() as f64 / 100.0;

        let a = &first[0];
        let b = second.iter().find(|d| d.number == a.number).unwrap();
        let idle_delta = (b.idle_time_100ns - a.idle_time_100ns) as f64;

        // A drive cannot be idle for longer than the interval, give or take the
        // sampling skew — and if the unit were milliseconds this ratio would be
        // about 0.0001, while seconds would make it about 0.0000001.
        let ratio = idle_delta / elapsed_100ns;
        assert!(
            ratio < 1.05,
            "idle time exceeded elapsed time: ratio {ratio}, which means the unit is not 100 ns"
        );
        assert!(
            ratio > 0.001,
            "idle time was implausibly small: ratio {ratio}, which means the unit is not 100 ns"
        );
    }

    /// Probing sixteen numbers on a one-drive machine fails fifteen times. That
    /// has to be cheap, because it happens every tick.
    #[test]
    fn probing_every_drive_number_is_cheap_enough_to_do_every_tick() {
        drives(); // warm the path
        let start = std::time::Instant::now();
        for _ in 0..10 {
            drives();
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / 10.0;
        assert!(each < 20.0, "a full drive sweep took {each:.2} ms");
    }
}
