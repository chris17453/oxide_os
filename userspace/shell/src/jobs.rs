//! Shell Job Control — background process tracking
//!
//! — ByteRiot: the overseer. Tracks background processes, their PIDs,
//! states, and command strings. Supports fg/bg/jobs/wait builtins.
//!
//! Job states follow POSIX conventions:
//!   Running — process is executing
//!   Stopped — process received SIGTSTP (Ctrl+Z)
//!   Done    — process exited

extern crate alloc;
use alloc::vec::Vec;

/// Maximum tracked jobs
const MAX_JOBS: usize = 32;

/// Job state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobState {
    Running,
    Stopped,
    Done,
}

/// A tracked background job
#[derive(Debug, Clone)]
pub struct Job {
    /// Job number (1-based)
    pub id: usize,
    /// Process group ID
    pub pgid: i32,
    /// PIDs in the pipeline
    pub pids: Vec<i32>,
    /// Current state
    pub state: JobState,
    /// Command string (for display)
    pub command: Vec<u8>,
}

/// Job table
pub struct JobTable {
    jobs: Vec<Option<Job>>,
    next_id: usize,
}

impl JobTable {
    pub const fn new() -> Self {
        JobTable {
            jobs: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a new job, return its job number
    /// — ByteRiot: accepts a slice for pids because pipeline stages come from
    /// a stack array. Clones into a Vec for the job entry.
    pub fn add(&mut self, pgid: i32, pids: &[i32], command: Vec<u8>) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        let job = Job {
            id,
            pgid,
            pids: pids.to_vec(),
            state: JobState::Running,
            command,
        };

        // Find empty slot or push
        let mut placed = false;
        for slot in self.jobs.iter_mut() {
            if slot.is_none() {
                *slot = Some(job.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            self.jobs.push(Some(job));
        }

        id
    }

    /// Get a job by ID
    pub fn get(&self, id: usize) -> Option<&Job> {
        for slot in &self.jobs {
            if let Some(job) = slot {
                if job.id == id {
                    return Some(job);
                }
            }
        }
        None
    }

    /// — ByteRiot: find a job by its process group ID. Used when a child stops
    /// and we need to look up or create the job entry.
    pub fn find_by_pgid(&self, pgid: i32) -> Option<&Job> {
        for slot in &self.jobs {
            if let Some(job) = slot {
                if job.pgid == pgid {
                    return Some(job);
                }
            }
        }
        None
    }

    /// Mark a job as done and return it
    pub fn mark_done(&mut self, pgid: i32) -> Option<&Job> {
        for slot in self.jobs.iter_mut() {
            if let Some(job) = slot {
                if job.pgid == pgid {
                    job.state = JobState::Done;
                    return Some(job);
                }
            }
        }
        None
    }

    /// — ByteRiot: mark a job as stopped (Ctrl+Z). Called when waitpid returns
    /// WIFSTOPPED — the child got SIGTSTP and is frozen.
    pub fn mark_stopped(&mut self, pgid: i32) {
        for slot in self.jobs.iter_mut() {
            if let Some(job) = slot {
                if job.pgid == pgid {
                    job.state = JobState::Stopped;
                    return;
                }
            }
        }
    }

    /// — ByteRiot: mark a job as running (after fg/bg sends SIGCONT).
    pub fn mark_running(&mut self, pgid: i32) {
        for slot in self.jobs.iter_mut() {
            if let Some(job) = slot {
                if job.pgid == pgid {
                    job.state = JobState::Running;
                    return;
                }
            }
        }
    }

    /// Remove completed jobs
    pub fn reap(&mut self) {
        for slot in self.jobs.iter_mut() {
            if let Some(job) = slot {
                if job.state == JobState::Done {
                    *slot = None;
                }
            }
        }
    }

    /// List all active jobs
    pub fn list(&self) -> Vec<&Job> {
        let mut result = Vec::new();
        for slot in &self.jobs {
            if let Some(job) = slot {
                if job.state != JobState::Done {
                    result.push(job);
                }
            }
        }
        result
    }

    /// — ByteRiot: find the most recent active job (for bare `fg`/`bg`)
    pub fn most_recent(&self) -> Option<&Job> {
        let mut best: Option<&Job> = None;
        for slot in &self.jobs {
            if let Some(job) = slot {
                if job.state != JobState::Done {
                    match best {
                        None => best = Some(job),
                        Some(b) if job.id > b.id => best = Some(job),
                        _ => {}
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    // Tests would go here but this is no_std
}
