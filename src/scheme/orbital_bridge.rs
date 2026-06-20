//! Redox Orbital Scheme Protocol Implementation for ZiqaKernel
//!
//! This module implements the display_v2 and input schemes that Orbital expects.
//! The protocol uses string commands over the scheme file descriptor:
//!   - "O" = new window with args: x, y, width, height, title
//!   - "F" = flip/flush
//!   - "A,1/0" = async mode
//!   - "M,C,1/0" = mouse cursor visibility
//!   - "P,x,y" = position
//!   - "S,w,h" = resize
//!   - "T,title" = title change
//!   - "Y,x,y,w,h" = damage/flush (damage sync)

use crate::scheme::{Scheme, SchemeResult};

use core::sync::atomic::{AtomicI32, AtomicU8, AtomicUsize, Ordering};
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// ZiqaKernel's simplified Event structure (matches Redox syscall::data::Event)
#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct Event {
    pub id: usize,
    pub flags: usize,
    pub data: usize,
}

/// Single window state for Orbital
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub async_mode: bool,
    pub mouse_cursor: bool,
    pub mouse_grab: bool,
    pub mouse_relative: bool,
    pub events: VecDeque<Event>,
}

impl WindowState {
    pub fn new(x: i32, y: i32, width: u32, height: u32, title: String) -> Self {
        Self {
            x, y, width, height, title,
            async_mode: false,
            mouse_cursor: true,
            mouse_grab: false,
            mouse_relative: false,
            events: VecDeque::new(),
        }
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push_back(event);
    }

    pub fn pop_events(&mut self, buf: &mut [Event]) -> usize {
        let mut count = 0;
        for dest in buf.iter_mut() {
            match self.events.pop_front() {
                Some(ev) => {
                    *dest = ev;
                    count += 1;
                }
                None => break,
            }
        }
        count
    }
}

/// Global window registry
pub static WINDOWS: Mutex<alloc::collections::BTreeMap<usize, Arc<Mutex<WindowState>>>> = 
    Mutex::new(alloc::collections::BTreeMap::new());

/// OrbitalBridge handles both display_v2 and input schemes
pub struct OrbitalBridge {
    next_handle: AtomicUsize,
    next_window_id: AtomicUsize,
    last_mouse_x: AtomicI32,
    last_mouse_y: AtomicI32,
    last_mouse_btn: AtomicU8,
}

impl OrbitalBridge {
    pub const fn new() -> Self {
        Self {
            next_handle: AtomicUsize::new(1),
            next_window_id: AtomicUsize::new(1),
            last_mouse_x: AtomicI32::new(0),
            last_mouse_y: AtomicI32::new(0),
            last_mouse_btn: AtomicU8::new(0),
        }
    }
}

impl Scheme for OrbitalBridge {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
        
        if path.starts_with("display_v2:") || path.starts_with("display:") {
            let inner = if path.starts_with("display_v2:") {
                path.trim_start_matches("display_v2:")
            } else {
                path.trim_start_matches("display:")
            };
            if let Some((title, rest)) = inner.split_once(':') {
                let parts: Vec<&str> = rest.split(',').collect();
                if parts.len() >= 4 {
                    let x = parts[0].parse::<i32>().unwrap_or(100);
                    let y = parts[1].parse::<i32>().unwrap_or(100);
                    let w = parts[2].parse::<u32>().unwrap_or(800);
                    let h = parts[3].parse::<u32>().unwrap_or(600);
                    
                    let window_id = self.next_window_id.fetch_add(1, Ordering::Relaxed);
                    let _ = WINDOWS.lock().insert(window_id, Arc::new(Mutex::new(WindowState::new(
                        x, y, w, h, title.to_string()
                    ))));
                }
            }
        }
        crate::println!("[OrbitalBridge] open '{}' -> handle={}", path, handle);
        Ok(handle)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let event_size = core::mem::size_of::<Event>();
        let events_to_read = buf.len() / event_size;
        if events_to_read == 0 {
            return Ok(0);
        }

        let events_slice = unsafe {
            core::slice::from_raw_parts_mut(
                buf.as_mut_ptr() as *mut Event,
                events_to_read,
            )
        };

        // Check window-specific events first
        {
            let windows = WINDOWS.lock();
            if let Some(window) = windows.get(&id) {
                let mut guard = window.lock();
                let count = guard.pop_events(events_slice);
                return Ok(count * event_size);
            }
        }

