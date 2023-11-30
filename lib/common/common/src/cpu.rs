use tokio::sync::OwnedSemaphorePermit;

/// Try to read number of CPUs from environment variable `QDRANT_NUM_CPUS`.
/// If it is not set, use `num_cpus::get()`.
pub fn get_num_cpus() -> usize {
    match std::env::var("QDRANT_NUM_CPUS") {
        Ok(val) => {
            let num_cpus = val.parse::<usize>().unwrap_or(0);
            if num_cpus > 0 {
                num_cpus
            } else {
                num_cpus::get()
            }
        }
        Err(_) => num_cpus::get(),
    }
}

/// CPU permit, used to limit number of concurrent CPU-intensive operations
///
/// This permit represents the number of CPUs allocated for an operation, so that the operation can
/// respect other parallel workloads. When dropped or `release()`-ed, the CPUs are given back for
/// other tasks to acquire.
///
/// These CPU permits are used to better balance and saturate resource utilization.
pub struct CpuPermit {
    /// Number of CPUs acquired in this permit.
    pub num_cpus: u32,
    /// Semaphore permit.
    permit: Option<OwnedSemaphorePermit>,
}

impl CpuPermit {
    /// New CPU permit with given CPU count and permit semaphore.
    pub fn new(count: u32, permit: OwnedSemaphorePermit) -> Self {
        Self {
            num_cpus: count,
            permit: Some(permit),
        }
    }

    /// New CPU permit with given CPU count without a backing semaphore for a shared pool.
    pub fn dummy(count: u32) -> Self {
        Self {
            num_cpus: count,
            permit: None,
        }
    }

    /// Release CPU permit, giving them back to the semaphore.
    pub fn release(&mut self) {
        self.permit.take();
    }
}

impl Drop for CpuPermit {
    fn drop(&mut self) {
        self.release();
    }
}
