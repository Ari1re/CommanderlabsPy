//! Background jobs with pollable progress.
//!
//! Long operations (card database update, deck generation, combo lookup, MPC
//! search, AI agent runs) run on worker threads. The UI starts a job, gets an
//! id back immediately, and polls [`Jobs::get`] for progress and the result.
//! Polling works identically for the Tauri app and the web server.
//!
//! Things to work on:
//! - Jobs can't be cancelled (long generations, model downloads). A
//!   cancellation flag checked between steps would cover most of them.

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LOG_LINES: usize = 400;
const KEEP_FINISHED: usize = 50;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobState {
    pub id: u64,
    pub kind: String,
    /// Free-form key the UI uses to find a job again (e.g. the deck name).
    pub subject: String,
    pub status: JobStatus,
    pub percent: u16,
    pub step: String,
    pub detail: String,
    pub log: Vec<String>,
    /// Text produced so far (an AI answer as it's written).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub partial: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub started_ms: u128,
    pub finished_ms: Option<u128>,
}

/// Milliseconds since 1970, for started/finished times.
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    map: Mutex<HashMap<u64, JobState>>,
}

/// Handed to the worker so it can report progress.
#[derive(Clone)]
pub struct JobHandle {
    id: u64,
    jobs: Arc<Jobs>,
}

impl JobHandle {
    /// Move the progress bar. An empty `step` keeps the previous step name. Each
    /// distinct "NN% step — detail" line is also appended to the log, trimmed to
    /// the last `MAX_LOG_LINES`.
    pub fn progress(&self, percent: u16, step: impl Into<String>, detail: impl Into<String>) {
        let step = step.into();
        let detail = detail.into();
        self.jobs.update(self.id, |job| {
            job.percent = percent.min(100);
            if !step.is_empty() {
                job.step = step.clone();
            }
            job.detail = detail.clone();
            let line = if detail.is_empty() {
                format!("{:>3}% {}", job.percent, job.step)
            } else {
                format!("{:>3}% {} — {}", job.percent, job.step, detail)
            };
            if job.log.last() != Some(&line) {
                job.log.push(line);
            }
            if job.log.len() > MAX_LOG_LINES {
                let excess = job.log.len() - MAX_LOG_LINES;
                job.log.drain(0..excess);
            }
        });
    }

    /// Replace the text produced so far (not added to the log).
    pub fn partial(&self, text: &str) {
        self.jobs.update(self.id, |job| {
            job.partial = text.to_string();
        });
    }

    /// Add a raw line to the log (sources used, agent thinking).
    /// TODO: unlike `progress`, this doesn't trim to MAX_LOG_LINES, so a chatty job
    /// can grow its log without limit.
    pub fn log(&self, line: impl Into<String>) {
        let line = line.into();
        self.jobs.update(self.id, |job| {
            job.log.push(line.clone());
        });
    }
}

impl Jobs {
    /// Start `work` on its own thread and return the job id at once. The UI then
    /// polls `get(id)`. Errors and panics both end as `Error` with a message, so a
    /// crashing generator never leaves a job stuck on "Running". Old finished jobs
    /// are pruned here.
    pub fn spawn<F>(self: &Arc<Self>, kind: &str, subject: &str, work: F) -> u64
    where
        F: FnOnce(JobHandle) -> Result<Value> + Send + 'static,
    {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut map = self.map.lock().unwrap();
            map.insert(
                id,
                JobState {
                    id,
                    kind: kind.to_string(),
                    subject: subject.to_string(),
                    status: JobStatus::Running,
                    percent: 0,
                    step: "Starting".to_string(),
                    detail: String::new(),
                    log: Vec::new(),
                    partial: String::new(),
                    result: None,
                    error: None,
                    started_ms: now_ms(),
                    finished_ms: None,
                },
            );
            Self::prune(&mut map);
        }

        let handle = JobHandle { id, jobs: self.clone() };
        let jobs = self.clone();
        thread::spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(handle)));
            jobs.update(id, |job| {
                job.finished_ms = Some(now_ms());
                match outcome {
                    Ok(Ok(value)) => {
                        job.status = JobStatus::Done;
                        job.percent = 100;
                        job.result = Some(value);
                    }
                    Ok(Err(err)) => {
                        job.status = JobStatus::Error;
                        job.error = Some(format!("{err:#}"));
                    }
                    Err(_) => {
                        job.status = JobStatus::Error;
                        job.error = Some("The task crashed unexpectedly.".to_string());
                    }
                }
            });
        });

        id
    }

    /// Change one job under the lock. Unknown (pruned) ids are ignored.
    fn update(&self, id: u64, f: impl FnOnce(&mut JobState)) {
        if let Some(job) = self.map.lock().unwrap().get_mut(&id) {
            f(job);
        }
    }

    /// Full job state, including the result, for the UI's polling.
    pub fn get(&self, id: u64) -> Option<JobState> {
        self.map.lock().unwrap().get(&id).cloned()
    }

    /// Running jobs plus recently finished ones, newest first, without the
    /// (possibly large) results.
    pub fn list(&self) -> Vec<JobState> {
        let mut jobs: Vec<JobState> = self
            .map
            .lock()
            .unwrap()
            .values()
            .map(|job| JobState {
                result: None,
                log: job.log.iter().rev().take(3).rev().cloned().collect(),
                ..job.clone()
            })
            .collect();
        jobs.sort_by(|a, b| b.id.cmp(&a.id));
        jobs
    }

    /// Is the same task already running (same kind and deck/commander)? The API
    /// uses this to reattach instead of starting a duplicate.
    pub fn running(&self, kind: &str, subject: &str) -> Option<u64> {
        self.map
            .lock()
            .unwrap()
            .values()
            .find(|job| job.kind == kind && job.subject == subject && job.status == JobStatus::Running)
            .map(|job| job.id)
    }

    /// Keep only the newest `KEEP_FINISHED` finished jobs. Running ones always
    /// stay.
    fn prune(map: &mut HashMap<u64, JobState>) {
        let mut finished: Vec<u64> = map
            .values()
            .filter(|job| job.status != JobStatus::Running)
            .map(|job| job.id)
            .collect();
        if finished.len() > KEEP_FINISHED {
            finished.sort();
            for id in &finished[..finished.len() - KEEP_FINISHED] {
                map.remove(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn job_reports_progress_and_result() {
        let jobs = Arc::new(Jobs::default());
        let id = jobs.spawn("test", "x", |h| {
            h.progress(50, "half", "detail");
            Ok(serde_json::json!({"ok": true}))
        });
        for _ in 0..100 {
            if jobs.get(id).unwrap().status != JobStatus::Running {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let job = jobs.get(id).unwrap();
        assert_eq!(job.status, JobStatus::Done);
        assert_eq!(job.result.unwrap()["ok"], true);
        assert!(job.log.iter().any(|l| l.contains("half")));
    }

    #[test]
    fn job_errors_are_captured() {
        let jobs = Arc::new(Jobs::default());
        let id = jobs.spawn("test", "x", |_| anyhow::bail!("boom"));
        for _ in 0..100 {
            if jobs.get(id).unwrap().status != JobStatus::Running {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(jobs.get(id).unwrap().error.as_deref(), Some("boom"));
    }
}
