//! LocalDocks core.
//!
//! Wiring only. The layers live beside this file and are described in
//! docs/ARCHITECTURE.md:
//!
//! ```text
//! commands/   thin IPC handlers
//! sampler     cadence, state, orchestration
//! logic/      pure, syscall-free, unit-tested
//! platform/   every `use windows::...`, behind #[cfg(windows)]
//! models      serde types shared with TypeScript
//! errors      the failure taxonomy
//! ```

mod commands;
mod errors;
mod logic;
mod models;
mod platform;
mod sampler;
mod time;

use std::time::Duration;

use tauri::{Emitter, Manager};

use sampler::{Sampler, SamplerEvent};

/// Cadence the sampler starts at, before the frontend states a preference.
///
/// Matches `DEFAULT_INTERVAL_MS` in `src/lib/ipc.ts`, so the first few ticks
/// before `set_sample_interval` arrives run at the rate the UI expects.
const DEFAULT_INTERVAL_MS: u64 = 1000;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // The sampler is managed state so commands can reach it, and is
            // started here rather than lazily on first subscribe: the first
            // scan should already be done by the time the webview asks.
            app.manage(Sampler::new(
                sampler::logical_cores(),
                Duration::from_millis(DEFAULT_INTERVAL_MS),
            ));

            let emitter = app.handle().clone();
            app.state::<Sampler>().start(move |event| match event {
                SamplerEvent::Update(snapshot) => {
                    if let Err(e) = emitter.emit("services:update", snapshot) {
                        log::error!("could not emit services:update: {e}");
                    }
                }
                // The contract types this payload as a string
                // (docs/ARCHITECTURE.md, and `listen<string>` in ipc.ts), so
                // the error is rendered rather than sent as a struct.
                SamplerEvent::Failure(message) => {
                    if let Err(e) = emitter.emit("services:error", message) {
                        log::error!("could not emit services:error: {e}");
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::set_sample_interval,
            commands::get_process_detail,
            commands::terminate_process,
            commands::open_external
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // `build` + `run(callback)` rather than `run(context)` purely so there is
    // somewhere to stop the sampler. Without this the process would exit with
    // the sampling thread still mid-scan.
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            handle.state::<Sampler>().stop();
        }
    });
}
