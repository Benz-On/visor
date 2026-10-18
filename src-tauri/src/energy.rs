use serde_json::{json, Value};

pub struct EnergyEstimate {
    pub watts: f64,
    pub cpu: f64,
    pub gpu: f64,
    pub memory: f64,
    pub storage: f64,
    pub platform: f64,
    pub conversion_loss: f64,
    pub measured_gpu: bool,
}

pub struct EnergyInputs {
    pub cpu_load: f64,
    pub gpu_load: f64,
    pub memory_total_gb: f64,
    pub disk_activity: f64,
    pub cpu_tdp: f64,
    pub speed_ratio: f64,
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

pub fn estimate(inputs: EnergyInputs) -> EnergyEstimate {
    let cpu_idle = (inputs.cpu_tdp * 0.075).max(5.0);
    let cpu = cpu_idle
        + (inputs.cpu_tdp - cpu_idle)
            * (clamp(inputs.cpu_load, 0.0, 100.0) / 100.0).powf(0.78)
            * clamp(inputs.speed_ratio, 0.45, 1.35);
    let measured_gpu = inputs.measured_gpu_power > 0.0;
    let gpu = if measured_gpu {
        inputs.measured_gpu_power
    } else {
        inputs.gpu_power_limit.max(30.0)
            * (0.1 + 0.9 * (clamp(inputs.gpu_load, 0.0, 100.0) / 100.0).powf(0.92))
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
        measured_gpu,
    }
}

pub fn as_json(energy: &EnergyEstimate, session_wh: f64) -> Value {
    let tariff = 0.25;
    let carbon = 56.0;
    let daily_kwh = energy.watts * 24.0 / 1000.0;
    json!({
        "watts": round(energy.watts, 1),
        "confidence": if energy.measured_gpu { "hybrid" } else { "estimated" },
        "confidenceScore": if energy.measured_gpu { 76 } else { 54 },
        "methodology": if energy.measured_gpu {
            "GPU measured by vendor telemetry; CPU and platform modeled from live load."
        } else {
            "CPU, GPU and platform modeled from live utilization and hardware limits."
        },
        "sessionWh": round(session_wh, 2),
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
            cpu_load: 50.0,
            gpu_load: 60.0,
            memory_total_gb: 32.0,
            disk_activity: 10.0,
            cpu_tdp: 105.0,
            speed_ratio: 1.0,
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
}
