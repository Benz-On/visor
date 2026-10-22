use serde_json::{json, Value};

pub struct EnergyEstimate {
    pub watts: f64,
    pub cpu: f64,
    pub gpu: f64,
    pub memory: f64,
    pub storage: f64,
    pub platform: f64,
    pub conversion_loss: f64,
    pub measured_cpu: bool,
    pub measured_gpu: bool,
}

pub struct EnergyInputs {
    /// GPU model string; used only to classify the power fallback when vendor
    /// telemetry is unavailable.
    pub gpu_model: String,
    pub cpu_load: f64,
    pub gpu_load: f64,
    pub memory_total_gb: f64,
    pub disk_activity: f64,
    pub cpu_tdp: f64,
    pub speed_ratio: f64,
    pub measured_cpu_power: f64,
    pub measured_gpu_power: f64,
    pub gpu_power_limit: f64,
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

/// Trailing model-number suffix, e.g. the `u` in "155u" or the `hx` in
/// "14900hx". Runs of three or more digits only, so ordinals such as
/// "13th Gen" are not mistaken for a model number.
fn model_suffix(brand: &str) -> Option<String> {
    let chars: Vec<char> = brand.chars().collect();
    let mut found = None;
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let mut digits = 0;
        while index + digits < chars.len() && chars[index + digits].is_ascii_digit() {
            digits += 1;
        }
        if digits >= 3 {
            let mut letters = 0;
            while letters < 2
                && index + digits + letters < chars.len()
                && chars[index + digits + letters].is_ascii_alphabetic()
            {
                letters += 1;
            }
            if letters > 0 {
                let start = index + digits;
                found = Some(chars[start..start + letters].iter().collect());
            }
        }
        index += digits;
    }
    found
}

pub fn infer_cpu_tdp(physical_cores: usize, brand: &str) -> f64 {
    // Suffix letters decide the power class for both Intel and AMD mobile
    // parts; matching on brand keywords alone left every Intel U-series chip in
    // the desktop core-count buckets and inflated its estimated draw.
    match model_suffix(&brand.to_ascii_lowercase()).as_deref() {
        Some("u") => 28.0,
        Some("h") | Some("hs") | Some("hx") => 55.0,
        _ => match physical_cores {
            0..=4 => 65.0,
            5..=8 => 105.0,
            9..=16 => 170.0,
            _ => 230.0,
        },
    }
}

/// Power ceiling for integrated graphics in watts. iGPUs share the package's
/// thermal and power budget with the CPU; 28 W covers the common U-class
/// parts and stays honest against the 220 W discrete default.
const INTEGRATED_GPU_LIMIT: f64 = 28.0;

/// Integrated-adapter detection by model name, mirroring the advisor's
/// shared-memory patterns. An absent model is treated as discrete: an unknown
/// GPU exaggerating a laptop projection was the actual failure mode, while
/// desktops without a readable name keep the existing behavior.
fn is_integrated_gpu(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    let intel = model.contains("uhd graphics")
        || model.contains("hd graphics")
        || model.contains("iris")
        // Meteor Lake and Arrow Lake brand their iGPU as "Arc Graphics";
        // discrete Arc cards carry an "Arc A###" name instead.
        || (model.contains("intel") && model.contains("arc") && !model.contains("arc a"));
    // WMI strings vary: "Radeon 780M Graphics", "Radeon(TM) Graphics",
    // "Radeon Vega 8". Every discrete card carries an "RX " prefix.
    let amd_apu = model.contains("radeon")
        && !model.contains("rx ")
        && (model.contains("graphics") || model.contains("vega"));
    let apple = model.contains("apple m");
    intel || amd_apu || apple
}

