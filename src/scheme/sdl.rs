//! Minimal SDL-style compatibility scheme.
//!
//! This is the kernel-side target for a future `libziqa_sdl`/SDL backend:
//! - `sdl:info`   read 16-byte display info: b"ZSDI", width, height, bpp
//! - `sdl:events` read 16-byte input events: kind, a, b, c (u32 LE)
//! - `sdl:frame`  write frame: b"ZSDL" + width + height + pitch + XRGB8888 pixels
//!
//! ponytail: tiny binary protocol first; replace with shared buffers once userspace
//! apps and handle passing exist.

use crate::abi::AbiError;
use crate::drivers::{keyboard, ps2_mouse, virtio_gpu};
use crate::scheme::{Scheme, SchemeResult};
use spin::Mutex;
pub static SDL_FRAME: Mutex<Option<(alloc::vec::Vec<u32>, u32, u32)>> = Mutex::new(None);

const HANDLE_INFO: usize = 1;
const HANDLE_EVENTS: usize = 2;
const HANDLE_FRAME: usize = 3;

const EVENT_NONE: u32 = 0;
const EVENT_MOUSE: u32 = 1;
const EVENT_KEY: u32 = 2;

#[derive(Clone, Copy)]
struct InputState {
    mouse_x: i32,
    mouse_y: i32,
    buttons: u8,
}

pub struct SdlScheme {
    input: Mutex<InputState>,
}

impl SdlScheme {
    pub const fn new() -> Self {
        Self {
            input: Mutex::new(InputState {
                mouse_x: -1,
                mouse_y: -1,
                buttons: 0,
            }),
        }
    }

    fn write_u32(buf: &mut [u8], off: usize, value: u32) {
        if off + 4 <= buf.len() {
            buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
    }

    fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
        if off + 4 > buf.len() {
            return None;
        }
        Some(u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]))
    }

    fn fill_event(buf: &mut [u8], kind: u32, a: u32, b: u32, c: u32) -> usize {
        if buf.len() < 16 {
            return 0;
        }
        Self::write_u32(buf, 0, kind);
        Self::write_u32(buf, 4, a);
        Self::write_u32(buf, 8, b);
        Self::write_u32(buf, 12, c);
        16
    }
}

impl Scheme for SdlScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        match path {
            "sdl:" | "sdl:frame" => Ok(HANDLE_FRAME),
            "sdl:info" => Ok(HANDLE_INFO),
            "sdl:events" => Ok(HANDLE_EVENTS),
            _ => Err(AbiError::Other("unsupported sdl path")),
        }
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        match id {
            HANDLE_INFO => {
                if buf.len() < 16 {
                    return Ok(0);
                }
                if let Some((_, w, h, bpp)) = virtio_gpu::get_fb_info() {
                    buf[0..4].copy_from_slice(b"ZSDI");
                    Self::write_u32(buf, 4, w);
                    Self::write_u32(buf, 8, h);
                    Self::write_u32(buf, 12, bpp);
                    Ok(16)
                } else {
                    Err(AbiError::Other("No display"))
                }
            }
            HANDLE_EVENTS => {
                let key = keyboard::poll_compositor_key();
                if key != 0 {
                    return Ok(Self::fill_event(buf, EVENT_KEY, key as u32, 0, 0));
                }

                let (x, y) = ps2_mouse::get_mouse_pos();
                let buttons = ps2_mouse::get_mouse_btn();
                let mut input = self.input.lock();
                if x != input.mouse_x || y != input.mouse_y || buttons != input.buttons {
                    input.mouse_x = x;
                    input.mouse_y = y;
                    input.buttons = buttons;
                    return Ok(Self::fill_event(
                        buf,
                        EVENT_MOUSE,
                        x.max(0) as u32,
                        y.max(0) as u32,
                        buttons as u32,
                    ));
                }

                Ok(Self::fill_event(buf, EVENT_NONE, 0, 0, 0))
            }
            _ => Err(AbiError::Other("Bad file descriptor")),
        }
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        if id != HANDLE_FRAME {
            return Err(AbiError::Other("write not supported for this handle"));
        }
        if buf.len() < 16 || &buf[0..4] != b"ZSDL" {
            return Err(AbiError::Other("invalid sdl frame header"));
        }

        let src_w = Self::read_u32(buf, 4).ok_or(AbiError::ParseError)? as usize;
        let src_h = Self::read_u32(buf, 8).ok_or(AbiError::ParseError)? as usize;
        let pitch = Self::read_u32(buf, 12).ok_or(AbiError::ParseError)? as usize;
        if src_w == 0 || src_h == 0 || pitch < src_w.saturating_mul(4) {
            return Err(AbiError::ParseError);
        }

        let mut frame_guard = SDL_FRAME.lock();
        let frame = frame_guard.get_or_insert_with(|| (alloc::vec::Vec::new(), 0, 0));
        frame.1 = src_w as u32;
        frame.2 = src_h as u32;
        frame.0.resize(src_w * src_h, 0);

        let payload = &buf[16..];
        for y in 0..src_h {
            let src = y * pitch;
            let len = src_w * 4;
            if src + len > payload.len() {
                break;
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    payload.as_ptr().add(src),
                    frame.0.as_mut_ptr().add(y * src_w) as *mut u8,
                    len,
                );
            }
        }
        Ok(buf.len())
    }

    fn fevent(&self, id: usize, _flags: usize) -> SchemeResult<usize> {
        match id {
            HANDLE_INFO | HANDLE_EVENTS | HANDLE_FRAME => Ok(1),
            _ => Err(AbiError::Other("bad file descriptor")),
        }
    }

    fn close(&self, _id: usize) -> SchemeResult<()> {
        Ok(())
    }
}
