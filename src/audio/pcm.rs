//! PCM (Pulse Code Modulation) stream handling
//!
//! Ring buffer implementation for audio streaming with format support.

use alloc::vec::Vec;
use alloc::sync::Arc;
use spin::Mutex;

/// PCM format descriptions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmFormat {
    /// Signed 16-bit little-endian
    S16Le,
    /// Unsigned 8-bit
    U8,
    /// Signed 24-bit in 32-bit container
    S24_32,
    /// Signed 32-bit little-endian (float in userspace, integer in kernel)
    S32Le,
}

impl PcmFormat {
    /// Bytes per sample for this format
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            PcmFormat::S16Le => 2,
            PcmFormat::U8 => 1,
            PcmFormat::S24_32 => 4,
            PcmFormat::S32Le => 4,
        }
    }
    
    /// Preferred format for hardware (closest match to hardware capability)
    pub fn hardware_preferred(&self) -> PcmFormat {
        // Most hardware prefers S16 or S32
        match self {
            PcmFormat::S16Le => PcmFormat::S16Le,
            PcmFormat::U8 => PcmFormat::S16Le, // Upsample to 16-bit
            PcmFormat::S24_32 => PcmFormat::S32Le,
            PcmFormat::S32Le => PcmFormat::S32Le,
        }
    }
}

/// PCM stream with ring buffer
pub struct PcmStream {
    /// Buffer in bytes (rounded to sector size)
    buffer: Vec<u8>,
    /// Read position (bytes)
    read_pos: usize,
    /// Write position (bytes)
    write_pos: usize,
    /// Format
    format: PcmFormat,
    /// Sample rate
    sample_rate: u32,
    /// Number of channels
    channels: u8,
    /// Buffer size in frames
    buffer_frames: usize,
}

impl PcmStream {
    /// Create a new PCM stream with the given buffer size
    pub fn new(buffer_frames: usize, format: PcmFormat, sample_rate: u32, channels: u8) -> Self {
        let buffer_bytes = buffer_frames * format.bytes_per_sample() * channels as usize;
        Self {
            buffer: alloc::vec![0u8; buffer_bytes],
            read_pos: 0,
            write_pos: 0,
            format,
            sample_rate,
            channels,
            buffer_frames,
        }
    }
    
    /// Samples available for reading
    pub fn readable_frames(&self) -> usize {
        let readable_bytes = (self.write_pos + self.buffer.len() - self.read_pos) % self.buffer.len();
        readable_bytes / self.format.bytes_per_sample() / self.channels as usize
    }
    
    /// Space available for writing
    pub fn writable_frames(&self) -> usize {
        let writable_bytes = (self.read_pos + self.buffer.len() - self.write_pos - 1) % self.buffer.len();
        writable_bytes / self.format.bytes_per_sample() / self.channels as usize
    }
    
    /// Write interleaved samples (blocking)
    pub fn write(&mut self, samples: &[i16]) -> usize {
        let writable = self.writable_frames().min(samples.len() / self.channels as usize);
        if writable == 0 {
            return 0;
        }
        
        let bytes_to_write = writable * self.format.bytes_per_sample() * self.channels as usize;
        // TODO: format conversion
        // For now, just write S16LE directly
        let bytes_written = self.write_at(samples, bytes_to_write);
        bytes_written / self.format.bytes_per_sample()
    }
    
    fn write_at(&mut self, samples: &[i16], bytes: usize) -> usize {
        let mut remaining = bytes;
        let mut src_offset = 0;
        
        while remaining > 0 {
            let avail = (self.buffer.len() - self.write_pos).min(remaining);
            self.buffer[self.write_pos..self.write_pos + avail].copy_from_slice(unsafe {
                core::slice::from_raw_parts(
                    samples.as_ptr() as *const u8,
                    bytes
                )
            }.split_at(src_offset).1.split_at(avail).0);
            // Simplified - actual implementation would interleave samples
            self.write_pos = (self.write_pos + avail) % self.buffer.len();
            src_offset += avail;
            remaining -= avail;
        }
        
        bytes
    }
}