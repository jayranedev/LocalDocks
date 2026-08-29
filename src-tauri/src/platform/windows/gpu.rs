//! GPU utilisation, memory and adapter identity.
//!
//! Three sources, joined on the adapter LUID:
//!
//! | Source | Gives | Cost |
//! |---|---|---|
//! | PDH `\GPU Engine(*)\Utilization Percentage` | per-process, per-engine utilisation | 0.50 ms, 599 instances |
//! | PDH `\GPU Adapter Memory(*)\Dedicated Usage` and `Shared Usage` | memory in use per adapter | 0.008 ms |
//! | DXGI `EnumAdapters1` | adapter name and installed memory | once, cached |
//!
//! # Why the counters
//!
//! They are what Task Manager reads, they need no elevation, and they are the
//! only route that covers every vendor — the development machine has both an
//! NVIDIA and an AMD adapter, so a vendor SDK would have meant shipping two.
//! The alternatives were rejected in docs/BACKEND.md: `D3DKMTQueryStatistics`
//! is undocumented gdi32 internals, and `IDXGIAdapter3::QueryVideoMemoryInfo`
//! reports the *calling process's* memory budget rather than the machine's,
//! which would have shown LocalDocks' own usage as the GPU's.
//!
//! The honest caveat, recorded rather than hidden: unlike `GetIfTable2` or
//! `IOCTL_DISK_PERFORMANCE`, this counter set has no reference page on Microsoft
//! Learn. It ships with Windows, it is stable, and Microsoft's own guidance
//! points at PDH for GPU usage — but it is a counter set rather than a
//! documented API contract, and it is absent on Windows before 10 1709, on
//! pre-WDDM-2.0 drivers, and in virtual machines with a basic display adapter.
//! Absence is handled as absence: `None`, never zero.
//!
//! # Instance names
//!
//! ```text
//! pid_8420_luid_0x00000000_0x00013C64_phys_0_eng_0_engtype_3D
//! luid_0x00000000_0x00013C64_phys_0
//! ```
//!
//! The LUID's high and low halves are the adapter key, and DXGI reports the
//! same LUID for the same card, which is what makes the join exact rather than
//! a match on the adapter's name.

use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG, DXGI_ADAPTER_FLAG_SOFTWARE,
};

use crate::logic::telemetry::{RawGpuAdapter, RawGpuEngine, RawGpuMemory};
use crate::platform::windows::pdh::{Counter, Sample};

/// The three open PDH queries, held across ticks.
///
/// Rate counters need two collections to produce a value, so these are opened
/// once and read each tick rather than opened per tick — which is also what
/// makes them cheap.
pub struct GpuCounters {
    engine: Option<Counter>,
    dedicated: Option<Counter>,
    shared: Option<Counter>,
    /// DXGI descriptions, read once. Adapters do not appear and disappear
    /// during a session in any way worth re-enumerating every second for.
    adapters: Vec<RawGpuAdapter>,
}

impl GpuCounters {
    /// Open what this machine has.
    ///
    /// Never fails: a machine with no GPU counters produces a struct whose
    /// every counter is `None`, and `read` then reports the whole metric as
    /// unavailable.
    pub fn open() -> Self {
        let counters = Self {
            engine: Counter::open(r"\GPU Engine(*)\Utilization Percentage"),
            dedicated: Counter::open(r"\GPU Adapter Memory(*)\Dedicated Usage"),
            shared: Counter::open(r"\GPU Adapter Memory(*)\Shared Usage"),
            adapters: adapters(),
        };
        if counters.engine.is_none() && counters.dedicated.is_none() {
            log::info!("this machine exposes no GPU performance counters");
        }
        counters
    }

    /// Everything needed to fold one tick's GPU rows.
    ///
    /// `None` when the machine has no counters at all — distinct from a machine
    /// whose GPU is genuinely idle, which returns rows full of zeroes.
    pub fn read(&self) -> Option<(Vec<RawGpuAdapter>, Vec<RawGpuEngine>, Vec<RawGpuMemory>)> {
        if self.engine.is_none() && self.dedicated.is_none() {
            return None;
        }

        let engines = self
            .engine
            .as_ref()
            .and_then(|c| c.read())
            .map(|samples| samples.iter().filter_map(parse_engine).collect::<Vec<_>>())
            .unwrap_or_default();

        let memory = self.read_memory();

        Some((self.adapters.clone(), engines, memory))
    }