pub fn estimate(inputs: EnergyInputs) -> EnergyEstimate {
    let cpu_idle = (inputs.cpu_tdp * 0.075).max(5.0);
    let measured_cpu = inputs.measured_cpu_power > 0.0;
    // The package sensor already accounts for boost clocks, per-core residency
    // and package-level losses, so it replaces the TDP curve instead of being
    // blended with it.
    let cpu = if measured_cpu {
        inputs.measured_cpu_power
    } else {
        cpu_idle
            + (inputs.cpu_tdp - cpu_idle)
                * (clamp(inputs.cpu_load, 0.0, 100.0) / 100.0).powf(0.78)
                * clamp(inputs.speed_ratio, 0.45, 1.35)
    };
    let measured_gpu = inputs.measured_gpu_power > 0.0;
    let gpu = if measured_gpu {
        inputs.measured_gpu_power
    } else {
        // A desktop card without vendor telemetry is a different machine from a
        // laptop iGPU: the same 220 W ceiling would inflate an integrated
        // GPU's share of the projection by an order of magnitude. Integrated
        // adapters are detected by their model names, matching the advisor's
        // shared-memory classification.
        let limit = if is_integrated_gpu(&inputs.gpu_model) {
            INTEGRATED_GPU_LIMIT
        } else {
            inputs.gpu_power_limit.max(30.0)
        };
        limit * (0.1 + 0.9 * (clamp(inputs.gpu_load, 0.0, 100.0) / 100.0).powf(0.92))
    };
    let memory = 2.0 + inputs.memory_total_gb.max(1.0) * 0.12;
    let storage = 2.5 + clamp(inputs.disk_activity, 0.0, 100.0) * 0.055;
    let platform = 30.0;
    let components = cpu + gpu + memory + storage + platform;
    let efficiency = if components > 350.0 {
        0.91
    } else if components > 150.0 {
        0.89
    } else {
        0.86
    };
    let watts = components / efficiency;
    EnergyEstimate {
        watts,
        cpu,
        gpu,
        memory,
        storage,
        platform,
        conversion_loss: watts - components,
        measured_cpu,
        measured_gpu,
    }
}

/// Assumed grid carbon intensity in gCO₂e/kWh and electricity price in €/kWh.
/// Settings can override both; these are what a fresh session projects with.
pub const DEFAULT_TARIFF: f64 = 0.25;
pub const DEFAULT_CARBON: f64 = 56.0;

pub fn as_json(
    energy: &EnergyEstimate,
    session_wh: f64,
    session_started_at: &str,
    tariff: f64,
    carbon: f64,
) -> Value {
    let daily_kwh = energy.watts * 24.0 / 1000.0;
    // Confidence follows what is actually instrumented: a modeled CPU with a
    // measured GPU is a different claim from both rails measured on-die.
    let (confidence, score, methodology) = match (energy.measured_cpu, energy.measured_gpu) {
        (true, true) => (
            "measured",
            88,
            "CPU package and GPU measured by on-die telemetry; memory, storage and platform modeled from live load.",
        ),
        (true, false) => (
            "hybrid",
            78,
            "CPU package measured by on-die telemetry; GPU and platform modeled from live load.",
        ),
        (false, true) => (
            "hybrid",
            76,
            "GPU measured by vendor telemetry; CPU and platform modeled from live load.",
        ),
        (false, false) => (
            "estimated",
            54,
            "CPU, GPU and platform modeled from live utilization and hardware limits.",
        ),
    };
    json!({
        "watts": round(energy.watts, 1),
        "confidence": confidence,
        "confidenceScore": score,
        "methodology": methodology,
        "cpuMeasured": energy.measured_cpu,
        "gpuMeasured": energy.measured_gpu,
        "sessionWh": round(session_wh, 2),
        "sessionStartedAt": if session_started_at.is_empty() { Value::Null } else { json!(session_started_at) },
        "hourlyKwh": round(energy.watts / 1000.0, 3),
        "dailyKwh": round(daily_kwh, 3),
        "dailyCost": round(daily_kwh * tariff, 2),
        "monthlyCost": round(daily_kwh * tariff * 30.0, 2),
        "dailyCarbonGrams": (daily_kwh * carbon).round(),
        "tariffPerKwh": tariff,
        "carbonGramsPerKwh": carbon,
        "breakdown": {
            "cpu": round(energy.cpu, 1),
            "gpu": round(energy.gpu, 1),
            "memory": round(energy.memory, 1),
            "storage": round(energy.storage, 1),
            "platform": round(energy.platform, 1),
            "conversionLoss": round(energy.conversion_loss, 1)
        }
    })
}

