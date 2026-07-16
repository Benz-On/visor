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

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

pub fn infer_cpu_tdp(physical_cores: usize, brand: &str) -> f64 {
    let normalized = brand.to_ascii_lowercase();
    if normalized.contains(" ryzen") && normalized.contains('u') {
        return 28.0;
    }
    if normalized.contains(" hx") || normalized.contains(" hs") {
        return 55.0;
    }
    match physical_cores {
        0..=4 => 65.0,
        5..=8 => 105.0,
        9..=16 => 170.0,
        _ => 230.0,
    }
}

pub fn estimate(
    cpu_load: f64,
    gpu_load: f64,
    memory_total_gb: f64,
    disk_activity: f64,
    cpu_tdp: f64,
    speed_ratio: f64,
    measured_gpu_power: f64,
    gpu_power_limit: f64,
) -> EnergyEstimate {
    let cpu_idle = (cpu_tdp * 0.075).max(5.0);
    let cpu = cpu_idle
        + (cpu_tdp - cpu_idle)
            * (clamp(cpu_load, 0.0, 100.0) / 100.0).powf(0.78)
            * clamp(speed_ratio, 0.45, 1.35);
    let measured_gpu = measured_gpu_power > 0.0;
    let gpu = if measured_gpu {
        measured_gpu_power
    } else {
        gpu_power_limit.max(30.0) * (0.1 + 0.9 * (clamp(gpu_load, 0.0, 100.0) / 100.0).powf(0.92))
    };
    let memory = 2.0 + memory_total_gb.max(1.0) * 0.12;
    let storage = 2.5 + clamp(disk_activity, 0.0, 100.0) * 0.055;
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
    fn measured_gpu_improves_confidence_without_hiding_estimates() {
        let estimated = estimate(50.0, 60.0, 32.0, 10.0, 105.0, 1.0, 0.0, 220.0);
        let hybrid = estimate(50.0, 60.0, 32.0, 10.0, 105.0, 1.0, 142.0, 220.0);
        assert!(!estimated.measured_gpu);
        assert!(hybrid.measured_gpu);
        assert_eq!(hybrid.gpu, 142.0);
        assert!(estimated.watts > 0.0);
    }
}
