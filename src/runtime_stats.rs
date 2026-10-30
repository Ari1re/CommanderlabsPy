use std::{process::Command, time::{Duration, Instant}};
use sysinfo::{get_current_pid, ProcessesToUpdate, System};

pub struct RuntimeStats {
    system: System,
    pid: Option<sysinfo::Pid>,
    started_at: Instant,
    last_refresh: Instant,
    cpu_usage: f32,
    memory_bytes: u64,
    gpu_usage: Option<f32>,
    gpu_memory_used_mb: Option<u64>,
    gpu_memory_total_mb: Option<u64>,
    last_gpu_refresh: Instant,
}

impl RuntimeStats {
    pub fn new() -> Self {
        let now = Instant::now();
        let mut stats = Self {
            system: System::new(),
            pid: get_current_pid().ok(),
            started_at: now,
            last_refresh: now
                .checked_sub(Duration::from_secs(2))
                .unwrap_or(now),
            cpu_usage: 0.0,
            memory_bytes: 0,
            gpu_usage: None,
            gpu_memory_used_mb: None,
            gpu_memory_total_mb: None,
            last_gpu_refresh: now
                .checked_sub(Duration::from_secs(3))
                .unwrap_or(now),
        };

        stats.refresh_if_due();
        stats
    }

    pub fn refresh_if_due(&mut self) {
        if self.last_refresh.elapsed() < Duration::from_secs(1) {
            return;
        }

        self.last_refresh = Instant::now();

        let Some(pid) = self.pid else {
            return;
        };

        self.system.refresh_processes(
            ProcessesToUpdate::Some(&[pid]),
            true,
        );

        if let Some(process) = self.system.process(pid) {
            self.cpu_usage = process.cpu_usage();
            self.memory_bytes = process.memory();
        }

        if self.last_gpu_refresh.elapsed() >= Duration::from_secs(2) {
            self.last_gpu_refresh = Instant::now();

            if let Some((usage, used, total)) =
                query_nvidia_gpu()
            {
                self.gpu_usage = Some(usage);
                self.gpu_memory_used_mb = Some(used);
                self.gpu_memory_total_mb = Some(total);
            } else if let Some(usage) = query_amd_gpu() {
                self.gpu_usage = Some(usage);
                self.gpu_memory_used_mb = None;
                self.gpu_memory_total_mb = None;
            }
        }
    }

    pub fn cpu_usage(&self) -> f32 {
        self.cpu_usage
    }

    pub fn memory_mb(&self) -> f64 {
        self.memory_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn gpu_usage(&self) -> Option<f32> {
        self.gpu_usage
    }

    pub fn gpu_memory_mb(&self) -> Option<(u64, u64)> {
        self.gpu_memory_used_mb
            .zip(self.gpu_memory_total_mb)
    }

    pub fn uptime_compact(&self) -> String {
        let total = self.started_at.elapsed().as_secs();
        let hours = total / 3600;
        let minutes = (total % 3600) / 60;
        let seconds = total % 60;

        if hours > 0 {
            format!("{hours}h{minutes:02}m")
        } else if minutes > 0 {
            format!("{minutes}m{seconds:02}s")
        } else {
            format!("{seconds}s")
        }
    }
}


fn query_nvidia_gpu() -> Option<(f32, u64, u64)> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .to_string();

    let mut parts = line.split(',').map(str::trim);

    let usage = parts.next()?.parse::<f32>().ok()?;
    let used = parts.next()?.parse::<u64>().ok()?;
    let total = parts.next()?.parse::<u64>().ok()?;

    Some((usage, used, total))
}

fn query_amd_gpu() -> Option<f32> {
    let output = Command::new("rocm-smi")
        .args(["--showuse", "--csv"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);

    for line in text.lines() {
        for value in line.split(',') {
            let clean = value
                .trim()
                .trim_end_matches('%')
                .trim();

            if let Ok(usage) = clean.parse::<f32>() {
                if (0.0..=100.0).contains(&usage) {
                    return Some(usage);
                }
            }
        }
    }

    None
}
