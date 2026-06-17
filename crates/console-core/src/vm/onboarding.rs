//! First-run onboarding logic: hardware detection and starter-model
//! recommendations sized to the machine's VRAM.

/// What we tell a novice about their machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HardwareSummary {
    /// e.g. "Apple M4 Pro"
    pub gpu_name: String,
    pub vram_gb: f64,
    pub unified_memory: bool,
}

/// Detect the primary GPU via the same prober the native-runtime installer
/// uses. `None` when no GPU is visible (CPU-only fallback still works).
///
/// On Apple Silicon the prober often reports a generic Metal name with no
/// VRAM figure; unified memory means total RAM *is* the GPU budget, so fall
/// back to `sysctl` for both the chip name and the memory size.
#[cfg(feature = "embedded")]
pub fn detect_hardware() -> Option<HardwareSummary> {
    let profile = mesh_llm_sdk::native_runtime::host_runtime_profile();
    let gpu = profile.gpus.first()?;
    let mut summary = HardwareSummary {
        gpu_name: gpu.display_name.clone(),
        vram_gb: gpu.vram_bytes.unwrap_or(0) as f64 / 1_073_741_824.0,
        unified_memory: gpu.unified_memory,
    };
    if cfg!(target_os = "macos") {
        if summary.vram_gb <= 0.0 {
            if let Some(bytes) = sysctl_u64("hw.memsize") {
                summary.vram_gb = bytes as f64 / 1_073_741_824.0;
                summary.unified_memory = true;
            }
        }
        if summary.gpu_name.is_empty() || summary.gpu_name.contains("Metal Support") {
            if let Some(chip) = sysctl_string("machdep.cpu.brand_string") {
                summary.gpu_name = chip;
            }
        }
    }
    Some(summary)
}

#[cfg(feature = "embedded")]
fn sysctl_string(key: &str) -> Option<String> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", key])
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

#[cfg(feature = "embedded")]
fn sysctl_u64(key: &str) -> Option<u64> {
    sysctl_string(key)?.parse().ok()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StarterModel {
    /// Model ref the daemon can download (HF GGUF ref).
    pub model_ref: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    pub size_gb: f64,
    /// Minimum VRAM where this is a comfortable pick.
    pub min_vram_gb: f64,
}

const STARTERS: &[StarterModel] = &[
    StarterModel {
        model_ref: "unsloth/Qwen3-0.6B-GGUF:Q4_K_M",
        label: "Qwen3 0.6B",
        blurb: "Tiny and instant — great for trying the mesh",
        size_gb: 0.5,
        min_vram_gb: 0.0,
    },
    StarterModel {
        model_ref: "unsloth/Qwen3-4B-GGUF:Q4_K_M",
        label: "Qwen3 4B",
        blurb: "Fast everyday assistant",
        size_gb: 2.6,
        min_vram_gb: 6.0,
    },
    StarterModel {
        model_ref: "unsloth/Qwen3-8B-GGUF:Q4_K_M",
        label: "Qwen3 8B",
        blurb: "Stronger reasoning, still snappy",
        size_gb: 5.0,
        min_vram_gb: 12.0,
    },
    StarterModel {
        model_ref: "unsloth/Qwen3-14B-GGUF:Q4_K_M",
        label: "Qwen3 14B",
        blurb: "Serious quality for bigger machines",
        size_gb: 9.0,
        min_vram_gb: 20.0,
    },
    StarterModel {
        model_ref: "unsloth/Qwen3-32B-GGUF:Q4_K_M",
        label: "Qwen3 32B",
        blurb: "Top quality — needs lots of memory",
        size_gb: 19.8,
        min_vram_gb: 36.0,
    },
    // Last in the list = first recommendation wherever it fits: a fast MoE
    // that outclasses similarly-sized dense models.
    StarterModel {
        model_ref: "unsloth/GLM-4.7-Flash-GGUF:Q4_K_M",
        label: "GLM 4.7 Flash",
        blurb: "Fast mixture-of-experts — the best all-rounder",
        size_gb: 17.6,
        min_vram_gb: 24.0,
    },
];

/// Model ref for the recommended all-rounder (GLM 4.7 Flash). Used as the
/// default pick when starting a mesh so users don't have to memorize a code.
pub const DEFAULT_MODEL_REF: &str = "unsloth/GLM-4.7-Flash-GGUF:Q4_K_M";

/// Every starter model in presentation order: best all-rounder first, then
/// largest-to-smallest. Used to populate model pickers outside onboarding,
/// where we don't size to the machine.
pub fn starter_catalog() -> Vec<StarterModel> {
    let mut v: Vec<StarterModel> = STARTERS.iter().copied().collect();
    v.reverse();
    v
}

/// The default starter model (GLM 4.7 Flash), falling back to the first
/// catalog entry if the ref ever drifts.
pub fn default_starter() -> StarterModel {
    STARTERS
        .iter()
        .copied()
        .find(|m| m.model_ref == DEFAULT_MODEL_REF)
        .unwrap_or(STARTERS[0])
}

/// Starter models that fit this machine, biggest-first so the best fitting
/// pick is the default. Always returns at least the tiny model.
pub fn recommend_models(vram_gb: f64) -> Vec<StarterModel> {
    let mut fits: Vec<StarterModel> = STARTERS
        .iter()
        .copied()
        .filter(|m| vram_gb >= m.min_vram_gb)
        .collect();
    if fits.is_empty() {
        fits.push(STARTERS[0]);
    }
    fits.reverse();
    fits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_machine_gets_only_tiny_model() {
        let picks = recommend_models(4.0);
        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].label, "Qwen3 0.6B");
    }

    #[test]
    fn mac_mini_24gb_defaults_to_glm_flash() {
        let picks = recommend_models(24.0);
        assert_eq!(picks[0].label, "GLM 4.7 Flash");
        assert!(picks.iter().any(|m| m.label == "Qwen3 0.6B"));
        assert!(!picks.iter().any(|m| m.label == "Qwen3 32B"));
    }

    #[test]
    fn m4_pro_48gb_still_prefers_glm_flash() {
        let picks = recommend_models(48.0);
        assert_eq!(picks[0].label, "GLM 4.7 Flash");
        assert!(picks.iter().any(|m| m.label == "Qwen3 32B"));
    }

    #[test]
    fn zero_vram_still_offers_something() {
        let picks = recommend_models(0.0);
        assert_eq!(picks.len(), 1);
    }
}
