//! Compile-time deadlock prevention via ordered lock levels.
//!
//! Use `sync::Mutex<L, T>` and `sync::RwLock<L, T>` instead of `spin::Mutex<T>`
//! when you want to enforce lock ordering at compile time.
//!
//! # Example
//! ```ignore
//! use crate::sync::{Mutex, RwLock, L0, L1, CleanLockToken};
//!
//! static FOO: Mutex<L0, i32> = Mutex::new(0);
//! static BAR: RwLock<L1, i32> = RwLock::new(0);
//!
//! fn example() {
//!     // Entry point: create a root token (unsafe — breaks ordering guarantee)
//!     let mut token = unsafe { CleanLockToken::new() };
//!
//!     let foo = FOO.lock(token.token());  // L0
//!     // BAR.write(token.downgrade());     // L1 — would compile error, L0 ≱ L1
//!     let bar = BAR.read(token.token());  // L0 — ok, L0 < L1
//! }
//! ```
pub use self::ordered::*;
pub use self::wait_condition::WaitCondition;
pub use self::wait_queue::WaitQueue;

pub mod ordered;
pub mod wait_condition;
pub mod wait_queue;
