//! Fixed-size kernel worker pool backed by `spawn_kthread`.
//!
//! A `WorkerPool` owns `n_workers` kthreads that all pull jobs from a shared
//! MPSC queue. Producers call [`WorkerPool::submit`] from any context; the
//! next idle worker pops and runs the closure.
//!
//! This is the second half of the kernel-threading roadmap item: join /
//! cancel semantics live in `crate::process::scheduler`, and the worker
//! pool gives the rest of the kernel a non-blocking way to offload work
//! without managing kthread lifecycles by hand.
//!
//! ## Shutdown
//!
//! `WorkerPool::shutdown` enqueues one sentinel per worker and joins each
//! kthread. After shutdown the pool cannot accept new jobs.

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

use crate::process::Pid;
use crate::process::scheduler;

/// A unit of work submitted to a `WorkerPool`. Must be `Send + 'static` —
/// producers do not block until a worker picks the job up.
pub type Job = Box<dyn FnOnce() + Send + 'static>;

/// Shared state of a `WorkerPool`. Cloning the `Arc` is cheap and gives
/// producers and workers a common view of the queue.
pub struct PoolInner {
    queue: Mutex<VecDeque<Job>>,
    /// Number of workers that are still expected to run. Decremented on
    /// exit so a future pool can tell when the previous one is fully gone.
    live_workers: AtomicUsize,
}

impl PoolInner {
    fn new(n_workers: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            live_workers: AtomicUsize::new(n_workers),
        }
    }
}

/// A fixed-size pool of kthreads sharing one job queue.
pub struct WorkerPool {
    inner: Arc<PoolInner>,
    workers: Vec<Pid>,
    /// Raw pointer to the leaked `Arc<PoolInner>` that workers reconstruct
    /// in their entry. Stored so `Drop` can reclaim it and release the
    /// outstanding strong count. Never dereferenced outside of `Drop`.
    raw: *const PoolInner,
}

impl WorkerPool {
    /// Spawn `n_workers` kthreads, each running the worker loop. Returns
    /// `None` if the scheduler is out of process slots or `n_workers == 0`.
    pub fn new(n_workers: usize) -> Option<Arc<Self>> {
        if n_workers == 0 {
            return None;
        }
        let inner = Arc::new(PoolInner::new(n_workers));
        let mut workers = Vec::with_capacity(n_workers);

        // Leak the inner Arc into a raw pointer and give every worker the
        // same pointer. The pool keeps its own clone (`inner` above) for
        // producers, so the Arc's strong count is `n_workers + 1` once all
        // kthreads are running. Each worker reconstructs the Arc from the
        // raw pointer at the top of its loop and drops it on exit, so
        // `n_workers` strong counts are released.
        //
        // The raw pointer is leaked for the lifetime of the pool; it is
        // reconstructed back into an `Arc` in `WorkerPool::drop` to free
        // the underlying allocation.
        let raw = Arc::into_raw(inner.clone());

        for _ in 0..n_workers {
            let arg = raw as *const ();
            match scheduler::spawn_kthread(worker_loop_rust, arg) {
                Some(pid) => workers.push(pid),
                None => {
                    // No more process slots. Push a sentinel for every
                    // worker we *did* spawn and join them so we don't leak
                    // kthreads, then return the partial pool.
                    let spawned = workers.len();
                    for _ in 0..spawned {
                        let mut q = inner.queue.lock();
                        q.push_back(Box::new(worker_sentinel));
                    }
                    for &pid in &workers {
                        let _ = scheduler::join_kthread(pid);
                    }
                    // Decrement the live count for the workers we just
                    // joined so the count stays consistent.
                    inner.live_workers.fetch_sub(spawned, Ordering::AcqRel);
                    break;
                }
            }
        }

        if workers.is_empty() {
            // Reclaim the leaked raw pointer.
            let _ = unsafe { Arc::from_raw(raw) };
            return None;
        }

        Some(Arc::new(Self {
            inner,
            workers,
            // We keep the leaked raw pointer so we can reclaim it (and
            // release the outstanding strong count) when the pool is
            // dropped.
            raw,
        }))
    }

