//! The update channel.
//!
//! LocalDocks distributes through two channels and only one of them is ours to
//! update. See `platform::windows::packaging`: an MSIX install leaves updates
//! entirely to the Microsoft Store, and everything in this module short-circuits
//! for it.
//!
//! For the GitHub channel the mechanism is `tauri-plugin-updater`, which is the
//! officially supported updater for this Tauri version. That choice is
//! deliberate and is the security-relevant one: the plugin verifies every
//! downloaded artifact against a minisign public key compiled into this binary
//! **before** it runs anything. A hand-rolled downloader would have to
//! reimplement that, and an update channel that gets signature verification
//! subtly wrong is a remote code execution channel with a progress bar.
//!
//! # The feed
//!
//! One endpoint, hard-coded in `tauri.conf.json`:
//!
//! ```text
//! https://github.com/jayranedev/LocalDocks/releases/latest/download/latest.json
//! ```
//!
//! `/releases/latest/` is not a synonym for "most recent". GitHub resolves it
//! to the newest release that is **neither a draft nor a prerelease**, which is
//! precisely the stable channel this product wants — enforced by GitHub rather
//! than by a filter of ours that could be wrong. `logic::release` then applies
//! the same rule again on this side, because two independent guards on "never
//! install a prerelease, never downgrade" is the right number for a mechanism
//! that replaces the running executable.
//!
//! # Failure
//!
//! Nothing here is allowed to matter. A machine with no network, a DNS failure,
//! a GitHub outage, a rate limit, a truncated JSON body, an HTML error page
//! where a manifest should be — every one of them ends as a `Failed` variant
//! that the UI renders as a quiet line, and the application carries on doing
//! the job it was opened for. There is no `?` on a network path in this file
//! that can reach a command's `Err`.
//!
//! Startup is never blocked: nothing in this module is called from `setup`.
//! The frontend asks, after it has rendered, and only if the user has left the
//! automatic check on.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::logic::release::{self, Decision};
use crate::platform::windows::packaging;

/// Whether this install can update itself at all, and where it came from.
///
/// Sent to the frontend once at startup so the UI can decide whether to show
/// an update section rather than showing one that can never do anything.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCapability {
    /// True when running from an MSIX. The Store owns updates; we do nothing.
    pub managed_by_store: bool,
    /// The version running right now, from `Cargo.toml` via Tauri.
    pub current_version: String,
}

/// The outcome of one check. Every variant is a normal, renderable state —
/// there is no failure here that the user has to do anything about.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateCheck {
    /// Nothing to do. Also the answer when the feed is behind us or offering a
    /// prerelease: from the user's side those are all "you are up to date".
    #[serde(rename_all = "camelCase")]
    UpToDate { current_version: String },
    /// A newer stable release exists and passed the policy in `logic::release`.
    #[serde(rename_all = "camelCase")]
    Available {
        current_version: String,
        version: String,
        notes: Option<String>,
        published_at: Option<String>,
    },
    /// This install does not update itself. Only ever the Store.
    #[serde(rename_all = "camelCase")]
    Unsupported { reason: String },
    /// The check could not be completed. The app is unaffected.
    #[serde(rename_all = "camelCase")]
    Failed { reason: String },
}

/// The update the last check found, held so `install` does not have to check
/// again and risk installing something different from what the user agreed to.
///
/// An async mutex rather than a `std` one: it is held across the `await` in
/// `install`, and holding a blocking mutex across an await point is how an
/// async runtime deadlocks. Tauri re-exports the runtime's own, so this costs
/// no extra dependency.
#[derive(Default)]
pub struct PendingUpdate(pub tauri::async_runtime::Mutex<Option<tauri_plugin_updater::Update>>);

/// What this install can do about updates. Cheap, infallible, no network.
pub fn capability(app: &AppHandle) -> UpdateCapability {
    UpdateCapability {
        managed_by_store: packaging::is_packaged(),
        current_version: app.package_info().version.to_string(),
    }
}

