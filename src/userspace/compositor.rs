/// Native Wayland-Compatible Compositor (NWCC)
///
/// VGA-Downsampled architecture: 80×25 virtual framebuffer → physical VGA text buffer.
/// Supports: window chrome (title bar + border), taskbar, shadow, mouse drag, IPC.

use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use alloc::string::String;
use crate::ipc::shm::SHM;
use crate::process::Pid;

// ── Virtual resolution ────────────────────────────────────────────────────
pub const VW: u32 = 80;
pub const VH: u32 = 25;
pub const PITCH: u32 = VW * 4;

// ── Window chrome constants ───────────────────────────────────────────────
const TITLEBAR_H: u32 = 2;   // rows reserved for title bar
const BORDER: u32 = 1;       // 1-pixel border on all sides
const TASKBAR_H: u32 = 1;    // bottom taskbar height

// ── Colors ────────────────────────────────────────────────────────────────
const COL_DESKTOP:    u32 = 0xFF1A1A2E; // deep navy
const COL_TASKBAR:    u32 = 0xFF0F3460;
const COL_TITLEBAR:   u32 = 0xFF16213E;
const COL_TITLEBAR_A: u32 = 0xFF3366FF;
const COL_BORDER:     u32 = 0xFF3366FF;
const _COL_SHADOW:    u32 = 0x80000000;
const _COL_TEXT:      u32 = 0xFFFFFFFF;
const COL_CLOSE_BTN:  u32 = 0xFFFF4444;
const _COL_CURSOR:    u32 = 0xFFFFFFFF;

/// Represents a client-side buffer backed by shared memory.
pub struct CompositorBuffer {
    pub shm_id: usize,
    pub width: u32,
    pub height: u32,
}

/// A surface is a rectangular area on the screen where a client renders.
pub struct Surface {
    pub owner: Pid,
    pub active_buffer: Option<usize>,
    pub x: i32,
    pub y: i32,
    pub z_index: i32,
    pub title: String,
    pub minimized: bool,
    /// Set true when Commit is received; cleared after compose pass
    pub dirty: bool,
    /// Damage region (client-space, NDC) — whole surface when (0,0,0,0)
    pub damage_x: u32,
    pub damage_y: u32,
    pub damage_w: u32,
    pub damage_h: u32,
}

impl Surface {
    /// Total width including border
    pub fn outer_w(&self, buf_w: u32) -> u32 { buf_w + BORDER * 2 }
    /// Total height including title bar + border
    pub fn outer_h(&self, buf_h: u32) -> u32 { buf_h + TITLEBAR_H + BORDER * 2 }
}

/// Wire format for IPC messages — must match Zig `Msg` struct exactly.
/// Layout: tag(u32) + pad(u32) + a(u64) + b(u64) + c(u64) + d(u64) + e(u64) = 48 bytes
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct WireMsg {
    pub tag: u32,
    pub _pad: u32,
    pub a: u64,
    pub b: u64,
    pub c: u64,
    pub d: u64,
    pub e: u64,
}

/// Decoded compositor commands
#[derive(Debug, Clone)]
pub enum WlMessage {
    CreateSurface { owner: Pid },
    CreateBuffer  { owner: Pid, shm_id: usize, width: u32, height: u32 },
    Attach        { surface_id: usize, buffer_id: usize },
    SetPosition   { surface_id: usize, x: i32, y: i32 },
    Commit        { surface_id: usize },
    SetTitle      { surface_id: usize, title: [u8; 32] },
    /// Client-to-server connection request (ping)
    Connect       { client_id: u32 },
    /// Server-to-client connection acknowledgment (pong)
    ConnectAck    { client_id: u32 },
    /// Damage region for partial updates (client-space coordinates)
    SetDamage     { surface_id: usize, x: u32, y: u32, w: u32, h: u32 },
}