    /// Dedicated and shared usage, paired by adapter.
    fn read_memory(&self) -> Vec<RawGpuMemory> {
        let dedicated = self.dedicated.as_ref().and_then(|c| c.read());
        let shared = self.shared.as_ref().and_then(|c| c.read());

        let mut rows: Vec<RawGpuMemory> = Vec::new();
        let mut push =
            |luid: u64, dedicated_bytes: Option<u64>, shared_bytes: Option<u64>| match rows
                .iter_mut()
                .find(|r| r.adapter == luid)
            {
                Some(existing) => {
                    if let Some(v) = dedicated_bytes {
                        existing.dedicated_used_bytes = v;
                    }
                    if let Some(v) = shared_bytes {
                        existing.shared_used_bytes = v;
                    }
                }
                None => rows.push(RawGpuMemory {
                    adapter: luid,
                    dedicated_used_bytes: dedicated_bytes.unwrap_or(0),
                    shared_used_bytes: shared_bytes.unwrap_or(0),
                }),
            };

        for s in dedicated.into_iter().flatten() {
            if let Some(luid) = parse_luid(&s.instance) {
                push(luid, Some(bytes(s.value)), None);
            }
        }
        for s in shared.into_iter().flatten() {
            if let Some(luid) = parse_luid(&s.instance) {
                push(luid, None, Some(bytes(s.value)));
            }
        }
        rows
    }
}

/// A counter value that is logically a byte count.
///
/// Negative and non-finite values are driver noise rather than measurements.
fn bytes(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value as u64
    } else {
        0
    }
}

/// `..._luid_0xHIGH_0xLOW_phys_N...` to one 64-bit key.
///
/// Both halves matter. The high half is zero on every adapter seen so far, but
/// it is part of the LUID and dropping it would collide two adapters on a
/// machine where it is not.
fn parse_luid(instance: &str) -> Option<u64> {
    let after = instance.split("luid_").nth(1)?;
    let mut parts = after.split('_');
    let high = parts.next()?.strip_prefix("0x")?;
    let low = parts.next()?.strip_prefix("0x")?;
    let high = u32::from_str_radix(high, 16).ok()?;
    let low = u32::from_str_radix(low, 16).ok()?;
    Some(((high as u64) << 32) | low as u64)
}

/// One engine instance to its adapter, engine type and utilisation.
///
/// The engine type is carried through because it is what makes summing wrong:
/// `logic::telemetry::fold_gpus` sums within an engine type and takes the
/// maximum across them.
fn parse_engine(sample: &Sample) -> Option<RawGpuEngine> {
    let adapter = parse_luid(&sample.instance)?;
    let engine_type = sample.instance.split("engtype_").nth(1)?.to_string();
    Some(RawGpuEngine {
        adapter,
        engine_type,
        utilization_percent: sample.value.max(0.0),
    })
}

