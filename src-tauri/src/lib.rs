// The embedded MeshNode builder/start future nests deeply enough to blow the
// default query depth when computing async fn layouts.
#![recursion_limit = "256"]

pub mod agent;
pub mod compute;
pub mod diagnose;
pub mod events;
pub mod node;
pub mod proxy;
pub mod server;
pub mod state;

/// Process-wide defaults shared by the desktop shell and the headless daemon.
/// Call before spawning any threads.
pub fn init_process_defaults() {
    // Force classic HTTP model downloads unless the user opted back in: xet's
    // chunked CAS protocol stalls on some networks (~150KB/s and frozen vs
    // ~14MB/s plain CDN GET). Honored by our hf-hub fork branch (git [patch]).
    // Multi-GB layer downloads need this. HF_HUB_DISABLE_XET=0 re-enables xet.
    if std::env::var_os("HF_HUB_DISABLE_XET").is_none() {
        // Safety: called from main before any other threads exist.
        unsafe { std::env::set_var("HF_HUB_DISABLE_XET", "1") };
    }

    // Root the embedded goose agent's config/session state in our own app dir
    // so it never touches a real goose install (~/Library/.../Block/goose).
    // Tests point this at a temp dir for hermetic runs.
    if std::env::var_os("GOOSE_PATH_ROOT").is_none() {
        let root = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("mesh-console/goose");
        // Safety: called from main before any other threads exist.
        unsafe { std::env::set_var("GOOSE_PATH_ROOT", root) };
    }
    // No secrets are stored (the mesh api key is a constant); skip the macOS
    // keychain so the app never triggers a keychain prompt.
    if std::env::var_os("GOOSE_DISABLE_KEYRING").is_none() {
        // Safety: called from main before any other threads exist.
        unsafe { std::env::set_var("GOOSE_DISABLE_KEYRING", "1") };
    }

    // Auto-compact the conversation once it fills 40% of the model's context
    // window (goose's default is 0.8). The mesh runs small local models with
    // modest context, and our one long-lived session accretes history across
    // restarts — compacting early keeps turns snappy and avoids hard
    // truncation mid-reply. goose reads this via get_param (env takes
    // precedence over its config file); it's the only knob — the compaction
    // threshold isn't part of the reply/SessionConfig API.
    if std::env::var_os("GOOSE_AUTO_COMPACT_THRESHOLD").is_none() {
        // Safety: called from main before any other threads exist.
        unsafe { std::env::set_var("GOOSE_AUTO_COMPACT_THRESHOLD", "0.4") };
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(
                // xet_client logs a concurrency-controller INFO line twice a second
                |_| "info,xet_client=warn,cas_client=warn".into(),
            ),
        )
        .with_writer(std::io::stderr)
        .init();
}
