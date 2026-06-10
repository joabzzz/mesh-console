//! Non-UI core of mesh-console: management-API client, persistence, and
//! display/view-model logic. The Dioxus app (`mesh-console`) layers views on
//! top of this crate; everything here is testable without a UI.

pub mod api;
pub mod chat_persist;
#[cfg(feature = "embedded")]
pub mod node_manager;
#[cfg(feature = "embedded")]
pub mod runtime_install;
pub mod sdk_session;
pub mod settings;
pub mod status_display;
pub mod vm;

/// Payload types shared with the management API (`console-types` crate),
/// re-exported under the historical module name.
pub use console_types as state;
