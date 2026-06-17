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
        fix_native_runtime_load_order();
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
        Ok(_) => {
            fix_native_runtime_load_order();
            Ok(())
        }
        Err(err) => {
            // A stale-version runtime is better than nothing when offline.
            let have_any = default_native_runtime_cache()
                .and_then(|cache| cache.installed())
                .map(|runtimes| !runtimes.is_empty())
                .unwrap_or(false);
            if have_any {
                fix_native_runtime_load_order();
                Ok(())
            } else {
                Err(format!("native runtime install failed: {err:#}"))
            }
        }
    }
}

/// Reorder the installed runtime's `manifest.json` library list into
/// dependency order. The v0.72.0-rc1 release artifact lists `libllama-common`
/// before `libllama`, violating the loader's own contract ("dependencies
/// should appear before the primary libllama") — dlopen then fails to resolve
/// `@rpath/libllama.0.dylib`. Idempotent; a stable no-op once upstream ships
/// correctly ordered manifests.
fn dylib_load_rank(path: &str) -> u8 {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.contains("ggml-base") {
        0
    } else if name.contains("ggml-") {
        1 // backend libs: cpu / blas / metal / cuda…
    } else if name.contains("ggml") {
        2
    } else if name.contains("llama-common") {
        4
    } else if name.contains("llama") {
        3
    } else if name.contains("mtmd") {
        5
    } else {
        6
    }
}

pub fn fix_native_runtime_load_order() {
    let Ok(cache) = default_native_runtime_cache() else {
        return;
    };
    let Ok(installed) = cache.installed() else {
        return;
    };
    for runtime in installed {
        let manifest_path = runtime.path.join("manifest.json");
        let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(mut manifest) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let Some(libraries) = manifest
            .get_mut("runtime")
            .and_then(|r| r.get_mut("libraries"))
            .and_then(|l| l.as_array_mut())
        else {
            continue;
        };
        let mut entries: Vec<String> = libraries
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let before = entries.clone();
        entries.sort_by_key(|path| dylib_load_rank(path));
        if entries != before {
            *libraries = entries.into_iter().map(serde_json::Value::String).collect();
            if let Ok(pretty) = serde_json::to_string_pretty(&manifest) {
                let _ = std::fs::write(&manifest_path, pretty);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_rank_orders_llama_after_its_dependencies() {
        let mut libs = vec![
            "lib/libllama-common.0.dylib",
            "lib/libllama.0.dylib",
            "lib/libggml.0.dylib",
            "lib/libmtmd.0.dylib",
            "lib/libggml-metal.0.dylib",
            "lib/libggml-base.0.dylib",
        ];
        libs.sort_by_key(|p| dylib_load_rank(p));
        assert_eq!(
            libs,
            vec![
                "lib/libggml-base.0.dylib",
                "lib/libggml-metal.0.dylib",
                "lib/libggml.0.dylib",
                "lib/libllama.0.dylib",
                "lib/libllama-common.0.dylib",
                "lib/libmtmd.0.dylib",
            ]
        );
    }

    #[test]
    fn already_ordered_manifest_is_stable() {
        let mut libs = vec![
            "lib/libggml-base.dylib",
            "lib/libggml-cpu.dylib",
            "lib/libggml.dylib",
            "lib/libllama.dylib",
            "lib/libllama-common.dylib",
        ];
        let before = libs.clone();
        libs.sort_by_key(|p| dylib_load_rank(p));
        assert_eq!(libs, before);
    }
}
