//! Brightflow Scheduler - Background job scheduling for data pipelines
//!
//! Manages recurring connector syncs and insights report generation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unique identifier for a scheduled job
pub type JobId = uuid::Uuid;

/// The scheduler manages background jobs
#[derive(Clone)]
pub struct Scheduler {
    jobs: Arc<RwLock<HashMap<JobId, JobEntry>>>,
}

/// Internal entry combining definition and runtime status
struct JobEntry {
    definition: JobDefinition,
    status: JobStatus,
}

/// What to run, when, and with what configuration
#[derive(Debug, Clone)]
pub struct JobDefinition {
    /// Human-readable name for this job
    pub name: String,
    /// What kind of job to run
    pub kind: JobKind,
    /// Cron-like interval in seconds (simplified for v1)
    pub interval_secs: u64,
}

/// The type of work a job performs
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum JobKind {
    /// Run a Longbow connector sync
    ConnectorSync {
        connector: String,
        config_path: String,
    },
    /// Generate an insights report
    InsightsReport {
        report_type: String,
        dataset: String,
    },
}

/// Runtime status of a scheduled job
#[derive(Debug, Clone)]
pub struct JobStatus {
    /// Unique job identifier
    pub id: JobId,
    /// Current state
    pub state: JobState,
    /// When the job last ran (if ever)
    pub last_run: Option<std::time::Instant>,
    /// Result of the last run
    pub last_result: Option<String>,
}

/// Possible states for a job
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum JobState {
    /// Waiting for next scheduled run
    Idle,
    /// Currently executing
    Running,
    /// Disabled by user
    Paused,
}

impl Scheduler {
    /// Create a new scheduler
    #[must_use]
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the scheduler background loop
    ///
    /// This runs forever, ticking every second to check for jobs that need to run.
    pub async fn start(&self) {
        tracing::info!("Scheduler started");
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            self.tick().await;
        }
    }

    /// Add a new job to the scheduler
    pub async fn add_job(&self, definition: JobDefinition) -> JobId {
        let id = JobId::new_v4();
        let entry = JobEntry {
            definition,
            status: JobStatus {
                id,
                state: JobState::Idle,
                last_run: None,
                last_result: None,
            },
        };
        self.jobs.write().await.insert(id, entry);
        tracing::info!("Added job {id}");
        id
    }

    /// Remove a job from the scheduler
    pub async fn remove_job(&self, id: JobId) {
        self.jobs.write().await.remove(&id);
        tracing::info!("Removed job {id}");
    }

    /// List all jobs and their current status
    pub async fn list_jobs(&self) -> Vec<(JobDefinition, JobStatus)> {
        self.jobs
            .read()
            .await
            .values()
            .map(|e| (e.definition.clone(), e.status.clone()))
            .collect()
    }

    /// Trigger a job to run immediately (regardless of schedule)
    pub async fn trigger_now(&self, id: JobId) {
        tracing::info!("Manual trigger for job {id}");
        // TODO: spawn the job execution
        let mut jobs = self.jobs.write().await;
        if let Some(entry) = jobs.get_mut(&id) {
            entry.status.last_run = Some(std::time::Instant::now());
            entry.status.last_result =
                Some("triggered (execution not yet implemented)".to_string());
        }
    }

    /// Internal tick — check if any jobs need to run
    async fn tick(&self) {
        let jobs = self.jobs.read().await;
        for (id, entry) in jobs.iter() {
            if entry.status.state != JobState::Idle {
                continue;
            }
            let should_run = match entry.status.last_run {
                None => true,
                Some(last) => last.elapsed().as_secs() >= entry.definition.interval_secs,
            };
            if should_run {
                tracing::debug!("Job {id} ({}) is due to run", entry.definition.name);
                // TODO: spawn actual job execution
            }
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
