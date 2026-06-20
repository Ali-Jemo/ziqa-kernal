//! Orbital-style pixel desktop for ZiqaKernel.
//!
//! Runs as a kernel thread and renders directly to the active XRGB framebuffer.
//! This is deliberately self-contained: it reuses the existing keyboard/mouse
//! drivers and avoids Redox userspace dependencies while keeping the Orbital
//! desktop model: windows, z-order, chrome, taskbar, launcher, built-in apps.

use alloc::vec::Vec;

use crate::drivers::{keyboard, ps2_mouse, virtio_gpu};

const MAX_WINDOWS: usize = 8;
const TITLE_H: i32 = 24;
const TASKBAR_H: i32 = 34;
const MIN_W: i32 = 160;
const MIN_H: i32 = 110;
const MOUSE_W: i32 = 1920;
const MOUSE_H: i32 = 1080;

const C_BG_BOT: u32 = 0x000F172A;
const C_PANEL: u32 = 0x001E293B;
const C_PANEL_HI: u32 = 0x00334155;
const C_TEXT: u32 = 0x00F8FAFC;
const C_DIM: u32 = 0x0094A3B8;
const C_ACCENT: u32 = 0x000EA5E9;
const C_ACCENT_2: u32 = 0x008B5CF6;
const C_WARN: u32 = 0x00EF4444;
const C_OK: u32 = 0x0010B981;
const C_WIN: u32 = 0x000F172A;
const C_WIN_2: u32 = 0x001E293B;
const C_BORDER: u32 = 0x00475569;
const C_BORDER_ACTIVE: u32 = 0x000EA5E9;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppKind {
    Terminal,
    Files,
    System,
    Editor,
    Settings,
    About,
    Client,
    SdlApp,
}

impl AppKind {
    fn title(self) -> &'static str {
        match self {
            AppKind::Terminal => "Terminal",
            AppKind::Files => "Files",
            AppKind::System => "System Monitor",
            AppKind::Editor => "Editor",
            AppKind::Settings => "Settings",
            AppKind::About => "About Ziqa",
            AppKind::Client => "Application Client",
            AppKind::SdlApp => "SDL2 Application",
        }
    }
}


#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Rect {
    const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    fn contains(self, px: i32, py: i32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
}

#[derive(Clone, Copy)]
struct Window {
    id: u32,
    app: AppKind,
    rect: Rect,
    restore_rect: Option<Rect>,
    minimized: bool,
    maximized: bool,
    tick: u32,
    input_len: usize,
    input: [u8; 64],
    lines: [[u8; 48]; 8],
    shm_addr: u64,
    shm_w: u32,
    shm_h: u32,
    path: [u8; 64],
    path_len: usize,
}

impl Window {
    const fn empty() -> Self {
        Self {
            id: 0,
            app: AppKind::About,
            rect: Rect::new(0, 0, 0, 0),
            restore_rect: None,
            minimized: true,
            maximized: false,
            tick: 0,
            input_len: 0,
            input: [0; 64],
            lines: [[0; 48]; 8],
            shm_addr: 0,
            shm_w: 0,
            shm_h: 0,
            path: [0; 64],
            path_len: 0,
        }
    }
}

struct Desktop {
    width: i32,
    height: i32,
    windows: [Option<Window>; MAX_WINDOWS],
    z: [usize; MAX_WINDOWS],
    z_len: usize,
    active: Option<usize>,
    next_id: u32,
    dragging: Option<(usize, i32, i32)>,
    resizing: Option<(usize, i32, i32)>,
    prev_btn: u8,
    last_mouse_x: i32,
    last_mouse_y: i32,
    menu_open: bool,
    menu_sel: usize,
    tick: u32,
    last_click_tick: u32,
    last_click_win: Option<usize>,
}