/// Ask the feed whether there is a newer stable release.
///
/// Returns a state, never an error. See the module note on failure.
pub async fn check(app: &AppHandle, pending: &PendingUpdate) -> UpdateCheck {
    let current = app.package_info().version.to_string();

    if packaging::is_packaged() {
        return UpdateCheck::Unsupported {
            reason: "Installed from the Microsoft Store, which manages updates.".into(),
        };
    }

    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            // Misconfiguration — a missing public key or a malformed endpoint.
            // A build defect, not a user's problem, so it is logged loudly and
            // shown quietly.
            log::error!("the updater is not configured correctly: {e}");
            return UpdateCheck::Failed {
                reason: "The update service is unavailable in this build.".into(),
            };
        }
    };

    let found = match updater.check().await {
        Ok(found) => found,
        Err(e) => {
            // The ordinary case: offline, DNS, TLS, 404, rate limit, a
            // truncated body, an HTML error page. All of it lands here and
            // none of it is worth alarming anyone about.
            log::warn!("update check failed: {e}");
            return UpdateCheck::Failed {
                reason: "Could not reach GitHub to check for updates.".into(),
            };
        }
    };

    let Some(update) = found else {
        *pending.0.lock().await = None;
        return UpdateCheck::UpToDate {
            current_version: current,
        };
    };

    // The plugin already refused anything it considered older. This is the
    // second, independent guard, and it is the one that knows about
    // prereleases. See `logic::release`.
    match release::decide(&current, &update.version) {
        Decision::Offer => {}
        Decision::Ignore(reason) => {
            log::info!(
                "ignoring advertised version {}: {}",
                update.version,
                reason.as_str()
            );
            *pending.0.lock().await = None;
            return UpdateCheck::UpToDate {
                current_version: current,
            };
        }
    }

    let result = UpdateCheck::Available {
        current_version: current,
        version: update.version.clone(),
        notes: update.body.clone().filter(|n| !n.trim().is_empty()),
        published_at: update.date.map(|d| d.to_string()),
    };

    // Held for `install`, so what gets installed is the thing that was
    // checked, policy-approved and shown to the user — not whatever the feed
    // happens to say a moment later.
    *pending.0.lock().await = Some(update);
    result
}

/// Download, verify and install the update the last check approved, then hand
/// off to the platform's update-completion path.
///
/// On Windows, the Tauri updater plugin launches the NSIS/MSI updater and exits
/// the process itself, so this function normally does not return on success.
///
/// Nothing is downloaded that was not first offered by `check`. That ordering
/// is the point — it means this function cannot be talked into fetching an
/// arbitrary artifact, because it never chooses one.
pub async fn install(app: &AppHandle, pending: &PendingUpdate) -> Result<(), String> {
    if packaging::is_packaged() {
        return Err("This installation is managed by the Microsoft Store.".into());
    }

    let update = pending.0.lock().await.take();
    let Some(update) = update else {
        return Err("No update is ready to install. Check for updates first.".into());
    };

    log::warn!("installing update {}", update.version);

    // The plugin verifies the download against the minisign public key baked
    // into this binary before it hands the bytes to Windows. An artifact that
    // fails verification never reaches the disk as something executable.
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| {
            log::error!("update install failed: {e}");
            // The installed app is untouched by a failed install: the NSIS
            // updater replaces files only after a successful download and
            // signature check.
            "The update could not be installed. Your current version is unchanged.".to_string()
        })?;

    restart_after_successful_install(app)
}

#[cfg(windows)]
fn restart_after_successful_install(_app: &AppHandle) -> Result<(), String> {
    // tauri-plugin-updater's Windows installer path starts the updater process
    // and calls std::process::exit(0). Calling AppHandle::restart afterwards is
    // therefore not the update mechanism on Windows, but it does pull Tauri's
    // restart helper and Rust's process-spawn implementation into the binary.
    Ok(())
}

#[cfg(not(windows))]
fn restart_after_successful_install(app: &AppHandle) -> Result<(), String> {
    app.restart()
}
