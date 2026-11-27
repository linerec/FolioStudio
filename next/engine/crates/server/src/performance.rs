mod hardware;
pub use hardware::{Hardware, adapters};
use serde::Serialize;

#[derive(Default, Debug)]
pub struct Options {
    pub profile: String,
    pub accel_off: bool,
    pub threads: Option<usize>,
    pub mem_budget_mb: Option<u64>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub profile: &'static str,
    pub render_workers: usize,
    pub pdf_reuse_image_rows: bool,
    pub budget_mb: Option<u64>,
    pub reason: &'static str,
}

/// Size the concurrent CPU preview pool. A page can exceed the per-worker planning allowance.
pub fn select(hw: &Hardware, opts: &Options, auto_validated: bool) -> Plan {
    let legacy = hw.available_threads.unwrap_or(4).clamp(1, 8);
    let requested = opts.profile.as_str();
    if opts.accel_off
        || (!auto_validated
            && matches!(requested, "" | "auto")
            && opts.threads.is_none()
            && opts.mem_budget_mb.is_none())
    {
        return Plan {
            profile: "legacy",
            render_workers: legacy,
            pdf_reuse_image_rows: !opts.accel_off,
            budget_mb: None,
            reason: if opts.accel_off {
                "accel-off"
            } else {
                "awaiting-platform-measurement"
            },
        };
    }
    let cores = hw
        .performance_cores
        .or(hw.physical_cores)
        .unwrap_or(legacy)
        .min(legacy);
    let budget = (hw.ram_mb.unwrap_or(4096) / 32)
        .clamp(64, 512)
        .min(opts.mem_budget_mb.unwrap_or(u64::MAX))
        .max(1);
    let (profile, cap, reason) = match requested {
        "low" => ("low", 1, "requested-profile"),
        "standard" => ("standard", 4, "requested-profile"),
        "high" => ("high", 8, "requested-profile"),
        _ if hw.ram_mb.is_none() || hw.available_threads.is_none() => {
            ("low", 1, "incomplete-hardware")
        }
        _ if cores <= 2 || budget < 128 => ("low", 1, "core-or-memory-limit"),
        _ if cores < 8 || budget < 512 => ("standard", 4, "core-or-memory-limit"),
        _ => ("high", 8, "cores-and-memory"),
    };
    let render_workers = cores
        .min(cap)
        .min((budget / 64) as usize)
        .min(opts.threads.unwrap_or(usize::MAX))
        .max(1);
    Plan {
        profile,
        render_workers,
        pdf_reuse_image_rows: true,
        budget_mb: Some(budget),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pc() -> Hardware {
        Hardware {
            available_threads: Some(32),
            physical_cores: Some(24),
            performance_cores: Some(8),
            ram_mb: Some(65536),
        }
    }
    #[test]
    fn core_memory_and_process_limits_bound_the_pool() {
        let mut hw = pc();
        let mut opts = Options::default();
        assert_eq!(select(&hw, &opts, true).render_workers, 8);
        hw.performance_cores = Some(3);
        assert_eq!(select(&hw, &opts, true).render_workers, 3);
        hw.available_threads = Some(2);
        opts.profile = "high".into();
        assert_eq!(select(&hw, &opts, true).render_workers, 2);
        opts.mem_budget_mb = Some(1);
        assert_eq!(select(&hw, &opts, true).render_workers, 1);
        opts.mem_budget_mb = None;
        opts.threads = Some(1);
        assert_eq!(select(&hw, &opts, true).render_workers, 1);
    }
    #[test]
    fn missing_hardware_disable_and_unmeasured_platform() {
        let hw = Hardware::default();
        assert_eq!(select(&hw, &Options::default(), true).render_workers, 1);
        let opts = Options {
            accel_off: true,
            ..Options::default()
        };
        assert_eq!(select(&hw, &opts, true).render_workers, 4);
        assert!(!select(&hw, &opts, true).pdf_reuse_image_rows);
        assert!(select(&hw, &Options::default(), true).pdf_reuse_image_rows);
        assert_eq!(select(&pc(), &Options::default(), false).profile, "legacy");
        let low = Options {
            profile: "low".into(),
            ..Options::default()
        };
        assert_eq!(select(&pc(), &low, false).render_workers, 1);
    }
}