impl WireMsg {
    pub fn decode(self) -> Option<WlMessage> {
        match self.tag {
            0 => Some(WlMessage::CreateSurface { owner: Pid(self.a) }),
            1 => Some(WlMessage::CreateBuffer {
                owner: Pid(self.a),
                shm_id: self.b as usize,
                width: self.c as u32,
                height: self.d as u32,
            }),
            2 => Some(WlMessage::Attach { surface_id: self.a as usize, buffer_id: self.b as usize }),
            3 => Some(WlMessage::SetPosition { surface_id: self.a as usize, x: self.b as i32, y: self.c as i32 }),
            4 => Some(WlMessage::Commit { surface_id: self.a as usize }),
            5 => {
                let mut title = [0u8; 32];
                let bytes = self.b.to_le_bytes();
                title[..8].copy_from_slice(&bytes);
                let bytes2 = self.c.to_le_bytes();
                title[8..16].copy_from_slice(&bytes2);
                let bytes3 = self.d.to_le_bytes();
                title[16..24].copy_from_slice(&bytes3);
                let bytes4 = self.e.to_le_bytes();
                title[24..32].copy_from_slice(&bytes4);
                Some(WlMessage::SetTitle { surface_id: self.a as usize, title })
            }
            6 => Some(WlMessage::Connect { client_id: self.a as u32 }),
            7 => Some(WlMessage::ConnectAck { client_id: self.a as u32 }),
            8 => Some(WlMessage::SetDamage {
                surface_id: self.a as usize,
                x: self.b as u32,
                y: self.c as u32,
                w: self.d as u32,
                h: self.e as u32,
            }),
            _ => None,
        }
    }
}

pub struct CompositorState {
    pub surfaces: BTreeMap<usize, Surface>,
    pub buffers:  BTreeMap<usize, CompositorBuffer>,
    pub next_id:  usize,
    pub ipc_channel: Option<u32>,

    // Drag state
    pub grabbed_surface: Option<usize>,
    pub grab_offset_x: i32,
    pub grab_offset_y: i32,
    pub prev_mouse_btn: bool,

    // Tick counter for animations
    pub tick: u32,
}

impl CompositorState {
    pub fn new() -> Self {
        let chan = 1;
        crate::println!("[NWCC] Compositor ready on channel {}", chan);
        Self {
            surfaces: BTreeMap::new(),
            buffers:  BTreeMap::new(),
            next_id:  1,
            ipc_channel: Some(chan),
            grabbed_surface: None,
            grab_offset_x: 0,
            grab_offset_y: 0,
            prev_mouse_btn: false,
            tick: 0,
        }
    }

    pub fn create_surface(&mut self, owner: Pid, title: &str) -> usize {
        let id = self.next_id; self.next_id += 1;
        self.surfaces.insert(id, Surface {
            owner,
            active_buffer: None,
            x: 5 + (id as i32 * 3) % 20,
            y: 3 + (id as i32 * 2) % 10,
            z_index: id as i32,
            title: String::from(title),
            minimized: false,
            dirty: true,
            damage_x: 0,
            damage_y: 0,
            damage_w: 0,
            damage_h: 0,
        });
        id
    }

    pub fn create_buffer(&mut self, _owner: Pid, shm_id: usize, w: u32, h: u32) -> usize {
        let id = self.next_id; self.next_id += 1;
        self.buffers.insert(id, CompositorBuffer { shm_id, width: w, height: h });
        id
    }

