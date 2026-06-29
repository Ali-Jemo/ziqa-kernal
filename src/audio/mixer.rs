//! Mixer controls for audio devices

use alloc::sync::Arc;
use spin::Mutex;

/// Mixer control types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixerControlType {
    Volume,
    Mute,
    Capture,
    Balance,
}

/// Mixer control definition
#[derive(Clone, Copy, Debug)]
pub struct MixerControl {
    pub name: &'static str,
    pub control_type: MixerControlType,
    pub min: i32,
    pub max: i32,
    pub value: i32,
}

/// Mixer with per-channel controls
pub struct Mixer {
    controls: Vec<MixerControl>,
    volumes: [i32; 2],
}

impl Mixer {
    pub const fn new() -> Self {
        Self {
            controls: crate::sync::EMPTY_VEC,
            volumes: [100, 100],
        }
    }
    
    pub fn set_volume(&mut self, channel: usize, value: i32) -> Result<(), ()> {
        let channel = channel.min(1);
        if value < 0 || value > 100 {
            return Err(());
        }
        self.volumes[channel] = value;
        Ok(())
    }
    
    pub fn get_volume(&self, channel: usize) -> i32 {
        self.volumes[channel.min(1)]
    }
    
    pub fn mute(&mut self, channel: usize, mute: bool) {
        let channel = channel.min(1);
        self.volumes[channel] = if mute { 0 } else { 100 };
    }
    
    pub fn add_control(&mut self, control: MixerControl) {
        self.controls.push(control);
    }
    
    pub fn list_controls(&self) -> &[MixerControl] {
        &self.controls
    }
}

lazy_static::lazy_static! {
    pub static ref DEFAULT_MIXER: Mutex<Mixer> = Mutex::new(Mixer::new());
}