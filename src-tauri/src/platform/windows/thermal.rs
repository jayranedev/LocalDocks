//! ACPI thermal zones.
//!
//! **What this is not: a CPU or GPU temperature.** The distinction runs through
//! the whole implementation, so it is worth stating before the code.
//!
//! # What Windows actually offers unelevated
//!
//! | Route | Verdict |
//! |---|---|
//! | WMI `MSAcpi_ThermalZoneTemperature` | **Measured: access denied unelevated** on the development machine |
//! | PDH `\Thermal Zone Information(*)\Temperature` | Works unelevated, 0.09 ms — used here |
//! | LibreHardwareMonitor, WinRing0, vendor SDKs | Kernel driver and administrator rights |
//! | Intel Power Gadget, NVML | Vendor SDKs, and this laptop has two vendors |
//!
//! So there is exactly one honest option, and it reports ACPI zones.
//!
//! # Why that is a weaker thing than it sounds
//!
//! A zone is whatever the platform firmware chose to expose under `\_TZ`. Its
//! name is the OEM's, its mapping to a physical component is unspecified, and
//! nothing requires a machine to expose any. The development machine exposes
//! three: `\_TZ.TSZ0` at 331 K, `\_TZ.TSZ2` at 293 K, and `\_TZ.TZ01` at
//! **0 K** — absolute zero, which is a stub rather than a reading.
//!
//! That last zone is the reason for the plausibility filter in
//! `logic::telemetry::map_thermal_zones`. A stub is reported as a zone with no
//! reading: not dropped, because the firmware says the zone exists, and not
//! converted, because −273 °C is not a temperature a laptop has.
//!
//! Package temperature — the number people mean by "CPU temp" — stays deferred.
//! Reaching it requires reading model-specific registers, which requires a
//! kernel driver, which LocalDocks does not ship and would not ship for one
//! readout.

use crate::platform::windows::pdh::Counter;

/// The open thermal query, held across ticks like the GPU counters.
pub struct ThermalCounter {
    temperature: Option<Counter>,
}

impl ThermalCounter {
    /// Open the counter if this machine has one.
    ///
    /// Most desktops do not, which is not an error and is logged as
    /// information rather than a failure.
    pub fn open() -> Self {
        let temperature = Counter::open(r"\Thermal Zone Information(*)\Temperature");
        if temperature.is_none() {
            log::info!("this machine exposes no ACPI thermal zones");
        }
        Self { temperature }
    }

    /// Each zone's name and its raw reading in Kelvin.
    ///
    /// Kelvin is passed through unconverted so that the plausibility check
    /// happens in one place, in pure code, where it is tested. Converting here
    /// and filtering there would mean a zone reporting 0 K arrived as
    /// −273.15 °C and had to be recognised by a magic number.
    pub fn read(&self) -> Option<Vec<(String, f64)>> {
        let samples = self.temperature.as_ref()?.read()?;
        Some(samples.into_iter().map(|s| (s.instance, s.value)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::telemetry::map_thermal_zones;

    /// A machine may legitimately expose no zones, so this asserts the shape
    /// of whatever is there.
    #[test]
    fn zones_are_named_and_their_readings_survive_the_plausibility_filter() {
        let counter = ThermalCounter::open();
        let Some(readings) = counter.read() else {
            return; // no ACPI zones on this machine
        };

        for (name, kelvin) in &readings {
            assert!(!name.is_empty(), "a zone must have a name");
            assert!(kelvin.is_finite(), "{name} reported a non-finite value");
        }

        let mapped = map_thermal_zones(&readings);
        assert_eq!(mapped.zones.len(), readings.len(), "no zone may be dropped");

        for zone in &mapped.zones {
            if let Some(c) = zone.celsius {
                assert!(
                    (0.0..=125.0).contains(&c),
                    "{} reported {c} °C, which the filter should have rejected",
                    zone.name
                );
            }
        }
    }

    /// The counter is read repeatedly, so it must not degrade.
    #[test]
    fn repeated_reads_keep_working() {
        let counter = ThermalCounter::open();
        let first = counter.read();
        std::thread::sleep(std::time::Duration::from_millis(120));
        let second = counter.read();
        assert_eq!(
            first.is_some(),
            second.is_some(),
            "availability must not change between reads"
        );
    }
}