    pub fn attach(&mut self, surface_id: usize, buffer_id: usize) -> Result<(), &'static str> {
        let surface = self.surfaces.get_mut(&surface_id).ok_or("no surface")?;
        if !self.buffers.contains_key(&buffer_id) { return Err("no buffer"); }
        surface.active_buffer = Some(buffer_id);
        Ok(())
    }

    // ── Rendering ─────────────────────────────────────────────────────────

    /// Draw a single character into the virtual framebuffer (1 pixel = 1 char cell).
    /// Encodes as: 0xFF_BG_FG_ASCII so present_to_vga can decode it.
        unsafe fn draw_char(fb: *mut u8, x: u32, y: u32, ch: u8, fg: u8, bg: u8) {
        if x >= VW || y >= VH { return; }
        let idx = (y * VW + x) as usize * 4;
        let pixel: u32 = 0xFF000000 | ((bg as u32) << 16) | ((fg as u32) << 8) | (ch as u32);
        let p = fb.add(idx) as *mut u32;
        core::ptr::write_volatile(p, pixel);
    }

    /// Draw a string of chars starting at (x, y).
    unsafe fn draw_str(fb: *mut u8, x: u32, y: u32, s: &str, fg: u8, bg: u8) {
        for (i, ch) in s.bytes().enumerate() {
            Self::draw_char(fb, x + i as u32, y, ch, fg, bg);
        }
    }

    /// Draw window chrome (shadow, border, title bar) for a surface.
    unsafe fn draw_chrome(&self, fb: *mut u8, id: usize, active: bool) {
        let surface = match self.surfaces.get(&id) { Some(s) => s, None => return };
        if surface.minimized { return; }
        let buf = match surface.active_buffer.and_then(|b| self.buffers.get(&b)) {
            Some(b) => b, None => return,
        };

        let ox = surface.x as u32;
        let oy = surface.y as u32;
        let ow = surface.outer_w(buf.width);
        let oh = surface.outer_h(buf.height);

        // Shadow (2px offset)
        crate::zig_ffi::blend_rect(fb, PITCH, ox + 2, oy + 2, ow, oh, 0x000000, 80);

        // Border
        let border_col = if active { COL_BORDER } else { 0xFF334466 };
        crate::zig_ffi::fill_rect(fb, PITCH, ox, oy, ow, oh, border_col);

        // Title bar background
        let tb_col = if active { COL_TITLEBAR_A } else { COL_TITLEBAR };
        crate::zig_ffi::fill_rect(fb, PITCH, ox + BORDER, oy + BORDER, buf.width, TITLEBAR_H, tb_col);

        // Title text (truncated to fit)
        let max_chars = (buf.width.saturating_sub(4)) as usize;
        let title_bytes = surface.title.as_bytes();
        let len = title_bytes.len().min(max_chars);
        for (i, &ch) in title_bytes[..len].iter().enumerate() {
            Self::draw_char(fb, ox + BORDER + 1 + i as u32, oy + BORDER, ch, 0x0F, if active { 0x01 } else { 0x00 });
        }

        // Close button [X] at top-right
        if buf.width >= 3 {
            let cx = ox + BORDER + buf.width - 2;
            let cy = oy + BORDER;
            crate::zig_ffi::fill_rect(fb, PITCH, cx, cy, 2, 1, COL_CLOSE_BTN);
            Self::draw_char(fb, cx, cy, b'X', 0x0F, 0x04);
        }
    }

    /// Blit the client's SHM buffer onto the framebuffer (below title bar).
    unsafe fn draw_surface_content(&self, fb: *mut u8, id: usize) {
        let surface = match self.surfaces.get(&id) { Some(s) => s, None => return };
        if surface.minimized { return; }
        let buf = match surface.active_buffer.and_then(|b| self.buffers.get(&b)) {
            Some(b) => b, None => return,
        };
        let dx = (surface.x + BORDER as i32) as u32;
        let dy = (surface.y + BORDER as i32 + TITLEBAR_H as i32) as u32;
        if let Ok(shm_addr) = SHM.lock().attach(buf.shm_id as u32, Pid(0)) {
            crate::zig_ffi::blit_bitmap(
                fb, PITCH,
                shm_addr as *const u8,
                0, 0, buf.width, buf.height,
                dx, dy,
            );
        }
    }

    /// Draw the taskbar at the bottom.
    unsafe fn draw_taskbar(&self, fb: *mut u8) {
        let y = VH - TASKBAR_H;
        crate::zig_ffi::fill_rect(fb, PITCH, 0, y, VW, TASKBAR_H, COL_TASKBAR);

        // "ZiqaOS" label on left
        Self::draw_str(fb, 1, y, "ZiqaOS", 0x0B, 0x01);

        // Window buttons
        let mut btn_x = 10u32;
        for (&id, surface) in &self.surfaces {
            if btn_x + 12 >= VW { break; }
            let active = self.grabbed_surface == Some(id);
            let bg: u8 = if active { 0x01 } else { 0x00 };
            let fg: u8 = if surface.minimized { 0x08 } else { 0x0F };
            let title = &surface.title;
            let len = title.len().min(10);
            // Draw button background
            crate::zig_ffi::fill_rect(fb, PITCH, btn_x, y, len as u32 + 2, 1,
                if active { COL_TITLEBAR_A } else { 0xFF223355 });
            Self::draw_char(fb, btn_x, y, b'[', 0x07, bg);
            for (i, &ch) in title.as_bytes()[..len].iter().enumerate() {
                Self::draw_char(fb, btn_x + 1 + i as u32, y, ch, fg, bg);
            }
            Self::draw_char(fb, btn_x + 1 + len as u32, y, b']', 0x07, bg);
            btn_x += len as u32 + 3;
        }

        // Clock (tick-based, fake HH:MM)
        let mins = (self.tick / 60) % 60;
        let hours = (self.tick / 3600) % 24;
        let mut clock = [b'0'; 5];
        clock[0] = b'0' + (hours / 10) as u8;
        clock[1] = b'0' + (hours % 10) as u8;
        clock[2] = b':';
        clock[3] = b'0' + (mins / 10) as u8;
        clock[4] = b'0' + (mins % 10) as u8;
        for (i, &ch) in clock.iter().enumerate() {
            Self::draw_char(fb, VW - 6 + i as u32, y, ch, 0x0B, 0x01);
        }
    }

    /// Full composite pass: desktop → surfaces (back-to-front) → taskbar → cursor.
    pub unsafe fn compose(&mut self, fb: *mut u8) {
        // Desktop background
        crate::zig_ffi::clear(fb, (PITCH * VH) as usize, COL_DESKTOP);

        // Sort surfaces by z-index (back to front)
        let mut keys: Vec<usize> = self.surfaces.keys().cloned().collect();
        keys.sort_by_key(|id| self.surfaces[id].z_index);

        let top_id = keys.last().cloned();

        for &id in &keys {
            self.draw_chrome(fb, id, Some(id) == top_id);
            self.draw_surface_content(fb, id);
            // Clear dirty flags after render
            if let Some(s) = self.surfaces.get_mut(&id) {
                s.dirty = false;
                s.damage_w = 0;
                s.damage_h = 0;
            }
        }

        self.draw_taskbar(fb);

        // Cursor
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        let cx = ((mx.max(0) as u32 * VW) / 1920).min(VW - 1);
        let cy = ((my.max(0) as u32 * VH) / 1080).min(VH - 1);
        Self::draw_char(fb, cx, cy, b'*', 0x0F, 0x00);
    }

    // ── Interaction ───────────────────────────────────────────────────────

    fn hit_test_titlebar(&self, x: i32, y: i32) -> Option<usize> {
        // Check top-most first
        let mut keys: Vec<usize> = self.surfaces.keys().cloned().collect();
        keys.sort_by_key(|id| -self.surfaces[id].z_index);

        for id in keys {
            let s = &self.surfaces[&id];
            if s.minimized { continue; }
            let buf = match s.active_buffer.and_then(|b| self.buffers.get(&b)) {
                Some(b) => b, None => continue,
            };
            let ox = s.x;
            let oy = s.y;
            let ow = s.outer_w(buf.width) as i32;
            // Title bar region: full width, TITLEBAR_H rows
            if x >= ox && x < ox + ow &&
               y >= oy && y < oy + BORDER as i32 + TITLEBAR_H as i32 {
                return Some(id);
            }
        }
        None
    }

    fn update_interaction(&mut self) {
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        let btn = crate::drivers::ps2_mouse::get_mouse_btn();
        let cx = ((mx.max(0) as u32 * VW) / 1920) as i32;
        let cy = ((my.max(0) as u32 * VH) / 1080) as i32;

        let btn_down = btn & 1 != 0;
        let just_pressed = btn_down && !self.prev_mouse_btn;
        let just_released = !btn_down && self.prev_mouse_btn;
        self.prev_mouse_btn = btn_down;

        if just_pressed {
            if let Some(id) = self.hit_test_titlebar(cx, cy) {
                // Bring to front
                let max_z = self.surfaces.values().map(|s| s.z_index).max().unwrap_or(0);
                self.surfaces.get_mut(&id).unwrap().z_index = max_z + 1;
                self.grabbed_surface = Some(id);
                let s = &self.surfaces[&id];
                self.grab_offset_x = cx - s.x;
                self.grab_offset_y = cy - s.y;
            }
        }

        if btn_down {
            if let Some(id) = self.grabbed_surface {
                if let Some(s) = self.surfaces.get_mut(&id) {
                    s.x = (cx - self.grab_offset_x).max(0).min(VW as i32 - 10);
                    s.y = (cy - self.grab_offset_y).max(0).min(VH as i32 - 5);
                }
            }
        }

        if just_released {
            self.grabbed_surface = None;
        }
    }

    // ── IPC ───────────────────────────────────────────────────────────────

    fn process_ipc(&mut self) {
        let chan_id = match self.ipc_channel { Some(c) => c, None => return };
        while let Ok(msg) = crate::ipc::recv(chan_id) {
            if msg.len >= core::mem::size_of::<WireMsg>() {
                let wire = unsafe { core::ptr::read(msg.data.as_ptr() as *const WireMsg) };
                if let Some(cmd) = wire.decode() {
                    self.handle_message(cmd);
                }
            }
        }
    }

    fn handle_message(&mut self, msg: WlMessage) {
        match msg {
            WlMessage::CreateSurface { owner } => {
                let id = self.create_surface(owner, "Window");
                crate::println!("[NWCC] Surface {} for PID {}", id, owner.0);
            }
            WlMessage::CreateBuffer { owner, shm_id, width, height } => {
                let id = self.create_buffer(owner, shm_id, width, height);
                crate::println!("[NWCC] Buffer {} (SHM {}, {}x{})", id, shm_id, width, height);
            }
            WlMessage::Attach { surface_id, buffer_id } => {
                let _ = self.attach(surface_id, buffer_id);
            }
            WlMessage::SetPosition { surface_id, x, y } => {
                if let Some(s) = self.surfaces.get_mut(&surface_id) { s.x = x; s.y = y; }
            }
            WlMessage::SetTitle { surface_id, title } => {
                if let Some(s) = self.surfaces.get_mut(&surface_id) {
                    let len = title.iter().position(|&b| b == 0).unwrap_or(32);
                    s.title = String::from(core::str::from_utf8(&title[..len]).unwrap_or("Window"));
                }
            }
            WlMessage::Commit { surface_id } => {
                // Mark surface as needing redraw
                if let Some(s) = self.surfaces.get_mut(&surface_id) {
                    s.dirty = true;
                    // If damage was empty, set full-surface damage
                    if s.damage_w == 0 && s.damage_h == 0 {
                        if let Some(buf) = s.active_buffer.and_then(|b| self.buffers.get(&b)) {
                            s.damage_w = buf.width;
                            s.damage_h = buf.height;
                        }
                    }
                }
            }
            WlMessage::Connect { client_id } => {
                // Respond with ConnectAck to establish the IPC channel
                let ack = WireMsg { tag: 7, _pad: 0, a: client_id as u64, b: 0, c: 0, d: 0, e: 0 };
                let bytes = unsafe {
                    core::slice::from_raw_parts(&ack as *const WireMsg as *const u8,
                        core::mem::size_of::<WireMsg>())
                };
                let chan = self.ipc_channel.unwrap_or(1);
                let _ = crate::ipc::send(chan, Pid(0), bytes);
                crate::println!("[NWCC] Client {} connected", client_id);
            }
            WlMessage::ConnectAck { client_id } => {
                crate::println!("[NWCC] Client {} acknowledged connection", client_id);
            }
            WlMessage::SetDamage { surface_id, x, y, w, h } => {
                if let Some(s) = self.surfaces.get_mut(&surface_id) {
                    s.damage_x = x;
                    s.damage_y = y;
                    s.damage_w = w;
                    s.damage_h = h;
                    s.dirty = true;
                }
            }
        }
    }

    // ── Main loop ─────────────────────────────────────────────────────────

    pub fn run(&mut self) -> ! {
        crate::println!("[NWCC] Starting compositor ({}x{} virtual)", VW, VH);

        let mut back_buffer = alloc::vec![0u32; (VW * VH) as usize].into_boxed_slice();
        let bb_ptr = back_buffer.as_mut_ptr() as *mut u8;

        loop {
            self.tick = self.tick.wrapping_add(1);
            self.update_interaction();
            self.process_ipc();
            unsafe { self.compose(bb_ptr); }
            present_to_vga(&back_buffer);
            crate::timer::sleep_ms(Pid(0), 16); // ~60fps target
        }
    }
}

