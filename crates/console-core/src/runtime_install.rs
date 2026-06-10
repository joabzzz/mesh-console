//! First-run installation of the native inference runtime (skippy backend
//! dylibs). The embedded mesh runtime only *loads* runtimes that are already
//! in the cache (`~/.cache/mesh-llm/native-runtimes`) — it never downloads —
//! so serve-mode starts must run this first.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mesh_llm_sdk::native_runtime::{
    default_native_runtime_cache, install_native_runtime, NativeRuntimeDownloadProgress,
    NativeRuntimeInstallOptions, CURRENT_MESH_VERSION,
};

/// Download progress for the native runtime install, suitable for a progress
/// bar. `total_bytes` is `None` until the server reports a content length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

/// Report progress at most every this many bytes so the UI signal isn't
/// hammered on every network chunk.
const PROGRESS_GRANULARITY_BYTES: u64 = 4 * 1024 * 1024;

/// True when a native runtime matching the embedded mesh version is already
/// installed, meaning serve-mode can start without any download.
pub fn native_runtime_installed() -> bool {
    default_native_runtime_cache()
        .and_then(|cache| cache.installed())
        .map(|runtimes| {
            runtimes
                .iter()
                .any(|runtime| runtime.mesh_version == CURRENT_MESH_VERSION)
        })
        .unwrap_or(false)
}

/// Ensure a native runtime for the current mesh version is installed,
/// downloading it if necessary. `on_progress` is invoked (throttled) during
/// the download.
///
/// Offline behavior: if the manifest/download fails but *some* runtime is
/// already installed locally, we proceed — the embedded loader picks the best
/// installed runtime.
pub async fn ensure_native_runtime(
    on_progress: impl Fn(InstallProgress) + Send + Sync + 'static,
) -> Result<(), String> {
    if native_runtime_installed() {
        return Ok(());
    }

    let last_reported = AtomicU64::new(0);
    let progress = Arc::new(move |p: NativeRuntimeDownloadProgress| {
        let last = last_reported.load(Ordering::Relaxed);
        if !p.finished && p.downloaded_bytes.saturating_sub(last) < PROGRESS_GRANULARITY_BYTES {
            return;
        }
        last_reported.store(p.downloaded_bytes, Ordering::Relaxed);
        on_progress(InstallProgress {
            downloaded_bytes: p.downloaded_bytes,
            total_bytes: p.total_bytes,
        });
    });

    let options = NativeRuntimeInstallOptions {
        progress: Some(progress),
        ..NativeRuntimeInstallOptions::default()
    };

    match install_native_runtime(options).await {
        Ok(_) => Ok(()),
        Err(err) => {
            // A stale-version runtime is better than nothing when offline.
            let have_any = default_native_runtime_cache()
                .and_then(|cache| cache.installed())
                .map(|runtimes| !runtimes.is_empty())
                .unwrap_or(false);
            if have_any {
                Ok(())
            } else {
                Err(format!("native runtime install failed: {err:#}"))
            }
        }
    }
}