pub fn round(value: f64, digits: i32) -> f64 {
    let factor = 10_f64.powi(digits);
    (value * factor).round() / factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_suffixes_set_power_class_for_intel_and_amd() {
        // Intel parts were previously unmatched and landed in desktop buckets.
        assert_eq!(infer_cpu_tdp(8, "Intel(R) Core(TM) Ultra 7 155U"), 28.0);
        assert_eq!(
            infer_cpu_tdp(8, "13th Gen Intel(R) Core(TM) i7-1355U"),
            28.0
        );
        assert_eq!(infer_cpu_tdp(10, "Intel(R) Core(TM) i9-14900HX"), 55.0);
        assert_eq!(infer_cpu_tdp(8, "AMD Ryzen 7 6800U"), 28.0);
        assert_eq!(infer_cpu_tdp(8, "AMD Ryzen 9 7945HS"), 55.0);
        // Desktop and server parts keep the core-count fallback.
        assert_eq!(infer_cpu_tdp(6, "Intel(R) Core(TM) i5-8400"), 105.0);
        assert_eq!(infer_cpu_tdp(12, "AMD Ryzen 9 5900X 12-Core"), 170.0);
        assert_eq!(infer_cpu_tdp(2, "Apple M2"), 65.0);
    }

    #[test]
    fn ordinals_are_not_read_as_model_suffixes() {
        // "13th" would otherwise look like a two-letter suffix, and the part
        // should fall through to the core-count estimate.
        assert_eq!(
            infer_cpu_tdp(16, "Intel(R) Core(TM) 13th Gen Ultra 9"),
            170.0
        );
    }

    #[test]
    fn measured_gpu_improves_confidence_without_hiding_estimates() {
        let base = || EnergyInputs {
            gpu_model: "NVIDIA GeForce RTX 4070".to_string(),
            cpu_load: 50.0,
            gpu_load: 60.0,
            memory_total_gb: 32.0,
            disk_activity: 10.0,
            cpu_tdp: 105.0,
            speed_ratio: 1.0,
            measured_cpu_power: 0.0,
            measured_gpu_power: 0.0,
            gpu_power_limit: 220.0,
        };
        let estimated = estimate(base());
        let hybrid = estimate(EnergyInputs {
            measured_gpu_power: 142.0,
            ..base()
        });
        assert!(!estimated.measured_gpu);
        assert!(hybrid.measured_gpu);
        assert_eq!(hybrid.gpu, 142.0);
        assert!(estimated.watts > 0.0);
    }

    #[test]
    fn integrated_gpus_do_not_get_a_discrete_power_ceiling() {
        // The collector falls back to a 220 W limit when the driver reports no
        // ceiling; an iGPU must not inherit it.
        let base = || EnergyInputs {
            gpu_model: "NVIDIA GeForce RTX 4070".to_string(),
            cpu_load: 10.0,
            gpu_load: 80.0,
            memory_total_gb: 16.0,
            disk_activity: 5.0,
            cpu_tdp: 28.0,
            speed_ratio: 0.7,
            measured_cpu_power: 0.0,
            measured_gpu_power: 0.0,
            gpu_power_limit: 220.0,
        };
        let discrete = estimate(base());
        let integrated = estimate(EnergyInputs {
            gpu_model: "Intel(R) Iris(R) Xe Graphics".to_string(),
            ..base()
        });
        // Same load, but the iGPU ceiling is 28 W instead of the 220 W
        // discrete fallback, so its modeled GPU watts must be far lower.
        assert!(integrated.gpu < discrete.gpu / 4.0);
        assert!(is_integrated_gpu("Intel Iris Xe Graphics"));
        assert!(is_integrated_gpu("AMD Radeon 780M Graphics"));
        assert!(is_integrated_gpu("AMD Radeon(TM) Graphics"));
        assert!(is_integrated_gpu("Intel(R) Arc(TM) Graphics"));
        assert!(is_integrated_gpu("Apple M2"));
        assert!(!is_integrated_gpu("NVIDIA GeForce RTX 5070 Ti"));
        assert!(!is_integrated_gpu("AMD Radeon RX 7900 XTX"));
        assert!(!is_integrated_gpu("Intel Arc A770 Graphics"));
    }

    #[test]
    fn a_measured_package_replaces_the_tdp_curve_not_blends_with_it() {
        let base = || EnergyInputs {
            gpu_model: "NVIDIA GeForce RTX 4070".to_string(),
            cpu_load: 50.0,
            gpu_load: 60.0,
            memory_total_gb: 32.0,
            disk_activity: 10.0,
            cpu_tdp: 170.0,
            speed_ratio: 1.0,
            measured_cpu_power: 0.0,
            measured_gpu_power: 0.0,
            gpu_power_limit: 220.0,
        };
        let modeled = estimate(base());
        let measured = estimate(EnergyInputs {
            measured_cpu_power: 58.2,
            ..base()
        });
        assert_eq!(measured.cpu, 58.2);
        assert!(measured.measured_cpu);
        assert_eq!(
            as_json(&measured, 0.0, "", DEFAULT_TARIFF, DEFAULT_CARBON)["confidence"],
            json!("hybrid")
        );
        // A package sensor and a package power limit are different instruments, so
        // the combined case must claim more than either alone.
        let both = estimate(EnergyInputs {
            measured_cpu_power: 58.2,
            measured_gpu_power: 142.0,
            ..base()
        });
        assert_eq!(
            as_json(&both, 0.0, "", DEFAULT_TARIFF, DEFAULT_CARBON)["confidence"],
            json!("measured")
        );
        assert!(
            as_json(&both, 0.0, "", DEFAULT_TARIFF, DEFAULT_CARBON)["confidenceScore"]
                .as_i64()
                .unwrap()
                > as_json(&measured, 0.0, "", DEFAULT_TARIFF, DEFAULT_CARBON)["confidenceScore"]
                    .as_i64()
                    .unwrap()
        );
        assert!(measured.cpu < modeled.cpu, "50% load on a 170 W part");
    }

    #[test]
    fn price_and_carbon_settings_move_the_projection() {
        let estimate = estimate(EnergyInputs {
            gpu_model: "NVIDIA GeForce RTX 4070".to_string(),
            cpu_load: 50.0,
            gpu_load: 60.0,
            memory_total_gb: 32.0,
            disk_activity: 10.0,
            cpu_tdp: 170.0,
            speed_ratio: 1.0,
            measured_cpu_power: 200.0,
            measured_gpu_power: 142.0,
            gpu_power_limit: 220.0,
        });
        let cheap = as_json(&estimate, 0.0, "", 0.10, 100.0);
        let dear = as_json(&estimate, 0.0, "", 0.40, 800.0);
        assert_eq!(cheap["tariffPerKwh"], json!(0.1));
        assert!(
            dear["dailyCost"].as_f64().unwrap() > cheap["dailyCost"].as_f64().unwrap() * 3.9,
            "0.40 vs 0.10 EUR/kWh"
        );
        assert!(
            dear["dailyCarbonGrams"].as_f64().unwrap()
                > cheap["dailyCarbonGrams"].as_f64().unwrap()
        );
        // The values echoed back must be the ones actually used, or the Settings
        // field would be decoration.
        assert_ne!(
            cheap["dailyCarbonGrams"],
            as_json(&estimate, 0.0, "", 0.10, DEFAULT_CARBON)["dailyCarbonGrams"]
        );
    }
}
