//! Event primitives for process synchronization AND the event notification
//! scheme (fevent/event: URLs, inspired by Redox OS).
//!
//! Two layers live here:
//! 1. `Event` — simple binary signal; a process waits on it until another signals it.
//! 2. `IoEvent` / `EventQueue` / registry — the multi-producer, multi-consumer
//!    notification bus that backs the `event:` scheme.

use crate::process::{Pid, scheduler};
use crate::sync::WaitQueue;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

// ── Layer 1: Binary Event ─────────────────────────────────────────────────────

/// Event that processes can block on until signaled.
pub struct Event {
    waiters: Mutex<Vec<Pid>>,
    pub id: u64,
    pub signaled: Mutex<bool>,
}

impl Event {
    pub fn new(id: u64) -> Self {
        Self {
            waiters: Mutex::new(Vec::new()),
            id,
            signaled: Mutex::new(false),
        }
    }

    /// Block the current process until this event is signaled.
    pub fn wait(&self) {
        if *self.signaled.lock() {
            return; // Already signaled
        }
        let pid = scheduler::SCHEDULER
            .current_pid()
            .expect("No current process");
        self.waiters.lock().push(pid);
        scheduler::SCHEDULER.block_current_task();
    }

    /// Signal all processes waiting on this event to wake up.
    pub fn signal(&self) {
        *self.signaled.lock() = true;
        let mut waiters = self.waiters.lock();
        for pid in waiters.drain(..) {
            if let Some(proc_arc) = scheduler::SCHEDULER.get_process(pid) {
                let mut proc = proc_arc.lock();
                proc.set_state_ready();
                scheduler::SCHEDULER
                    .ready_queues
                    .lock()
                    .push(proc.pid, proc.vruntime);
            }
        }
    }

    pub fn reset(&self) {
        *self.signaled.lock() = false;
    }
}

// ── Layer 2: Event Notification Bus (fevent / event: scheme) ─────────────────
// ── Event flags (bitflags style, kept simple for no_std) ──────────────────────

pub const EVENT_READ: u64 = 1 << 0;
pub const EVENT_WRITE: u64 = 1 << 1;

/// A single notification delivered through the event bus.
#[derive(Debug, Clone, Copy)]
pub struct IoEvent {
    /// Opaque identifier of the source file/number.
    pub id: u64,
    /// Which flags triggered (bitmask of EVENT_READ | EVENT_WRITE, etc.).
    pub flags: u64,
    /// User-supplied cookie returned alongside the notification.
    pub data: u64,
}

/// A queue that accumulates [`IoEvent`]s for a consumer to drain.
pub struct EventQueue {
    pub id: u64,
    queue: WaitQueue<IoEvent>,
}

impl EventQueue {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            queue: WaitQueue::new(),
        }
    }

    /// Push an event into the queue, waking any blocked reader.
    pub fn trigger(&self, event: IoEvent) {
        self.queue.send(event);
    }

    /// Block until an event is available, then return it.
    pub fn receive(&self) -> IoEvent {
        self.queue.receive("EventQueue::receive")
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// Key used in the event registry: (scheme_name, file_number).
type RegKey = (String, u64);

/// Value stored per registration.
#[derive(Clone)]
struct RegEntry {
    queue_id: u64,
    flags: u64,
    data: u64,
}

/// Maps (scheme, number) → list of interested event queues.
static REGISTRY: Mutex<BTreeMap<RegKey, Vec<RegEntry>>> = Mutex::new(BTreeMap::new());

/// All active event queues, indexed by their globally-unique id.
static QUEUES: Mutex<BTreeMap<u64, Arc<EventQueue>>> = Mutex::new(BTreeMap::new());

static NEXT_QUEUE_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate a fresh, globally-unique event-queue id.
pub fn next_queue_id() -> u64 {
    NEXT_QUEUE_ID.fetch_add(1, Ordering::SeqCst)
}

/// Insert a newly created queue into the global set.
pub fn insert_queue(id: u64, queue: Arc<EventQueue>) {
    QUEUES.lock().insert(id, queue);
}

/// Remove and return a queue by id.
pub fn remove_queue(id: u64) -> Option<Arc<EventQueue>> {
    QUEUES.lock().remove(&id)
}

/// Look up a queue by id (cloned Arc so the caller can use it lock-free).
pub fn get_queue(id: u64) -> Option<Arc<EventQueue>> {
    QUEUES.lock().get(&id).cloned()
}

/// Register interest: when `scheme:number` produces events matching `flags`,
/// deliver an [`IoEvent`] into queue `queue_id` with the given `data` cookie.
pub fn register(
    scheme: &str,
    number: u64,
    queue_id: u64,
    flags: u64,
    data: u64,
) {
    let key = (String::from(scheme), number);
    let entry = RegEntry {
        queue_id,
        flags,
        data,
    };
    REGISTRY.lock().entry(key.clone()).or_default().push(entry);
    
    // Sync with the scheme to trigger immediately if already ready
    let ready_flags = {
        let registry = crate::scheme::SCHEME_REGISTRY.lock();
        if let Some(s) = registry.get(scheme) {
            s.fevent(number as usize, flags as usize).unwrap_or(0) as u64
        } else {
            0
        }
    };
    
    if ready_flags > 0 {
        trigger(scheme, number, ready_flags);
    }
}

/// Unregister all entries for a given file (called when a file is closed).
pub fn unregister_file(scheme: &str, number: u64) {
    let key = (String::from(scheme), number);
    REGISTRY.lock().remove(&key);
}

/// Trigger events for all queues registered on `scheme:number` whose
/// registration flags intersect with the given `flags`.
///
/// Every interested queue receives one [`IoEvent`] per registration entry.
pub fn trigger(scheme: &str, number: u64, flags: u64) {
    let entries: Vec<RegEntry> = {
        let reg = REGISTRY.lock();
        let key = (String::from(scheme), number);
        reg.get(&key).cloned().unwrap_or_default()
    };

    for entry in &entries {
        let common = flags & entry.flags;
        if common != 0 {
            if let Some(queue) = get_queue(entry.queue_id) {
                queue.trigger(IoEvent {
                    id: number,
                    flags: common,
                    data: entry.data,
                });
            }
        }
    }
}
