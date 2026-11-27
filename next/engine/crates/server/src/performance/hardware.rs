use serde::Serialize;

#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub available_threads: Option<usize>,
    pub physical_cores: Option<usize>,
    pub performance_cores: Option<usize>,
    pub ram_mb: Option<u64>,
}

impl Hardware {
    pub fn detect() -> Self {
        let mut hw = Self {
            available_threads: std::thread::available_parallelism().ok().map(|n| n.get()),
            ..Self::default()
        };
        platform(&mut hw);
        hw
    }
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::adapters;
#[cfg(windows)]
use windows::platform;

#[cfg(not(windows))]
pub fn adapters() -> Option<Vec<String>> {
    None
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn platform(hw: &mut Hardware) {
    let info = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    hw.ram_mb = info
        .lines()
        .find_map(|s| s.strip_prefix("MemTotal:"))
        .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
        .map(|kb| kb / 1024);
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::platform;

#[cfg(not(any(
    windows,
    target_os = "macos",
    target_os = "linux",
    target_os = "android"
)))]
fn platform(_: &mut Hardware) {}