        // Use the separate Orbital input buffer so we don't consume
        // events before the kernel shell (read_stdin / INPUT_BUF) sees them.
        let mut key_buf = [0u8; 4];
        let n = crate::drivers::keyboard::read_editor_byte(&mut key_buf);
        if n > 0 {
            events_slice[0] = Event {
                id: 0, // Keyboard
                flags: 1, // EVENT_READ
                data: key_buf[0] as usize,
            };
            return Ok(core::mem::size_of::<Event>());
        }

        // Try mouse
        let (x, y) = crate::drivers::ps2_mouse::get_mouse_pos();
        let btn = crate::drivers::ps2_mouse::get_mouse_btn();
        if x != self.last_mouse_x.load(Ordering::Relaxed) 
            || y != self.last_mouse_y.load(Ordering::Relaxed) 
            || btn != self.last_mouse_btn.load(Ordering::Relaxed) {
            self.last_mouse_x.store(x, Ordering::Relaxed);
            self.last_mouse_y.store(y, Ordering::Relaxed);
            self.last_mouse_btn.store(btn, Ordering::Relaxed);
            events_slice[0] = Event {
                id: 1, // Mouse
                flags: 1, // EVENT_READ
                data: ((x as usize) << 16) | (y as usize),
            };
            return Ok(event_size);
        }

        Ok(0)
    }

    fn fevent(&self, id: usize, flags: usize) -> SchemeResult<usize> {
        let mut ready = 0usize;

        let windows = WINDOWS.lock();
        if let Some(window) = windows.get(&id) {
            if !window.lock().events.is_empty() {
                ready |= flags;
                return Ok(ready);
            }
        }

        if crate::drivers::keyboard::has_input() {
            ready |= flags & 1;
        }
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        if mx != self.last_mouse_x.load(Ordering::Relaxed) 
            || my != self.last_mouse_y.load(Ordering::Relaxed) {
            ready |= flags & 1;
        }

        Ok(ready & flags)
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let s = match core::str::from_utf8(buf) {
            Ok(s) => s,
            Err(_) => return Err(crate::abi::AbiError::Other("Invalid UTF-8")),
        };
        
        let mut parts = s.split(',');
        let cmd = parts.next().unwrap_or("");

        match cmd.chars().next() {
            Some('O') => Ok(buf.len()),
            Some('F') | Some('f') => {
                crate::drivers::virtio_gpu::flush();
                Ok(buf.len())
            }
            Some('A') => {
                let val = parts.next().unwrap_or("0");
                if let Some(window) = WINDOWS.lock().get(&id) {
                    window.lock().async_mode = val == "1";
                }
                Ok(buf.len())
            }
            Some('M') => {
                let kind = parts.next().unwrap_or("");
                let val = parts.next().unwrap_or("0");
                if let Some(window) = WINDOWS.lock().get(&id) {
                    let mut guard = window.lock();
                    match kind {
                        "C" => guard.mouse_cursor = val == "1",
                        "G" => guard.mouse_grab = val == "1",
                        "R" => guard.mouse_relative = val == "1",
                        _ => {}
                    }
                }
                Ok(buf.len())
            }
            Some('P') => {
                let x: i32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let y: i32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                if let Some(window) = WINDOWS.lock().get(&id) {
                    let mut guard = window.lock();
                    guard.x = x;
                    guard.y = y;
                }
                Ok(buf.len())
            }
            Some('S') => {
                let w: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let h: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                if let Some(window) = WINDOWS.lock().get(&id) {
                    let mut guard = window.lock();
                    guard.width = w;
                    guard.height = h;
                }
                Ok(buf.len())
            }
            Some('T') => {
                let title = parts.next().unwrap_or("").to_string();
                if let Some(window) = WINDOWS.lock().get(&id) {
                    window.lock().title = title;
                }
                Ok(buf.len())
            }
            Some('Y') => {
                crate::drivers::virtio_gpu::flush();
                Ok(buf.len())
            }
            Some('D') => Ok(buf.len()),
            _ => Ok(buf.len()),
        }
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        let mut windows = WINDOWS.lock();
        windows.remove(&id);
        crate::klog!(crate::klog::Level::Debug, "[OrbitalBridge] close: {}", id);
        Ok(())
    }
}