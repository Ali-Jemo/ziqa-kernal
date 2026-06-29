//! Common utilities ported from Redox OS.
//!
//! - `int_like!` macro (`#[macro_export]`, available at crate root): type-safe
//!   integer wrappers with `Atomic*` variants.
//! - `aligned_box`: heap allocation with custom alignment (`AlignedBox<T, ALIGN>`).

pub mod aligned_box;