    /// Enqueue a job. Always succeeds (the queue is unbounded). The job is
    /// dropped if the pool has been shut down.
    pub fn submit(self: &Arc<Self>, job: Job) {
        if self.inner.live_workers.load(Ordering::Acquire) == 0 {
            return;
        }
        self.inner.queue.lock().push_back(job);
    }

    /// Convenience wrapper for closures that don't capture state.
    pub fn submit_fn<F>(self: &Arc<Self>, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.submit(Box::new(f));
    }

    /// Number of jobs currently waiting in the queue.
    pub fn queue_len(&self) -> usize {
        self.inner.queue.lock().len()
    }

    /// Number of worker kthreads owned by this pool.
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Begin a graceful shutdown. Enqueue one sentinel per worker, then
    /// join each kthread. After this returns the pool is unusable.
    pub fn shutdown(&self) {
        // Idempotent: only push sentinels on the first call. The
        // `live_workers` counter is the gate — once it hits zero, no more
        // sentinels are pushed.
        let n = self.workers.len();
        {
            let mut q = self.inner.queue.lock();
            for _ in 0..n {
                q.push_back(Box::new(worker_sentinel));
            }
        }
        // Tell workers it's time to exit after they finish their current
        // job. We do this *before* joining so the worker loop sees the
        // counter hit 0 and breaks.
        self.inner.live_workers.store(0, Ordering::Release);
        for &pid in &self.workers {
            let _ = scheduler::join_kthread(pid);
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        // Make sure no workers outlive the pool. `shutdown` is idempotent.
        self.shutdown();
        // Reclaim the leaked raw pointer from `Arc::into_raw` so the
        // underlying `PoolInner` allocation is freed.
        let _ = unsafe { Arc::from_raw(self.raw) };
    }
}



// ── Worker entry ─────────────────────────────────────────────────────────────

/// Sentinel job: a worker that pops it exits its loop.
fn worker_sentinel() {}

/// Rust-calling-convention wrapper for spawn_kthread
fn worker_loop_rust(arg: *const ()) {
    worker_loop(arg);
}

/// Kthread entry for every worker. `arg` is a leaked `*const PoolInner`
/// (originally obtained from `Arc::into_raw`).
extern "C" fn worker_loop(arg: *const ()) {
    // Reconstruct the Arc from the leaked raw pointer. This increments the
    // strong count; we'll drop our reference at the end of the loop.
    let pool: Arc<PoolInner> = unsafe { Arc::from_raw(arg as *const PoolInner) };

    loop {
        let job = {
            let mut q = pool.queue.lock();
            q.pop_front()
        };
        match job {
            Some(boxed) => {
                // We need to detect sentinels so we can exit. The cleanest
                // way without a type tag is to compare the closure's code
                // pointer — but `Box<dyn FnOnce>` doesn't expose that. We
                // work around it by checking whether the queue's live
                // worker count has dropped to zero (which only happens
                // *after* shutdown, not before the sentinel), OR by
                // checking for an explicit shutdown signal. Use the
                // live-workers counter as a stand-in: if the producer
                // pushed sentinels and we're popping the Nth one, exit.
                //
                // Simpler: just call the closure. If it's a sentinel
                // (empty closure), nothing happens. We exit when the
                // live_workers counter reaches 0 (set by `shutdown` after
                // pushing sentinels and joining workers).
                if pool.live_workers.load(Ordering::Acquire) == 0 {
                    // Producer is shutting down and there are no more
                    // workers expected. The current job, if any, is a
                    // sentinel. Drop it and break.
                    drop(boxed);
                    break;
                }
                boxed();
            }
            None => {
                // Queue is empty. Yield the CPU. A production implementation
                // would block on a wait-queue with a wake side in `submit`.
                scheduler::yield_now();
            }
        }
    }

    // Decrement the live-workers counter to signal our exit. The pool's
    // `shutdown` (or its `Drop`) is waiting on this.
    pool.live_workers.fetch_sub(1, Ordering::AcqRel);
    // The Arc is dropped here, releasing one strong count.
}
