//! Display abstraction — selects backend based on features
//
// Normal builds (Redox): uses graphics-ipc/drm via display/display_redox.rs
// ZiqaKernel builds: uses direct BGA framebuffer via display/display_ziqa_bga.rs

#[cfg(not(feature = "ziqa-bga-direct"))]
mod display_redox;

#[cfg(feature = "ziqa-bga-direct")]
mod display_ziqa_bga;

#[cfg(not(feature = "ziqa-bga-direct"))]
pub use display_redox::{Display, Displays, SCALE_BASELINE};

#[cfg(not(feature = "ziqa-bga-direct"))]
pub use graphics_ipc::V2GraphicsHandle;

#[cfg(feature = "ziqa-bga-direct")]
pub use display_ziqa_bga::{Display, Displays, SCALE_BASELINE};
