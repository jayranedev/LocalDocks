//! A minimal wrapper over the Performance Data Helper.
//!
//! Two metrics need PDH — GPU and thermal — and neither has a Win32 function
//! that answers directly. Rather than open a query in each, both share this.
//!
//! # Why PDH at all
//!
//! For GPU it is the only unelevated, non-vendor route. The alternatives were
//! `D3DKMTQueryStatistics`, which is undocumented gdi32 internals;
//! `IDXGIAdapter3::QueryVideoMemoryInfo`, which reports the *calling process's*
//! budget rather than the machine's; and the NVIDIA and AMD SDKs, which are
//! vendor-specific and would need both to cover a laptop with two adapters.
//! These counters are what Task Manager itself reads.
//!
//! For thermal it is the only unelevated route at all: the WMI class
//! `MSAcpi_ThermalZoneTemperature` was measured returning **access denied**
//! unelevated on the development machine.
//!
//! # Two rules that are easy to get wrong
//!
//! **English counter names.** `PdhAddCounterW` takes *localised* names, so
//! `\\GPU Engine(*)\\Utilization Percentage` fails on a German or Japanese
//! Windows. `PdhAddEnglishCounterW` takes the English name on every locale and
//! is the only correct choice for a hard-coded path.
//!
//! **A query is a long-lived object.** Rate counters need two collections to
//! produce a value, so a query opened and closed inside one tick would report
//! nothing forever. The query is opened once and `PdhCollectQueryData` is
//! called each tick, which is also what makes it cheap: measured at 0.50 ms for
//! the 599-instance GPU engine counter and 0.09 ms for thermal zones.
//!
//! # Absence is not failure
//!
//! A machine with no WDDM 2.0 adapter has no `GPU Engine` object at all, and
//! most desktops expose no ACPI thermal zones. `PdhAddEnglishCounterW` then
//! returns `PDH_CSTATUS_NO_OBJECT`, which is a fact about the machine rather
//! than an error: the counter simply never becomes available, the provider
//! reports `None`, and the UI says so.

use windows::core::HSTRING;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

/// `PDH_FMT_NOCAP100` — do not clamp a value at 100.
///
/// Not exposed as a constant by the `windows` crate. It matters for the memory
/// counters, which are byte counts: a counter PDH believed was a percentage
/// would otherwise be silently capped, and 1.7 GB would arrive as 100.
const NOCAP100: u32 = 0x0000_8000;

/// The format every counter here is read in.
fn format() -> PDH_FMT {
    PDH_FMT(PDH_FMT_DOUBLE.0 | NOCAP100)
}

/// `PDH_MORE_DATA` — the size probe succeeded and is asking for a buffer.
const PDH_MORE_DATA: u32 = 0x800007D2;
/// `PDH_CSTATUS_NO_OBJECT` — this machine does not have that counter object.
const PDH_CSTATUS_NO_OBJECT: u32 = 0xC0000BB8;
/// `PDH_CSTATUS_NO_COUNTER` — the object exists but not this counter.
const PDH_CSTATUS_NO_COUNTER: u32 = 0xC0000BB9;
/// `PDH_NO_DATA` — the counter exists but has no instances right now.
const PDH_NO_DATA: u32 = 0x800007D5;
/// `PDH_INVALID_DATA` — a rate counter without two samples yet.
const PDH_INVALID_DATA: u32 = 0xC0000BC6;

/// One counter instance and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// The PDH instance name, e.g.
    /// `pid_8420_luid_0x00000000_0x00013c64_phys_0_eng_0_engtype_3D`.
    pub instance: String,
    pub value: f64,
}

/// One open PDH query holding one wildcard counter.
///
/// One counter per query rather than several, because a query that fails to add
/// its counter is simply not created — which keeps "this machine has no GPU
/// counters" from disabling the thermal counter as a side effect.
pub struct Counter {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

// The handles are opaque values owned by this struct and used only from the
// sampler thread; PDH itself is thread-safe for distinct queries.
unsafe impl Send for Counter {}

impl Counter {
    /// Open a query for one wildcard counter path.
    ///
    /// `None` when this machine does not expose the object — the common,
    /// expected case for GPU on a VM and for thermal on most desktops.
    pub fn open(path: &str) -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: a null data source means the local machine; `query` is a live
        // handle slot owned by this frame.
        let status = unsafe { PdhOpenQueryW(None, 0, &mut query) };
        if status != 0 {
            log::debug!("PdhOpenQueryW failed for {path}: {status:#010X}");
            return None;
        }

        let wide = HSTRING::from(path);
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY: `wide` is a live NUL-terminated wide string for the duration
        // of the call and `counter` is a live handle slot.
        //
        // English rather than localised: see the module docs.
        let status = unsafe { PdhAddEnglishCounterW(query, &wide, 0, &mut counter) };
        if status != 0 {
            if status == PDH_CSTATUS_NO_OBJECT || status == PDH_CSTATUS_NO_COUNTER {
                log::info!("{path} is not available on this machine");
            } else {
                log::debug!("PdhAddEnglishCounterW failed for {path}: {status:#010X}");
            }
            // SAFETY: `query` came from a successful open and is closed once.
            unsafe {
                let _ = PdhCloseQuery(query);
            }
            return None;
        }

