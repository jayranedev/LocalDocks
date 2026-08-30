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

/// How the app logs, which differs between a debug build and a shipped one.
///
/// # Release
///
/// Warnings and errors, to a rotating file in the OS application-log directory
/// — `%LOCALAPPDATA%\\com.silentminds.localdocks\\logs` on Windows. Nothing is
/// sent anywhere: LocalDocks has no network client, no crash reporter and no
/// analytics, and this file is the only thing it ever writes outside its
/// settings.
///
/// It exists because the alternative was worse. A shipped app whose sampler
/// stops updating leaves the user with nothing to attach to a bug report and
/// the maintainer with nothing to read, and "it just stopped" is not a
/// reproducible defect.
///
/// **Warn and above, deliberately.** Info would record every interval change
/// and every sampler start, which is noise on disk for no diagnostic value.
/// Below that, `debug!` carries PIDs and API failure detail that is useful at a
/// terminal and has no business persisting on a user's machine.
///
/// What can appear in the file: process executable names, PIDs, port numbers,
/// and Windows error text. What cannot: command lines, file paths, working
/// directories, or anything the user typed. Those are read only for the detail
/// panel and the classifier, and neither logs at warn or above.
///
/// # Debug
///
/// Info and above to stdout and to the webview console, which is what makes a
/// `tauri dev` session readable.
fn logging() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    let builder = tauri_plugin_log::Builder::default();

    if cfg!(debug_assertions) {
        return builder
            .level(log::LevelFilter::Info)
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Stdout,
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Webview,
            ))
            .build();
    }

    builder
        .level(log::LevelFilter::Warn)
        .target(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::LogDir { file_name: None },
        ))
        // One rotation rather than an unbounded directory: a log that grows
        // without limit on a machine the user never looks at is a defect, not
        // a diagnostic.
        .max_file_size(512 * 1024)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            app.handle().plugin(logging())?;

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
        // The only panic on this path, and it is the right one: if the
        // webview runtime or the generated context cannot be built there is no
        // application to degrade into. It happens before any window appears,
        // so there is nothing to lose and nothing to report to.
        .expect("LocalDocks could not start: the Tauri application failed to build");

    // `build` + `run(callback)` rather than `run(context)` purely so there is
    // somewhere to stop the sampler. Without this the process would exit with
    // the sampling thread still mid-scan.
    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            handle.state::<Sampler>().stop();
        }
    });
}
