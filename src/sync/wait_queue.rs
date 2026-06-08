//! Thread-safe queue with blocking capabilities.
//! Ported and simplified from Redox OS.

use alloc::collections::VecDeque;
use spin::Mutex;
use crate::sync::wait_condition::WaitCondition;

#[derive(Debug)]
pub struct WaitQueue<T> {
    inner: Mutex<VecDeque<T>>,
    pub condition: WaitCondition,
}

impl<T> WaitQueue<T> {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
            condition: WaitCondition::new(),
        }
    }

    /// Send a value to the queue and notify one waiter.
    pub fn send(&self, value: T) {
        self.inner.lock().push_back(value);
        self.condition.notify(); // In our simplified version, notify wakes all
    }

    /// Receive a value from the queue, blocking if empty.
    pub fn receive(&self, reason: &'static str) -> T {
        loop {
            if let Some(value) = self.inner.lock().pop_front() {
                return value;
            }
            self.condition.wait(reason);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}
