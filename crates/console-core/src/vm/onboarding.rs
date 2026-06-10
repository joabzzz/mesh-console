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
#[cfg(feature = "embedded")]
pub fn detect_hardware() -> Option<HardwareSummary> {
    let profile = mesh_llm_sdk::native_runtime::host_runtime_profile();
    let gpu = profile.gpus.first()?;
    Some(HardwareSummary {
        gpu_name: gpu.display_name.clone(),
        vram_gb: gpu.vram_bytes.unwrap_or(0) as f64 / 1_073_741_824.0,
        unified_memory: gpu.unified_memory,
    })
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
];

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
    fn mac_mini_24gb_defaults_to_14b() {
        let picks = recommend_models(24.0);
        assert_eq!(picks[0].label, "Qwen3 14B");
        assert!(picks.iter().any(|m| m.label == "Qwen3 0.6B"));
        assert!(!picks.iter().any(|m| m.label == "Qwen3 32B"));
    }

    #[test]
    fn zero_vram_still_offers_something() {
        let picks = recommend_models(0.0);
        assert_eq!(picks.len(), 1);
    }
}
