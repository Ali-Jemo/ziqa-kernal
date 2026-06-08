//! CLOCK page eviction policy.
//!
//! Tracks user-mapped physical frames and maintains a "referenced" bit for each.
//! When memory pressure is high, it scans for unreferenced frames to evict
//! (swap out).

// use crate::memory::{paging, swap};
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::structures::paging::PhysFrame;

struct EvictablePage {
    frame: PhysFrame,
    referenced: bool,
}

struct EvictionRegistryInner {
    pages: Vec<EvictablePage>,
    clock_hand: usize,
}

impl EvictionRegistryInner {
    const fn new() -> Self {
        Self {
            pages: Vec::new(),
            clock_hand: 0,
        }
    }

    fn register_page(&mut self, frame: PhysFrame) {
        if !self.pages.iter().any(|p| p.frame == frame) {
            self.pages.push(EvictablePage {
                frame,
                referenced: true,
            });
        }
    }

    fn mark_referenced(&mut self, frame: PhysFrame) {
        if let Some(page) = self.pages.iter_mut().find(|p| p.frame == frame) {
            page.referenced = true;
        }
    }

    fn evict_one(&mut self) -> Option<PhysFrame> {
        let len = self.pages.len();
        if len == 0 {
            return None;
        }

        // CLOCK algorithm
        for _ in 0..(len * 2) {
            let page = &mut self.pages[self.clock_hand];
            if !page.referenced {
                let frame = page.frame;
                // Try to swap out
                if swap::swap_out(frame.start_address().as_u64()).is_some() {
                    self.pages.remove(self.clock_hand);
                    if self.clock_hand >= self.pages.len() {
                        self.clock_hand = 0;
                    }
                    return Some(frame);
                }
            } else {
                page.referenced = false;
            }
            self.clock_hand = (self.clock_hand + 1) % len;
        }
        None
    }
}

lazy_static! {
    static ref REGISTRY: Mutex<EvictionRegistryInner> = Mutex::new(EvictionRegistryInner::new());
}

pub fn register_page(frame: PhysFrame) {
    REGISTRY.lock().register_page(frame);
}

pub fn mark_referenced(frame: PhysFrame) {
    REGISTRY.lock().mark_referenced(frame);
}

pub fn evict_one() -> Option<PhysFrame> {
    REGISTRY.lock().evict_one()
}