impl Desktop {
    const fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            windows: [None; MAX_WINDOWS],
            z: [0; MAX_WINDOWS],
            z_len: 0,
            active: None,
            next_id: 1,
            dragging: None,
            resizing: None,
            prev_btn: 0,
            last_mouse_x: -1,
            last_mouse_y: -1,
            menu_open: false,
            menu_sel: 0,
            tick: 0,
            last_click_tick: 0,
            last_click_win: None,
        }
    }

    fn boot_apps(&mut self) {
        self.launch(AppKind::Terminal);
        self.launch(AppKind::Files);
        self.launch(AppKind::System);
        self.launch(AppKind::About);
        if self.windows[0].is_some() {
            self.raise(0);
            self.active = Some(0);
        }
    }

    fn launch(&mut self, app: AppKind) {
        let mut free = None;
        for i in 0..MAX_WINDOWS {
            if self.windows[i].is_none() {
                free = Some(i);
                break;
            }
        }
        if let Some(slot) = free {
            let offset = (slot as i32) * 32;
            let w = match app {
                AppKind::Terminal => 460,
                AppKind::Files => 420,
                AppKind::System => 360,
                AppKind::Editor => 430,
                AppKind::Settings => 380,
                AppKind::About => 360,
                AppKind::Client => 320,
                AppKind::SdlApp => 320,
            };
            let h = match app {
                AppKind::Terminal => 280,
                AppKind::Editor => 300,
                _ => 240,
            };
            let max_x = (self.width - w - 16).max(16);
            let max_y = (self.height - TASKBAR_H - h - 16).max(42);
            let mut win = Window::empty();
            win.id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            win.app = app;
            win.rect = Rect::new((64 + offset) % max_x, (58 + offset) % max_y, w, h);
            win.minimized = false;
            if app == AppKind::Files {
                let init_path = "/";
                win.path_len = init_path.len();
                win.path[..init_path.len()].copy_from_slice(init_path.as_bytes());
            }
            seed_window(&mut win);
            self.windows[slot] = Some(win);
            self.raise(slot);
            self.active = Some(slot);
        }
    }

    fn launch_client(&mut self, w: u32, h: u32) {
        let mut free = None;
        for i in 0..MAX_WINDOWS {
            if self.windows[i].is_none() {
                free = Some(i);
                break;
            }
        }
        if let Some(slot) = free {
            let offset = (slot as i32) * 32;
            let win_w = (w as i32).clamp(MIN_W, self.width - 64);
            let win_h = (h as i32 + TITLE_H + 36).clamp(MIN_H, self.height - 64);
            let mut win = Window::empty();
            win.id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            win.app = AppKind::Client;
            win.rect = Rect::new((64 + offset) % (self.width - win_w), (58 + offset) % (self.height - win_h), win_w, win_h);
            win.minimized = false;
            self.windows[slot] = Some(win);
            self.raise(slot);
            self.active = Some(slot);
        }
    }

    fn raise(&mut self, slot: usize) {
        let mut out = [0usize; MAX_WINDOWS];
        let mut n = 0;
        for i in 0..self.z_len {
            if self.z[i] != slot {
                out[n] = self.z[i];
                n += 1;
            }
        }
        out[n] = slot;
        n += 1;
        self.z = out;
        self.z_len = n;
    }

    fn close(&mut self, slot: usize) {
        self.windows[slot] = None;
        let mut out = [0usize; MAX_WINDOWS];
        let mut n = 0;
        for i in 0..self.z_len {
            if self.z[i] != slot {
                out[n] = self.z[i];
                n += 1;
            }
        }
        self.z = out;
        self.z_len = n;
        self.active = None;
        if n > 0 {
            self.active = Some(self.z[n - 1]);
        }
    }

    fn maximize(&mut self, slot: usize) {
        if let Some(win) = &mut self.windows[slot] {
            if win.maximized {
                if let Some(restored) = win.restore_rect {
                    win.rect = restored;
                }
                win.maximized = false;
            } else {
                win.restore_rect = Some(win.rect);
                win.rect = Rect::new(0, 0, self.width, self.height - TASKBAR_H);
                win.maximized = true;
            }
        }
    }

    fn hit_window(&self, x: i32, y: i32) -> Option<(usize, Hit)> {
        let mut i = self.z_len;
        while i > 0 {
            i -= 1;
            let slot = self.z[i];
            if let Some(win) = &self.windows[slot] {
                if win.minimized || !win.rect.contains(x, y) {
                    continue;
                }
                let local_x = x - win.rect.x;
                let local_y = y - win.rect.y;
                let close = Rect::new(win.rect.w - 24, 5, 16, 14);
                let max = Rect::new(win.rect.w - 44, 5, 16, 14);
                let min = Rect::new(win.rect.w - 64, 5, 16, 14);
                let resize = Rect::new(win.rect.w - 16, win.rect.h - 16, 16, 16);
                if close.contains(local_x, local_y) {
                    return Some((slot, Hit::Close));
                }
                if max.contains(local_x, local_y) {
                    return Some((slot, Hit::Maximize));
                }
                if min.contains(local_x, local_y) {
                    return Some((slot, Hit::Minimize));
                }
                if !win.maximized && resize.contains(local_x, local_y) {
                    return Some((slot, Hit::Resize));
                }
                if local_y < TITLE_H {
                    return Some((slot, Hit::Title));
                }
                return Some((slot, Hit::Body));
            }
        }
        None
    }

    fn mouse_pos(&self) -> (i32, i32) {
        let (x, y) = ps2_mouse::get_mouse_pos();
        (
            (x.clamp(0, MOUSE_W - 1) * self.width / MOUSE_W).clamp(0, self.width - 1),
            (y.clamp(0, MOUSE_H - 1) * self.height / MOUSE_H).clamp(0, self.height - 1),
        )
    }

    fn handle_mouse(&mut self) -> bool {
        let (mx, my) = self.mouse_pos();
        let btn = ps2_mouse::get_mouse_btn();
        let down = (btn & 1) != 0;
        let was_down = (self.prev_btn & 1) != 0;
        let pressed = down && !was_down;
        let released = !down && was_down;
        let moved = mx != self.last_mouse_x || my != self.last_mouse_y;
        self.last_mouse_x = mx;
        self.last_mouse_y = my;
        let mut dirty = moved || pressed || released;

        if pressed {
            dirty = true;
            if my >= self.height - TASKBAR_H {
                self.handle_taskbar_click(mx, my);
            } else if self.menu_open {
                self.handle_menu_click(mx, my);
            } else if let Some((slot, hit)) = self.hit_window(mx, my) {
                self.raise(slot);
                self.active = Some(slot);
                match hit {
                    Hit::Close => self.close(slot),
                    Hit::Maximize => self.maximize(slot),
                    Hit::Minimize => {
                        if let Some(win) = &mut self.windows[slot] {
                            win.minimized = true;
                        }
                    }
                    Hit::Title => {
                        let double_click = self.last_click_win == Some(slot) && self.tick.wrapping_sub(self.last_click_tick) < 20;
                        self.last_click_tick = self.tick;
                        self.last_click_win = Some(slot);

                        if double_click {
                            self.maximize(slot);
                        } else if let Some(win) = &self.windows[slot] {
                            if !win.maximized {
                                self.dragging = Some((slot, mx - win.rect.x, my - win.rect.y));
                            }
                        }
                    }
                    Hit::Resize => {
                        if let Some(win) = &self.windows[slot] {
                            self.resizing = Some((slot, win.rect.w - mx, win.rect.h - my));
                        }
                    }
                    Hit::Body => {
                        if let Some(win_mut) = &mut self.windows[slot] {
                            let local_x = mx - win_mut.rect.x;
                            let local_y = my - win_mut.rect.y;
                            if win_mut.app == AppKind::Files {
                                let dir_path = alloc::string::String::from(
                                    core::str::from_utf8(&win_mut.path[..win_mut.path_len]).unwrap_or("/")
                                );
                                if dir_path != "/" && local_x >= win_mut.rect.w - 52 && local_x <= win_mut.rect.w - 12 && local_y >= TITLE_H + 12 && local_y <= TITLE_H + 26 {
                                    if let Some(idx) = dir_path.rfind('/') {
                                        let parent = if idx == 0 { "/" } else { &dir_path[..idx] };
                                        win_mut.path_len = parent.len();
                                        win_mut.path[..parent.len()].copy_from_slice(parent.as_bytes());
                                    }
                                } else {
                                    let item_idx = ((local_y - (TITLE_H + 36)) / 22) as usize;
                                    let files = crate::fs::vfs::VFS.read().list_dir(&dir_path);
                                    if item_idx < files.len() {
                                        let target = files[item_idx].clone();
                                        if crate::fs::vfs::VFS.read().is_dir(&target) {
                                            win_mut.path_len = target.len();
                                            win_mut.path[..target.len()].copy_from_slice(target.as_bytes());
                                        }
                                    }
                                }
                            } else if win_mut.app == AppKind::Editor {
                                let area = Rect::new(12, TITLE_H + 12, win_mut.rect.w - 24, win_mut.rect.h - TITLE_H - 36);
                                handle_editor_click(win_mut, local_x, local_y, area);
                            }
                        }
                    }
                }
            } else {
                self.active = None;
                self.menu_open = false;
            }
        }

        if down {
            if let Some((slot, off_x, off_y)) = self.dragging {
                if let Some(win) = &mut self.windows[slot] {
                    win.rect.x = (mx - off_x).clamp(0, self.width - win.rect.w);
                    win.rect.y = (my - off_y).clamp(0, self.height - TASKBAR_H - TITLE_H);
                    dirty = true;
                }
            }
            if let Some((slot, off_w, off_h)) = self.resizing {
                if let Some(win) = &mut self.windows[slot] {
                    win.rect.w = (mx + off_w).clamp(MIN_W, self.width - win.rect.x);
                    win.rect.h = (my + off_h).clamp(MIN_H, self.height - TASKBAR_H - win.rect.y);
                    dirty = true;
                }
            }
        }

        if released {
            self.dragging = None;
            self.resizing = None;
        }
        self.prev_btn = btn;
        dirty
    }

    fn handle_taskbar_click(&mut self, x: i32, _y: i32) {
        if x < 92 {
            self.menu_open = !self.menu_open;
            return;
        }
        let mut x0 = 104;
        for i in 0..self.z_len {
            let slot = self.z[i];
            if let Some(win) = &mut self.windows[slot] {
                let w = 138;
                if x >= x0 && x < x0 + w {
                    win.minimized = false;
                    self.raise(slot);
                    self.active = Some(slot);
                    self.menu_open = false;
                    return;
                }
                x0 += w + 6;
            }
        }
    }

    fn handle_menu_click(&mut self, x: i32, y: i32) {
        let menu = Rect::new(12, self.height - TASKBAR_H - 214, 220, 204);
        if !menu.contains(x, y) {
            self.menu_open = false;
            return;
        }
        let item = ((y - menu.y - 12) / 30) as usize;
        if item < MENU_ITEMS.len() {
            self.launch(MENU_ITEMS[item].0);
        }
        self.menu_open = false;
    }

    fn handle_key(&mut self) -> bool {
        let raw = keyboard::poll_compositor_key();
        if raw == 0 {
            return false;
        }
        let key = if (raw & 0x100) != 0 { (raw & 0xff) as u8 } else { raw as u8 };
        match key {
            b' ' => self.menu_open = !self.menu_open,
            b'1'..=b'6' => {
                let idx = (key - b'1') as usize;
                if idx < MENU_ITEMS.len() {
                    self.launch(MENU_ITEMS[idx].0);
                }
            }
            0x1b => self.menu_open = false,
            0x80 => self.nudge_active(0, -8),
            0x81 => self.nudge_active(0, 8),
            0x82 => self.nudge_active(-8, 0),
            0x83 => self.nudge_active(8, 0),
            b'\n' | b'\r' => self.push_terminal_line(),
            8 | 0x7f => self.backspace_active(),
            32..=126 => self.type_active(key),
            _ => {}
        }
        true
    }

    fn nudge_active(&mut self, dx: i32, dy: i32) {
        if let Some(slot) = self.active {
            if let Some(win) = &mut self.windows[slot] {
                if !win.maximized {
                    win.rect.x = (win.rect.x + dx).clamp(0, self.width - win.rect.w);
                    win.rect.y = (win.rect.y + dy).clamp(0, self.height - TASKBAR_H - win.rect.h);
                }
            }
        }
    }

    fn type_active(&mut self, ch: u8) {
        if let Some(slot) = self.active {
            if let Some(win) = &mut self.windows[slot] {
                if matches!(win.app, AppKind::Terminal | AppKind::Editor) && win.input_len < win.input.len() {
                    win.input[win.input_len] = ch;
                    win.input_len += 1;
                }
            }
        }
    }

    fn backspace_active(&mut self) {
        if let Some(slot) = self.active {
            if let Some(win) = &mut self.windows[slot] {
                if win.input_len > 0 {
                    win.input_len -= 1;
                    win.input[win.input_len] = 0;
                }
            }
        }
    }

    fn push_terminal_line(&mut self) {
        if let Some(slot) = self.active {
            let mut cmd = [0u8; 64];
            let mut cmd_len = 0;
            if let Some(win) = &mut self.windows[slot] {
                if win.app != AppKind::Terminal || win.input_len == 0 {
                    return;
                }
                cmd_len = win.input_len;
                cmd[..cmd_len].copy_from_slice(&win.input[..cmd_len]);
                win.input_len = 0;
            }
            if let Some(win) = &mut self.windows[slot] {
                if let Ok(cmd_str) = core::str::from_utf8(&cmd[..cmd_len]) {
                    interpret_terminal_command(win, cmd_str);
                }
            }
        }
    }

    fn step(&mut self) -> Damage {
        self.tick = self.tick.wrapping_add(1);
        for slot in 0..MAX_WINDOWS {
            if let Some(win) = &mut self.windows[slot] {
                win.tick = win.tick.wrapping_add(1);
            }
        }

        // Auto-launch SdlApp window if a frame arrives
        let has_sdl_frame = crate::scheme::sdl::SDL_FRAME.lock().is_some();
        if has_sdl_frame {
            let mut found = false;
            for slot in 0..MAX_WINDOWS {
                if let Some(win) = &self.windows[slot] {
                    if win.app == AppKind::SdlApp {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                self.launch(AppKind::SdlApp);
            }
        }
        // Poll COMPOSITOR_CHAN for userspace client windows (limit to prevent starving the event loop)
        let mut limit = 10;
        while limit > 0 {
            limit -= 1;
            match crate::ipc::recv(3) {
                Ok(msg) => {
                    if msg.len < 1 { continue; }
                    let op = msg.data[0];
                    match op {
                        1 => { // Connect
                            crate::klog!(crate::klog::Level::Info, "[GUI] Client connected via IPC");
                        }
                        2 => { // CreateSurface
                            if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::CreateSurfaceMsg>() {
                                let payload = unsafe {
                                    core::ptr::read_unaligned(
                                        msg.data.as_ptr().add(1) as *const crate::ipc::gui::CreateSurfaceMsg,
                                    )
                                };
                                self.launch_client(payload.width, payload.height);
                            }
                        }
                        5 => { // BufferAttach
                            if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::BufferAttachMsg>() {
                                let payload = unsafe {
                                    core::ptr::read_unaligned(
                                        msg.data.as_ptr().add(1) as *const crate::ipc::gui::BufferAttachMsg,
                                    )
                                };
                                let shm = crate::ipc::shm::SHM.lock();
                                if let Ok(addr) = shm.attach(payload.shm_id, crate::process::Pid(0)) {
                                    for slot in 0..MAX_WINDOWS {
                                        if let Some(win) = &mut self.windows[slot] {
                                            if win.app == AppKind::Client && win.shm_addr == 0 {
                                                win.shm_addr = addr;
                                                win.shm_w = payload.width;
                                                win.shm_h = payload.height;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Err(_) => break,
            }
        }

        let mouse_dirty = self.handle_mouse();
        let key_dirty = self.handle_key();
        if key_dirty || (self.tick % 60 == 0) {
            Damage::Scene
        } else if mouse_dirty {
            let down = (self.prev_btn & 1) != 0;
            if down || self.dragging.is_some() || self.resizing.is_some() {
                Damage::Scene
            } else {
                Damage::Cursor
            }
        } else {
            Damage::None
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Damage {
    None,
    Cursor,
    Scene,
}

#[derive(Clone, Copy)]
enum Hit {
    Close,
    Maximize,
    Minimize,
    Resize,
    Title,
    Body,
}

const MENU_ITEMS: &[(AppKind, &str, &str)] = &[
    (AppKind::Terminal, "Terminal", "Run commands"),
    (AppKind::Files, "Files", "Browse disks"),
    (AppKind::System, "System", "CPU / memory"),
    (AppKind::Editor, "Editor", "Write notes"),
    (AppKind::Settings, "Settings", "Display / input"),
    (AppKind::About, "About", "Kernel info"),
];

fn seed_window(win: &mut Window) {
    match win.app {
        AppKind::Terminal => {
            copy_line(&mut win.lines[0], b"Ziqa terminal ready");
            copy_line(&mut win.lines[1], b"Type and press Enter");
        }
        AppKind::Editor => copy_line(&mut win.lines[0], b"Type text here"),
        _ => {}
    }
}

fn copy_line(dst: &mut [u8; 48], src: &[u8]) {
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);
    if n < dst.len() {
        dst[n..].fill(0);
    }
}

struct Canvas {
    ptr: *mut u8,
    width: i32,
    height: i32,
    back: Vec<u32>,
}

impl Canvas {
    fn from_active_display() -> Option<Self> {
        virtio_gpu::get_fb_info().map(|(addr, w, h, _)| {
            let mut back = Vec::new();
            back.resize((w as usize).saturating_mul(h as usize), 0);
            Self {
                ptr: addr as *mut u8,
                width: w as i32,
                height: h as i32,
                back,
            }
        })
    }

    fn rect(&mut self, r: Rect, color: u32) {
        let x0 = r.x.max(0);
        let y0 = r.y.max(0);
        let x1 = (r.x + r.w).min(self.width);
        let y1 = (r.y + r.h).min(self.height);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for y in y0..y1 {
            let start = (y * self.width + x0) as usize;
            let len = (x1 - x0) as usize;
            if start + len <= self.back.len() {
                self.back[start..start + len].fill(color);
            }
        }
    }

    fn blit(&mut self, dx: i32, dy: i32, src: *const u32, sw: i32, sh: i32) {
        for y in 0..sh {
            let dest_y = dy + y;
            if dest_y < 0 || dest_y >= self.height { continue; }
            for x in 0..sw {
                let dest_x = dx + x;
                if dest_x < 0 || dest_x >= self.width { continue; }
                let pixel = unsafe { *src.add((y * sw + x) as usize) };
                let idx = (dest_y * self.width + dest_x) as usize;
                if let Some(px) = self.back.get_mut(idx) {
                    *px = pixel;
                }
            }
        }
    }

    fn present(&mut self) {
        let count = self.back.len();
        unsafe {
            core::ptr::copy_nonoverlapping(self.back.as_ptr(), self.ptr as *mut u32, count);
        }
    }

    fn present_rect(&mut self, r: Rect) {
        let x0 = r.x.max(0);
        let y0 = r.y.max(0);
        let x1 = (r.x + r.w).min(self.width);
        let y1 = (r.y + r.h).min(self.height);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for y in y0..y1 {
            let start = (y * self.width + x0) as usize;
            let len = (x1 - x0) as usize;
            if start + len <= self.back.len() {
                let dst = unsafe { (self.ptr as *mut u32).add(start) };
                unsafe {
                    core::ptr::copy_nonoverlapping(self.back.as_ptr().add(start), dst, len);
                }
            }
        }
    }

    fn direct_px(&mut self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        unsafe {
            let p = (self.ptr as *mut u32).add((y * self.width + x) as usize);
            core::ptr::write_volatile(p, color);
        }
    }

    fn border(&mut self, r: Rect, color: u32) {
        self.rect(Rect::new(r.x, r.y, r.w, 1), color);
        self.rect(Rect::new(r.x, r.y + r.h - 1, r.w, 1), color);
        self.rect(Rect::new(r.x, r.y, 1, r.h), color);
        self.rect(Rect::new(r.x + r.w - 1, r.y, 1, r.h), color);
    }

    fn text(&mut self, mut x: i32, y: i32, text: &str, color: u32) {
        for b in text.bytes() {
            self.glyph(x, y, b, color);
            x += 8;
        }
    }

    fn bytes(&mut self, mut x: i32, y: i32, bytes: &[u8], color: u32) {
        for &b in bytes {
            if b == 0 {
                break;
            }
            self.glyph(x, y, b, color);
            x += 8;
        }
    }

    fn glyph(&mut self, x: i32, y: i32, ch: u8, color: u32) {
        let g = glyph_bits(ch);
        for row in 0..7 {
            let bits = g[row as usize];
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 != 0 {
                    self.rect(Rect::new(x + col, y + row, 1, 1), color);
                }
            }
        }
    }
}

fn render(desktop: &Desktop, fb: &mut Canvas) {
    draw_wallpaper(desktop, fb);
    for i in 0..desktop.z_len {
        let slot = desktop.z[i];
        if let Some(win) = &desktop.windows[slot] {
            if !win.minimized {
                draw_window(desktop, fb, slot, win);
            }
        }
    }
    draw_taskbar(desktop, fb);
    if desktop.menu_open {
        draw_menu(desktop, fb);
    }
    fb.present();
    let (mx, my) = desktop.mouse_pos();
    draw_cursor_direct(fb, mx, my);
    virtio_gpu::flush();
}

fn draw_wallpaper(desktop: &Desktop, fb: &mut Canvas) {
    // Solid background (instantly filled in a single memory block write)
    fb.rect(Rect::new(0, 0, fb.width, fb.height), C_BG_BOT);

    // Modern header panel
    fb.rect(Rect::new(0, 0, fb.width, 42), C_PANEL);
    fb.rect(Rect::new(0, 42, fb.width, 1), C_BORDER);

    // Branding
    fb.text(24, 18, "Ziqa Desktop Environment", 0x00FFFFFF);
    fb.text(fb.width - 300, 18, "You OS Compositor (Isolated Mode)", C_DIM);

    // Modern decorative underline
    let pulse = ((desktop.tick / 8) % 80) as i32;
    fb.rect(Rect::new(24, 38, 140 + pulse, 2), C_ACCENT);
}

fn draw_window(desktop: &Desktop, fb: &mut Canvas, slot: usize, win: &Window) {
    let active = desktop.active == Some(slot);
    let r = win.rect;

    if !win.maximized {
        fb.rect(Rect::new(r.x + 4, r.y + 4, r.w, r.h), 0x33000000);
        fb.rect(Rect::new(r.x + 8, r.y + 8, r.w - 8, r.h - 8), 0x1A000000);
    }

    fb.rect(r, C_WIN);
    fb.rect(Rect::new(r.x + 1, r.y + TITLE_H, r.w - 2, r.h - TITLE_H - 1), C_WIN_2);

    let title_bg = if active { C_ACCENT } else { C_PANEL };
    fb.rect(Rect::new(r.x, r.y, r.w, TITLE_H), title_bg);
    fb.border(r, if active { C_BORDER_ACTIVE } else { C_BORDER });
    fb.text(r.x + 10, r.y + 8, win.app.title(), 0x00FFFFFF);

    button(fb, r.x + r.w - 64, r.y + 5, "_", C_PANEL_HI, C_TEXT);
    button(fb, r.x + r.w - 44, r.y + 5, if win.maximized { "r" } else { "^" }, C_PANEL_HI, C_TEXT);
    button(fb, r.x + r.w - 24, r.y + 5, "x", C_WARN, 0x00FFFFFF);

    let app_area = Rect::new(r.x + 12, r.y + TITLE_H + 12, r.w - 24, r.h - TITLE_H - 36);
    draw_app(fb, win, app_area);

    let status_y = r.y + r.h - 20;
    fb.rect(Rect::new(r.x + 1, status_y, r.w - 2, 19), C_PANEL);
    fb.rect(Rect::new(r.x + 1, status_y, r.w - 2, 1), C_BORDER);
    fb.text(r.x + 10, status_y + 6, "Ready", C_DIM);

    if !win.maximized {
        fb.rect(Rect::new(r.x + r.w - 12, r.y + r.h - 12, 8, 8), C_PANEL_HI);
        fb.border(Rect::new(r.x + r.w - 12, r.y + r.h - 12, 8, 8), C_BORDER);
    }
}

fn button(fb: &mut Canvas, x: i32, y: i32, label: &str, bg: u32, fg: u32) {
    fb.rect(Rect::new(x, y, 16, 14), bg);
    fb.border(Rect::new(x, y, 16, 14), C_BORDER);
    fb.text(x + 5, y + 4, label, fg);
}

fn draw_app(fb: &mut Canvas, win: &Window, area: Rect) {
    match win.app {
        AppKind::Terminal => draw_terminal(fb, win, area),
        AppKind::Files => draw_files(fb, win, area),
        AppKind::System => draw_system(fb, win, area),
        AppKind::Editor => draw_editor(fb, win, area),
        AppKind::Settings => draw_settings(fb, area),
        AppKind::About => draw_about(fb, area),
        AppKind::Client => draw_client(fb, win, area),
        AppKind::SdlApp => draw_sdl_app(fb, win, area),
    }
}

fn draw_terminal(fb: &mut Canvas, win: &Window, area: Rect) {
    fb.rect(area, 0x000F172A);
    fb.border(area, 0x00334155);
    let mut y = area.y + 10;
    for line in &win.lines {
        fb.bytes(area.x + 10, y, line, C_OK);
        y += 16;
    }
    fb.text(area.x + 10, area.y + area.h - 24, "$", C_ACCENT);
    fb.bytes(area.x + 28, area.y + area.h - 24, &win.input[..win.input_len], C_TEXT);
    if (win.tick / 20) % 2 == 0 {
        fb.rect(Rect::new(area.x + 30 + (win.input_len as i32 * 8), area.y + area.h - 23, 7, 10), C_TEXT);
    }
}

fn draw_files(fb: &mut Canvas, win: &Window, area: Rect) {
    let dir_path = core::str::from_utf8(&win.path[..win.path_len]).unwrap_or("/");
    fb.text(area.x, area.y, "Directory:", C_TEXT);
    fb.text(area.x + 88, area.y, dir_path, C_ACCENT);

    let has_back = dir_path != "/";
    if has_back {
        button(fb, area.x + area.w - 32, area.y, "..", C_PANEL_HI, C_TEXT);
    }

    let files = crate::fs::vfs::VFS.read().list_dir(dir_path);
    let mut y = area.y + 24;
    for (i, name) in files.iter().enumerate() {
        if y + 20 > area.y + area.h {
            break;
        }
        let is_even = i % 2 == 0;
        let bg = if is_even { 0x001E293B } else { 0x000F172A };
        fb.rect(Rect::new(area.x, y - 4, area.w, 18), bg);

        let display_name = if let Some(idx) = name.rfind('/') {
            &name[idx + 1..]
        } else {
            name.as_str()
        };

        let is_dir = crate::fs::vfs::VFS.read().is_dir(name);
        let color = if is_dir { C_ACCENT } else { C_TEXT };
        fb.text(area.x + 10, y, display_name, color);
        if is_dir {
            fb.text(area.x + area.w - 48, y, "DIR", C_DIM);
        } else {
            fb.text(area.x + area.w - 48, y, "FILE", C_DIM);
        }
        y += 22;
    }
}

fn draw_system(fb: &mut Canvas, win: &Window, area: Rect) {
    fb.text(area.x, area.y, "Activity & Resources:", C_TEXT);
    meter(fb, area.x, area.y + 28, area.w, "CPU", ((win.tick / 3) % 100) as i32, C_ACCENT);
    meter(fb, area.x, area.y + 64, area.w, "MEM", 42, C_OK);
    meter(fb, area.x, area.y + 100, area.w, "IO", ((win.tick / 5) % 70) as i32, C_ACCENT_2);
    fb.text(area.x, area.y + 140, "Kernel: ZiqaKernel v0.1.0", C_DIM);
    fb.text(area.x, area.y + 158, "Display: 32bpp framebuffer", C_DIM);
}

fn meter(fb: &mut Canvas, x: i32, y: i32, w: i32, label: &str, val: i32, color: u32) {
    fb.text(x, y, label, C_TEXT);
    fb.rect(Rect::new(x + 40, y, w - 50, 12), 0x000F172A);
    fb.rect(Rect::new(x + 40, y, (w - 50) * val / 100, 12), color);
    fb.border(Rect::new(x + 40, y, w - 50, 12), C_BORDER);
}

fn draw_editor(fb: &mut Canvas, win: &Window, area: Rect) {
    fb.text(area.x, area.y, "File: /disk/notes.txt", C_TEXT);
    fb.rect(Rect::new(area.x, area.y + 22, area.w, area.h - 44), 0x00F8FAFC);
    fb.border(Rect::new(area.x, area.y + 22, area.w, area.h - 44), C_BORDER);
    fb.text(area.x + 12, area.y + 36, "Type text below:", 0x0064748B);
    fb.bytes(area.x + 12, area.y + 58, &win.input[..win.input_len], 0x000F172A);

    button(fb, area.x, area.y + area.h - 16, "Save", C_ACCENT, C_TEXT);
    button(fb, area.x + 48, area.y + area.h - 16, "Load", C_PANEL_HI, C_TEXT);
}

fn draw_settings(fb: &mut Canvas, area: Rect) {
    fb.text(area.x, area.y, "System Preferences", C_TEXT);
    let settings = [
        ("Theme", "Ziqa Dark Classic"),
        ("Compositor", "Active FB Thread"),
        ("Input Driver", "PS/2 + USB Keyboard/Mouse"),
        ("Display Res", "1024x768 32bpp"),
    ];
    for (i, &(k, v)) in settings.iter().enumerate() {
        let y = area.y + 30 + i as i32 * 24;
        fb.text(area.x + 8, y, k, C_TEXT);
        fb.text(area.x + 130, y, v, C_DIM);
    }
}

fn draw_about(fb: &mut Canvas, area: Rect) {
    fb.text(area.x, area.y, "About ZiqaOS", C_TEXT);
    fb.text(area.x, area.y + 26, "A modern microkernel-inspired system.", C_DIM);
    fb.text(area.x, area.y + 44, "Native You OS-style GUI Compositor.", C_DIM);
    fb.text(area.x, area.y + 78, "Composed entirely in Rust & Zig.", C_ACCENT);
    fb.text(area.x, area.y + 96, "Antigravity UI Kit Engine.", C_ACCENT_2);
}

fn draw_client(fb: &mut Canvas, win: &Window, area: Rect) {
    if win.shm_addr != 0 && win.shm_w > 0 && win.shm_h > 0 {
        let src = win.shm_addr as *const u32;
        fb.blit(area.x, area.y, src, win.shm_w as i32, win.shm_h as i32);
    } else {
        fb.rect(area, 0x000F172A);
        fb.text(area.x + 10, area.y + 10, "Attaching client buffer...", C_DIM);
    }
}
fn draw_sdl_app(fb: &mut Canvas, _win: &Window, area: Rect) {
    let frame_guard = crate::scheme::sdl::SDL_FRAME.lock();
    if let Some((ref pixels, w, h)) = *frame_guard {
        fb.blit(area.x, area.y, pixels.as_ptr(), w as i32, h as i32);
    } else {
        fb.rect(area, 0x000F172A);
        fb.text(area.x + 10, area.y + 10, "Waiting for SDL2 frame...", C_DIM);
    }
}

fn draw_taskbar(desktop: &Desktop, fb: &mut Canvas) {
    let y = desktop.height - TASKBAR_H;
    fb.rect(Rect::new(0, y, desktop.width, TASKBAR_H), C_PANEL);
    fb.rect(Rect::new(0, y, desktop.width, 1), C_BORDER);

    fb.rect(Rect::new(10, y + 6, 74, 22), if desktop.menu_open { C_ACCENT } else { C_PANEL_HI });
    fb.text(22, y + 13, "Start", C_TEXT);

    let mut x = 104;
    for i in 0..desktop.z_len {
        let slot = desktop.z[i];
        if let Some(win) = &desktop.windows[slot] {
            let active = desktop.active == Some(slot);
            fb.rect(Rect::new(x, y + 6, 132, 22), if active { C_ACCENT } else { C_PANEL_HI });
            fb.text(x + 8, y + 13, win.app.title(), C_TEXT);
            x += 138;
        }
    }

    let hours = (desktop.tick / 3600) % 24;
    let mins = (desktop.tick / 60) % 60;
    let mut clock_str = [0u8; 8];
    clock_str[0] = b'0' + (hours / 10) as u8;
    clock_str[1] = b'0' + (hours % 10) as u8;
    clock_str[2] = b':';
    clock_str[3] = b'0' + (mins / 10) as u8;
    clock_str[4] = b'0' + (mins % 10) as u8;
    clock_str[5] = b':';
    let secs = desktop.tick % 60;
    clock_str[6] = b'0' + (secs / 10) as u8;
    clock_str[7] = b'0' + (secs % 10) as u8;

    fb.rect(Rect::new(desktop.width - 98, y + 6, 88, 22), C_PANEL_HI);
    fb.border(Rect::new(desktop.width - 98, y + 6, 88, 22), C_BORDER);
    fb.bytes(desktop.width - 90, y + 13, &clock_str, C_TEXT);
}

fn draw_menu(desktop: &Desktop, fb: &mut Canvas) {
    let menu = Rect::new(12, desktop.height - TASKBAR_H - 214, 220, 204);
    fb.rect(menu, C_PANEL);
    fb.border(menu, C_BORDER_ACTIVE);

    fb.rect(Rect::new(menu.x + 1, menu.y + 1, menu.w - 2, 24), C_PANEL_HI);
    fb.text(menu.x + 12, menu.y + 8, "Ziqa Launcher", C_TEXT);

    for (i, item) in MENU_ITEMS.iter().enumerate() {
        let y = menu.y + 38 + i as i32 * 28;
        fb.rect(
            Rect::new(menu.x + 8, y - 5, menu.w - 16, 24),
            if desktop.menu_sel == i { C_ACCENT } else { C_PANEL },
        );
        fb.text(menu.x + 16, y, item.1, C_TEXT);
        fb.text(menu.x + 106, y, item.2, C_DIM);
    }
}

fn cursor_rect(x: i32, y: i32) -> Rect {
    Rect::new(x, y, 12, 12)
}

fn draw_cursor_direct(fb: &mut Canvas, x: i32, y: i32) {
    let shape = [
        0b10000000u8,
        0b11000000,
        0b11100000,
        0b11110000,
        0b11111000,
        0b11100000,
        0b10110000,
        0b00110000,
    ];
    for (row, bits) in shape.iter().enumerate() {
        for col in 0..8 {
            if (bits >> (7 - col)) & 1 != 0 {
                fb.direct_px(x + col, y + row as i32, 0x00FFFFFF);
                fb.direct_px(x + col + 1, y + row as i32 + 1, 0x00000000);
            }
        }
    }
}

fn glyph_bits(ch: u8) -> [u8; 7] {
    match ch {
        b'A' | b'a' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'B' | b'b' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        b'C' | b'c' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        b'D' | b'd' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        b'E' | b'e' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        b'F' | b'f' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        b'G' | b'g' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        b'H' | b'h' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'I' | b'i' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F],
        b'J' | b'j' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0E],
        b'K' | b'k' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' | b'l' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        b'M' | b'm' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' | b'n' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' | b'o' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'P' | b'p' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        b'Q' | b'q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        b'R' | b'r' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        b'S' | b's' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        b'T' | b't' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' | b'u' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'V' | b'v' => [0x11, 0x11, 0x11, 0x11, 0x0A, 0x0A, 0x04],
        b'W' | b'w' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        b'X' | b'x' => [0x11, 0x0A, 0x04, 0x04, 0x04, 0x0A, 0x11],
        b'Y' | b'y' => [0x11, 0x0A, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'Z' | b'z' => [0x1F, 0x02, 0x04, 0x04, 0x08, 0x10, 0x1F],
        b'0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        b'1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        b'3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        b'4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        b'5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        b'6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        b'7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        b'8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        b'9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        b'.' => [0, 0, 0, 0, 0, 0x0C, 0x0C],
        b',' => [0, 0, 0, 0, 0, 0x0C, 0x08],
        b':' => [0, 0x0C, 0x0C, 0, 0x0C, 0x0C, 0],
        b'/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        b'(' => [0x02, 0x04, 0x04, 0x04, 0x04, 0x04, 0x02],
        b')' => [0x08, 0x04, 0x04, 0x04, 0x04, 0x04, 0x08],
        b'%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        b'&' => [0x0C, 0x12, 0x12, 0x0C, 0x15, 0x12, 0x0D],
        b'^' => [0x04, 0x0A, 0x11, 0, 0, 0, 0],
        b'<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        b'>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        b'-' => [0, 0, 0, 0x1F, 0, 0, 0],
        b'_' => [0, 0, 0, 0, 0, 0, 0x1F],
        b'$' => [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04],
        _ => [0x1F, 0x11, 0x05, 0x02, 0x04, 0, 0x04],
    }
}

fn interpret_terminal_command(win: &mut Window, cmd: &str) {
    let mut args = cmd.split_whitespace();
    let command = args.next().unwrap_or("");

    for row in 0..win.lines.len() {
        win.lines[row] = [0u8; 48];
    }
    let mut line_idx = 0;

    let mut print_line = |text: &str| {
        if line_idx < win.lines.len() {
            copy_line(&mut win.lines[line_idx], text.as_bytes());
            line_idx += 1;
        }
    };

    match command {
        "help" => {
            print_line("ZiqaOS GUI Shell v0.1.0");
            print_line("Available commands:");
            print_line("  ls [path]      - list files");
            print_line("  cat <path>     - show file");
            print_line("  write <p> <t>  - write text");
            print_line("  clear          - clear display");
        }
        "clear" => {}
        "ls" => {
            let path = args.next().unwrap_or("/");
            let files = crate::fs::vfs::VFS.read().list_dir(path);
            print_line(&alloc::format!("Listing: {}", path));
            for name in files.iter().take(6) {
                let display_name = if let Some(idx) = name.rfind('/') {
                    &name[idx + 1..]
                } else {
                    name.as_str()
                };
                print_line(&alloc::format!(" - {}", display_name));
            }
            if files.len() > 6 {
                print_line(&alloc::format!("... and {} more", files.len() - 6));
            }
        }
        "cat" => {
            if let Some(path) = args.next() {
                match crate::fs::vfs::VFS.read().open(path, 0) {
                    Ok(handle) => {
                        let mut buf = [0u8; 256];
                        match crate::fs::vfs::VFS.read().read_handle(&handle, &mut buf, 0) {
                            Ok(n) => {
                                let content = core::str::from_utf8(&buf[..n]).unwrap_or("[Binary]");
                                let mut lines = content.lines();
                                for _ in 0..7 {
                                    if let Some(line) = lines.next() {
                                        print_line(line);
                                    }
                                }
                            }
                            Err(_) => print_line("Error: failed to read file"),
                        }
                    }
                    Err(_) => print_line("Error: file not found"),
                }
            } else {
                print_line("Usage: cat <file_path>");
            }
        }
        "write" => {
            if let Some(path) = args.next() {
                let rest: alloc::vec::Vec<&str> = args.collect();
                let text = rest.join(" ");
                crate::fs::vfs::VFS.write().create(path);
                match crate::fs::vfs::VFS.read().open(path, 0) {
                    Ok(handle) => {
                        match crate::fs::vfs::VFS.read().write_handle(&handle, text.as_bytes(), 0) {
                            Ok(_) => print_line("File written successfully"),
                            Err(_) => print_line("Error: failed to write"),
                        }
                    }
                    Err(_) => print_line("Error: failed to open file"),
                }
            } else {
                print_line("Usage: write <path> <text>");
            }
        }
        _ => {
            print_line(&alloc::format!("Unknown: {}", command));
            print_line("Type 'help' for commands list");
        }
    }
}

fn handle_editor_click(win: &mut Window, local_x: i32, local_y: i32, area: Rect) {
    let save_btn = Rect::new(area.x, area.y + area.h - 16, 40, 14);
    let load_btn = Rect::new(area.x + 48, area.y + area.h - 16, 40, 14);
    let path_str = "/disk/notes.txt";

    if save_btn.contains(local_x, local_y) {
        crate::klog!(crate::klog::Level::Info, "Editor: Saving to {}", path_str);
        crate::fs::vfs::VFS.write().create(path_str);
        if let Ok(handle) = crate::fs::vfs::VFS.read().open(path_str, 0) {
            let data = &win.input[..win.input_len];
            let _ = crate::fs::vfs::VFS.read().write_handle(&handle, data, 0);
        }
    } else if load_btn.contains(local_x, local_y) {
        crate::klog!(crate::klog::Level::Info, "Editor: Loading from {}", path_str);
        if let Ok(handle) = crate::fs::vfs::VFS.read().open(path_str, 0) {
            let mut buf = [0u8; 64];
            if let Ok(n) = crate::fs::vfs::VFS.read().read_handle(&handle, &mut buf, 0) {
                win.input_len = n.min(64);
                win.input[..win.input_len].copy_from_slice(&buf[..win.input_len]);
            }
        }
    }
}

pub fn orbital_desktop_main(_arg: *const ()) {
    crate::println!("[GUI] You OS desktop starting");
    let Some(mut fb) = Canvas::from_active_display() else {
        crate::println!("[GUI] no active framebuffer; desktop disabled");
        return;
    };
    let mut desktop = Desktop::new(fb.width, fb.height);
    desktop.boot_apps();

    // Enable preemption so other processes (drivers, clients, keyboard, scheduler)
    // can run preemptively without getting starved by our loop.
    crate::process::scheduler::enable_preemption();

    render(&desktop, &mut fb);
    let (mut cursor_x, mut cursor_y) = desktop.mouse_pos();
    loop {
        match desktop.step() {
            Damage::Scene => {
                render(&desktop, &mut fb);
            }
            Damage::Cursor => {
                fb.present_rect(cursor_rect(cursor_x, cursor_y));
                let pos = desktop.mouse_pos();
                cursor_x = pos.0;
                cursor_y = pos.1;
                draw_cursor_direct(&mut fb, cursor_x, cursor_y);
                virtio_gpu::flush();
            }
            Damage::None => {}
        }
        // Sleep for 16ms (roughly 60 FPS) to yield CPU to other processes cleanly
        // and prevent freezing or high CPU usage.
        crate::timer::sleep_ms(crate::process::Pid(0), 16);
    }
}