/// Adapter identity, from DXGI.
///
/// Enumeration only: no device is created, nothing is rendered, and no
/// privilege is needed. Software adapters — the Microsoft Basic Render Driver
/// and WARP — are skipped, because they are not hardware and reporting a
/// utilisation for them would be meaningless.
fn adapters() -> Vec<RawGpuAdapter> {
    // SAFETY: CreateDXGIFactory1 initialises COM for DXGI itself and returns a
    // reference-counted interface managed by the windows crate.
    let factory: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
        Ok(f) => f,
        Err(e) => {
            log::debug!("CreateDXGIFactory1 failed: {e}");
            return Vec::new();
        }
    };

    let mut found = Vec::new();
    for index in 0..u32::MAX {
        // SAFETY: enumeration ends with DXGI_ERROR_NOT_FOUND, which breaks the
        // loop; the returned adapter is reference-counted.
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            break;
        };
        // SAFETY: the adapter came from a successful enumeration.
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        if DXGI_ADAPTER_FLAG(desc.Flags as i32) == DXGI_ADAPTER_FLAG_SOFTWARE {
            continue;
        }

        let end = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        found.push(RawGpuAdapter {
            luid: ((desc.AdapterLuid.HighPart as u32 as u64) << 32)
                | desc.AdapterLuid.LowPart as u64,
            name: String::from_utf16_lossy(&desc.Description[..end])
                .trim()
                .to_string(),
            dedicated_memory_bytes: desc.DedicatedVideoMemory as u64,
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_luid_parses_out_of_both_instance_shapes() {
        assert_eq!(
            parse_luid("pid_8420_luid_0x00000000_0x00013C64_phys_0_eng_0_engtype_3D"),
            Some(0x0000_0000_0001_3C64)
        );
        assert_eq!(
            parse_luid("luid_0x00000000_0x00017274_phys_0"),
            Some(0x0000_0000_0001_7274)
        );
        // The high half is part of the key, not decoration.
        assert_eq!(
            parse_luid("luid_0x0000000A_0x00000001_phys_0"),
            Some(0x0000_000A_0000_0001)
        );
        // Case-insensitive hex, as PDH has been observed emitting both.
        assert_eq!(
            parse_luid("luid_0x00000000_0x00013c64_phys_0"),
            parse_luid("luid_0x00000000_0x00013C64_phys_0")
        );
    }

    #[test]
    fn an_instance_without_a_luid_parses_to_nothing_rather_than_a_wrong_key() {
        for bad in [
            "",
            "_Total",
            "pid_8420_phys_0",
            "luid_0x00000000",
            "luid_notahexnumber_0x1_phys_0",
            "luid_0x00000000_0xZZZZ_phys_0",
        ] {
            assert!(parse_luid(bad).is_none(), "{bad} should not parse");
        }
    }

    #[test]
    fn an_engine_instance_carries_its_engine_type() {
        let e = parse_engine(&Sample {
            instance: "pid_8420_luid_0x00000000_0x00013C64_phys_0_eng_0_engtype_3D".into(),
            value: 42.5,
        })
        .unwrap();
        assert_eq!(e.adapter, 0x0000_0000_0001_3C64);
        assert_eq!(e.engine_type, "3D");
        assert_eq!(e.utilization_percent, 42.5);

        // Engine types with spaces survive intact — they are the grouping key.
        let e = parse_engine(&Sample {
            instance: "pid_1_luid_0x00000000_0x1_phys_0_eng_7_engtype_High Priority 3D".into(),
            value: 1.0,
        })
        .unwrap();
        assert_eq!(e.engine_type, "High Priority 3D");
    }

    #[test]
    fn a_negative_counter_value_is_floored_rather_than_carried() {
        let e = parse_engine(&Sample {
            instance: "pid_1_luid_0x0_0x1_phys_0_eng_0_engtype_3D".into(),
            value: -5.0,
        });
        assert_eq!(e.unwrap().utilization_percent, 0.0);
        assert_eq!(bytes(-1.0), 0);
        assert_eq!(bytes(f64::NAN), 0);
        assert_eq!(bytes(f64::INFINITY), 0);
        assert_eq!(bytes(1024.0), 1024);
    }

    /// The real machine. A GPU may legitimately be absent — a VM, a server —
    /// so this asserts the shape of whatever is there rather than a count.
    #[test]
    fn the_machine_reports_coherent_gpu_data_or_none_at_all() {
        let counters = GpuCounters::open();
        std::thread::sleep(std::time::Duration::from_millis(150));

        let Some((adapters, engines, memory)) = counters.read() else {
            // No GPU counters on this machine. That is a valid outcome and the
            // one the UI must render as unavailable.
            return;
        };

        for a in &adapters {
            assert!(!a.name.is_empty(), "an enumerated adapter must have a name");
            assert_ne!(a.luid, 0, "a real adapter has a non-zero LUID");
        }
        for e in &engines {
            assert!(e.utilization_percent >= 0.0);
            assert!(!e.engine_type.is_empty());
        }
        for m in &memory {
            assert_ne!(m.adapter, 0);
        }

        // Every adapter DXGI enumerated should also have memory counters. If
        // this ever fails, the LUID join is wrong and the UI would show two
        // half-populated cards instead of one whole one.
        for a in &adapters {
            assert!(
                memory.iter().any(|m| m.adapter == a.luid),
                "no memory counter matched adapter {} (luid {:#x}) — the join is broken",
                a.name,
                a.luid
            );
        }
    }
}
