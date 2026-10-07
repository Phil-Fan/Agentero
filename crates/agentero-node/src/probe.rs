//! Host capability probe.
//!
//! Produces the machine snapshot advertised in the Agent Card's `agentero`
//! extension. Everything here is best-effort: a missing GPU tool or an
//! unreadable `/proc` degrades to an empty/`None` field rather than failing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostCapabilities {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub cpu_count: usize,
    pub memory_bytes: Option<u64>,
    pub gpus: Vec<GpuDevice>,
    pub agentero_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDevice {
    pub name: String,
    pub memory_bytes: Option<u64>,
}

/// Probe the current host.
pub fn probe() -> HostCapabilities {
    HostCapabilities {
        hostname: hostname(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_count: cpu_count(),
        memory_bytes: memory_total(),
        gpus: probe_gpus(),
        agentero_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Best-effort hostname: `HOSTNAME` env, then the `hostname` command.
pub fn hostname() -> String {
    if let Ok(value) = std::env::var("HOSTNAME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(output) = std::process::Command::new("hostname").output() {
        if output.status.success() {
            if let Ok(text) = String::from_utf8(output.stdout) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    "agentero-node".to_string()
}

fn cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Total physical memory in bytes. Linux: `/proc/meminfo`; elsewhere `None`.
fn memory_total() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb.saturating_mul(1024));
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// NVIDIA GPUs via `nvidia-smi`, empty when absent.
fn probe_gpus() -> Vec<GpuDevice> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split(',').map(str::trim);
            let name = parts.next()?.to_string();
            if name.is_empty() {
                return None;
            }
            let memory_bytes = parts
                .next()
                .and_then(|m| m.parse::<u64>().ok())
                .map(|mb| mb.saturating_mul(1024 * 1024));
            Some(GpuDevice { name, memory_bytes })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_reports_os_and_arch() {
        let caps = probe();
        assert_eq!(caps.os, std::env::consts::OS);
        assert_eq!(caps.arch, std::env::consts::ARCH);
        assert!(caps.cpu_count >= 1);
        assert!(!caps.agentero_version.is_empty());
        assert!(!caps.hostname.is_empty());
    }
}
