//! Scheduling groups
//!
//! Groups allow organizing tasks for hierarchical scheduling.
//! Tasks in a group share CPU bandwidth allocation.

extern crate alloc;

use alloc::vec::Vec;
use sched_traits::{CpuSet, Pid};
use spin::Mutex;

/// Scheduling group ID
pub type GroupId = u32;

/// Predefined group IDs
pub mod group_id {
    use super::GroupId;

    /// Foreground group - interactive tasks
    pub const FOREGROUND: GroupId = 1;
    /// Background group - batch tasks
    pub const BACKGROUND: GroupId = 2;
}

/// Scheduling group
///
/// Groups allow hierarchical bandwidth control. Tasks in a group
/// share the group's allocated bandwidth.
pub struct SchedGroup {
    /// Group ID
    id: GroupId,
    /// Group name (for debugging)
    name: &'static str,
    /// Relative weight for bandwidth allocation
    weight: u64,
    /// Maximum CPU percentage (0-100, 0 = no limit)
    cpu_cap: u32,
    /// CPUs this group can use
    cpu_set: CpuSet,
    /// Tasks in this group
    tasks: Mutex<Vec<Pid>>,
    /// Group's accumulated vruntime (for group-level fairness)
    vruntime: Mutex<u64>,
}

impl SchedGroup {
    /// Create a new scheduling group
    pub const fn new(id: GroupId, name: &'static str, weight: u64, cpu_cap: u32) -> Self {
        Self {
            id,
            name,
            weight,
            cpu_cap,
            cpu_set: CpuSet::all(),
            tasks: Mutex::new(Vec::new()),
            vruntime: Mutex::new(0),
        }
    }

    /// Create a new scheduling group with CPU set
    pub fn with_cpu_set(
        id: GroupId,
        name: &'static str,
        weight: u64,
        cpu_cap: u32,
        cpu_set: CpuSet,
    ) -> Self {
        Self {
            id,
            name,
            weight,
            cpu_cap,
            cpu_set,
            tasks: Mutex::new(Vec::new()),
            vruntime: Mutex::new(0),
        }
    }

    /// Get group ID
    pub fn id(&self) -> GroupId {
        self.id
    }

    /// Get group name
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Get group weight
    pub fn weight(&self) -> u64 {
        self.weight
    }

    /// Get CPU cap (0 = no limit)
    pub fn cpu_cap(&self) -> u32 {
        self.cpu_cap
    }

    /// Get allowed CPU set
    pub fn cpu_set(&self) -> &CpuSet {
        &self.cpu_set
    }

    /// Add a task to this group
    pub fn add_task(&self, pid: Pid) {
        let mut tasks = self.tasks.lock();
        if !tasks.contains(&pid) {
            tasks.push(pid);
        }
    }

    /// Remove a task from this group
    pub fn remove_task(&self, pid: Pid) {
        let mut tasks = self.tasks.lock();
        tasks.retain(|&p| p != pid);
    }

    /// Get number of tasks in group
    pub fn task_count(&self) -> usize {
        self.tasks.lock().len()
    }

    /// Check if a task is in this group
    pub fn contains(&self, pid: Pid) -> bool {
        self.tasks.lock().contains(&pid)
    }

    /// Get group vruntime
    pub fn vruntime(&self) -> u64 {
        *self.vruntime.lock()
    }

    /// Update group vruntime
    pub fn update_vruntime(&self, delta: u64) {
        let mut vr = self.vruntime.lock();
        *vr = vr.saturating_add(delta);
    }

    /// Check if task can run on the given CPU based on group affinity
    pub fn can_run_on(&self, cpu: u32) -> bool {
        self.cpu_set.is_set(cpu)
    }
}

impl core::fmt::Debug for SchedGroup {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SchedGroup")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("weight", &self.weight)
            .field("cpu_cap", &self.cpu_cap)
            .field("task_count", &self.task_count())
            .finish()
    }
}

/// Global scheduling groups
pub struct SchedGroups {
    /// Foreground group - high priority interactive tasks
    pub foreground: SchedGroup,
    /// Background group - lower priority batch tasks
    pub background: SchedGroup,
}

impl SchedGroups {
    /// Create default scheduling groups
    pub fn new() -> Self {
        Self {
            foreground: SchedGroup::new(
                group_id::FOREGROUND,
                "foreground",
                1024, // Same as nice 0
                0,    // No CPU cap
            ),
            background: SchedGroup::with_cpu_set(
                group_id::BACKGROUND,
                "background",
                256, // 1/4 of foreground weight
                50,  // 50% CPU cap
                CpuSet::all(),
            ),
        }
    }

    /// Get a group by ID
    pub fn get(&self, id: GroupId) -> Option<&SchedGroup> {
        match id {
            group_id::FOREGROUND => Some(&self.foreground),
            group_id::BACKGROUND => Some(&self.background),
            _ => None,
        }
    }

    /// Move a task to a group
    pub fn set_task_group(&self, pid: Pid, group_id: GroupId) {
        // Remove from all groups first
        self.foreground.remove_task(pid);
        self.background.remove_task(pid);

        // Add to new group
        match group_id {
            group_id::FOREGROUND => self.foreground.add_task(pid),
            group_id::BACKGROUND => self.background.add_task(pid),
            _ => {}
        }
    }

    /// Get the group a task belongs to
    pub fn get_task_group(&self, pid: Pid) -> Option<GroupId> {
        if self.foreground.contains(pid) {
            Some(group_id::FOREGROUND)
        } else if self.background.contains(pid) {
            Some(group_id::BACKGROUND)
        } else {
            None
        }
    }
}

impl Default for SchedGroups {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_task_management() {
        let group = SchedGroup::new(1, "test", 1024, 0);

        group.add_task(1);
        group.add_task(2);

        assert!(group.contains(1));
        assert!(group.contains(2));
        assert_eq!(group.task_count(), 2);

        group.remove_task(1);
        assert!(!group.contains(1));
        assert_eq!(group.task_count(), 1);
    }

    #[test]
    fn test_groups_management() {
        let groups = SchedGroups::new();

        groups.set_task_group(1, group_id::FOREGROUND);
        groups.set_task_group(2, group_id::BACKGROUND);

        assert_eq!(groups.get_task_group(1), Some(group_id::FOREGROUND));
        assert_eq!(groups.get_task_group(2), Some(group_id::BACKGROUND));

        // Move task 1 to background
        groups.set_task_group(1, group_id::BACKGROUND);
        assert_eq!(groups.get_task_group(1), Some(group_id::BACKGROUND));
    }
}