        // The first collection primes the counter. A rate counter has no value
        // until the second, which is why the query outlives the tick.
        // SAFETY: `query` is live and owns `counter`.
        unsafe {
            let _ = PdhCollectQueryData(query);
        }

        Some(Self { query, counter })
    }

    /// Collect one sample of every instance.
    ///
    /// `None` distinguishes "could not read" from "read, and there is nothing",
    /// which is the difference between an unavailable GPU and an idle one.
    pub fn read(&self) -> Option<Vec<Sample>> {
        // SAFETY: `self.query` is live for the lifetime of this struct.
        let status = unsafe { PdhCollectQueryData(self.query) };
        if status != 0 {
            // A rate counter that has not yet seen two samples reports this
            // once, on the first tick, and then works.
            if status != PDH_INVALID_DATA && status != PDH_NO_DATA {
                log::debug!("PdhCollectQueryData failed: {status:#010X}");
            }
            return None;
        }

        // Ask for the size first. Instance counts move — the GPU engine
        // counter has one instance per process per engine, measured at 599 on
        // the development machine, and every process start changes it.
        let mut size: u32 = 0;
        let mut count: u32 = 0;
        // SAFETY: a null item buffer with size 0 is the documented size probe.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(self.counter, format(), &mut size, &mut count, None)
        };
        if status != PDH_MORE_DATA || size == 0 {
            if status != PDH_NO_DATA && status != PDH_INVALID_DATA {
                log::debug!("PDH size probe returned {status:#010X}");
            }
            return None;
        }

        // A Vec<u64> rather than a Vec<u8>: the item array is a struct
        // containing a pointer and a double, and reading it out of a buffer
        // that happened to be byte-aligned would be undefined behaviour.
        let words = (size as usize).div_ceil(8) + 1;
        let mut buffer = vec![0u64; words];
        let mut size = (words * 8) as u32;

        // SAFETY: `buffer` is a live, correctly aligned allocation of at least
        // the size PDH just asked for.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                self.counter,
                format(),
                &mut size,
                &mut count,
                Some(buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W),
            )
        };
        if status != 0 {
            log::debug!("PdhGetFormattedCounterArrayW failed: {status:#010X}");
            return None;
        }

        // SAFETY: on success PDH has written `count` items at the start of the
        // buffer, each with a name pointer into the tail of that same
        // allocation, which outlives this borrow.
        let items = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
                count as usize,
            )
        };

        let mut samples = Vec::with_capacity(count as usize);
        for item in items {
            if item.szName.is_null() {
                continue;
            }
            // An individual instance can fail while its siblings succeed — a
            // process exiting mid-collection is the usual cause. Skipping it is
            // correct; treating it as zero would drag an average down.
            if item.FmtValue.CStatus != 0 {
                continue;
            }
            // SAFETY: a non-null name pointer from a successful call points at
            // a NUL-terminated wide string inside `buffer`.
            let name = unsafe { item.szName.to_string() };
            let Ok(instance) = name else { continue };
            // SAFETY: the value union holds a double because PDH_FMT_DOUBLE
            // was requested and CStatus reported success.
            let value = unsafe { item.FmtValue.Anonymous.doubleValue };
            if !value.is_finite() {
                continue;
            }
            samples.push(Sample { instance, value });
        }

        Some(samples)
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful open and is closed once.
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A counter every Windows machine has, used to prove the wrapper works
    /// before asking it about hardware that may not exist.
    #[test]
    fn a_universal_counter_opens_and_reads_instances() {
        let counter = Counter::open(r"\Processor(*)\% Processor Time")
            .expect("every Windows machine has processor counters");
        std::thread::sleep(std::time::Duration::from_millis(120));
        let samples = counter.read().expect("a second collection produces values");

        assert!(!samples.is_empty(), "at least _Total must be present");
        assert!(samples.iter().any(|s| s.instance == "_Total"));
        assert!(
            samples
                .iter()
                .all(|s| s.value.is_finite() && s.value >= 0.0),
            "every sample must be a real number"
        );
    }

    /// The case that must not be an error: a counter this machine does not
    /// have. Every optional provider depends on this returning None rather
    /// than panicking or reporting zero.
    #[test]
    fn a_counter_that_does_not_exist_is_absent_rather_than_an_error() {
        assert!(Counter::open(r"\LocalDocks Nonexistent Object(*)\Nothing").is_none());
        assert!(Counter::open(r"\Processor(*)\LocalDocks Nonexistent Counter").is_none());
        assert!(Counter::open("not a counter path at all").is_none());
        assert!(Counter::open("").is_none());
    }

    /// Rate counters need two collections, and the first tick must not be
    /// mistaken for a machine at zero.
    #[test]
    fn the_first_read_may_report_nothing_and_later_reads_succeed() {
        let counter = Counter::open(r"\Processor(_Total)\% Processor Time").unwrap();
        // Whatever the first read does, the query must recover.
        let _ = counter.read();
        std::thread::sleep(std::time::Duration::from_millis(120));
        assert!(counter.read().is_some(), "the query must work once primed");
    }
}