// ── VGA Downsampling ──────────────────────────────────────────────────────

/// Present the 80×25 virtual framebuffer to the physical VGA text buffer.
///
/// Pixel encoding:
///   - `0xFF_BG_FG_ASCII` → direct VGA char cell (used for text/chrome)
///   - Any other XRGB    → nearest VGA color, block char (█ = 0xDB)
fn present_to_vga(v_fb: &[u32]) {
    let offset = crate::BOOT_INFO.lock()
        .as_ref()
        .map(|bi| bi.physical_memory_offset)
        .unwrap_or(0);
    let vga = (offset + 0xb8000) as *mut u16;

    for y in 0..25usize {
        for x in 0..80usize {
            let pixel = v_fb[y * 80 + x];
            let cell = if pixel >> 24 == 0xFF && (pixel & 0x00FF0000) < 0x00100000 {
                // Encoded text cell: 0xFF_BG_FG_ASCII (BG < 16, FG < 16)
                let ascii = (pixel & 0xFF) as u16;
                let fg    = ((pixel >> 8) & 0x0F) as u16;
                let bg    = ((pixel >> 16) & 0x0F) as u16;
                ascii | (((bg << 4) | fg) << 8)
            } else if pixel == 0 {
                (b' ' as u16) | (0x00u16 << 8)
            } else {
                let vga_color = nearest_vga_color(pixel) as u16;
                0xDBu16 | ((vga_color | (vga_color << 4)) << 8)
            };
            unsafe { core::ptr::write_volatile(vga.add(y * 80 + x), cell); }
        }
    }
}

fn nearest_vga_color(color: u32) -> u8 {
    const VGA: [u32; 16] = [
        0x000000, 0x0000AA, 0x00AA00, 0x00AAAA,
        0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
        0x555555, 0x5555FF, 0x55FF55, 0x55FFFF,
        0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
    ];
    let r = ((color >> 16) & 0xFF) as i32;
    let g = ((color >> 8)  & 0xFF) as i32;
    let b = ( color        & 0xFF) as i32;
    let mut best = 0u8;
    let mut min_d = i32::MAX;
    for (i, &vc) in VGA.iter().enumerate() {
        let vr = ((vc >> 16) & 0xFF) as i32;
        let vg = ((vc >> 8)  & 0xFF) as i32;
        let vb = ( vc        & 0xFF) as i32;
        let d = (r-vr)*(r-vr) + (g-vg)*(g-vg) + (b-vb)*(b-vb);
        if d < min_d { min_d = d; best = i as u8; }
    }
    best
}

pub fn start() -> ! {
    let mut compositor = CompositorState::new();
    compositor.run();
}
