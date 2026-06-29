//! Audio subsystem for ZiqaKernel
//!
//! Provides PCM streams, mixer controls, and format conversion.
//! Designed for HDA, AC97, and USB Audio hardware backends.

mod pcm;
mod mixer;
mod format;

pub use pcm::*;
pub use mixer::*;
pub use format::*;

use alloc::sync::Arc;
use spin::Mutex;

/// Audio device trait - hardware backends implement this
pub trait AudioDevice: Send + Sync {
    /// Get device name
    fn name(&self) -> &str;
    
    /// Get number of playback channels
    fn channels(&self) -> u8;
    
    /// Get sample rate
    fn sample_rate(&self) -> u32;
    
    /// Get current hardware pointer position (0-indexed sample)
    fn position(&self) -> usize;
    
    /// Write samples to hardware buffer (blocking)
    fn write_samples(&self, samples: &[i16]) -> usize;
    
    /// Read samples from hardware buffer (blocking)
    fn read_samples(&self, buf: &mut [i16]) -> usize;
    
    /// Set sample format
    fn set_format(&self, format: PcmFormat) -> Result<(), ()>;
    
    /// Set volume (0.0 to 1.0)
    fn set_volume(&self, channel: u8, volume: f32);
    
    /// Get volume (0.0 to 1.0)
    fn get_volume(&self, channel: u8) -> f32;
    
    /// Start playback/capture
    fn start(&self) -> Result<(), ()>;
    
    /// Stop playback/capture
    fn stop(&self) -> Result<(), ()>;
    
    /// Handle hardware interrupt (called from IRQ handler)
    fn interrupt_handler(&self);
}

/// Global audio registry
pub struct AudioRegistry {
    devices: Vec<Arc<Mutex<dyn AudioDevice>>>,
}

impl AudioRegistry {
    pub const fn new() -> Self {
        Self {
            devices: crate::sync::EMPTY_VEC,
        }
    }
    
    pub fn register(&mut self, device: Arc<Mutex<dyn AudioDevice>>) {
        self.devices.push(device);
    }
    
    pub fn list(&self) -> &[Arc<Mutex<dyn AudioDevice>>] {
        &self.devices
    }
}

lazy_static::lazy_static! {
    pub static ref AUDIO_REGISTRY: Mutex<AudioRegistry> = Mutex::new(AudioRegistry::new());
}