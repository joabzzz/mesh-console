//! `/api/runtime/llama` — llama.cpp server metrics and slot occupancy shown
//! in the node detail drawer. Mirrors `LlamaRuntimePayload` consumed by
//! `mesh-llm-ui` (`NodeDrawer`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaRuntimeMetricSample {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub labels: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    pub value: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaRuntimeSlot {
    #[serde(default)]
    pub index: Option<u32>,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub id_task: Option<i64>,
    #[serde(default)]
    pub n_ctx: Option<u64>,
    #[serde(default)]
    pub speculative: Option<bool>,
    #[serde(default)]
    pub is_processing: bool,
}

/// "ready" | "error" | "unavailable" | …
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaRuntimeMetrics {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_attempt_unix_ms: Option<u64>,
    #[serde(default)]
    pub last_success_unix_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub samples: Vec<LlamaRuntimeMetricSample>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaRuntimeSlots {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_attempt_unix_ms: Option<u64>,
    #[serde(default)]
    pub last_success_unix_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub slots: Vec<LlamaRuntimeSlot>,
}

/// Pre-digested combination of metrics + slots the UI can render directly.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaRuntimeItems {
    #[serde(default)]
    pub metrics: Vec<LlamaRuntimeMetricSample>,
    #[serde(default)]
    pub slots: Vec<LlamaRuntimeSlot>,
    #[serde(default)]
    pub slots_total: u64,
    #[serde(default)]
    pub slots_busy: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LlamaRuntimePayload {
    #[serde(default)]
    pub metrics: LlamaRuntimeMetrics,
    #[serde(default)]
    pub slots: LlamaRuntimeSlots,
    #[serde(default)]
    pub items: Option<LlamaRuntimeItems>,
}
