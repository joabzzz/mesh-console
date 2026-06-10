//! Serde mirrors of the mesh-llm management API payloads — the same wire
//! format the web console (`mesh-llm-ui`) consumes. The canonical shapes live
//! in `mesh-llm-host-runtime/src/api/` (they are `pub(crate)` upstream, so we
//! mirror them here) cross-checked against `mesh-llm-ui/src/lib/api/types.ts`.
//!
//! Every struct is tolerant: `#[serde(default)]` on all fields and unknown
//! fields ignored, so older/newer daemons still parse. Fixture round-trip
//! tests (captured from a live node) are the drift alarm.

mod chat;
mod control;
mod discover;
mod models;
mod runtime;
mod status;

pub use chat::*;
pub use control::*;
pub use discover::*;
pub use models::*;
pub use runtime::*;
pub use status::*;
