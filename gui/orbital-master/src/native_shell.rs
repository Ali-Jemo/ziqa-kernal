//! Native shell for `ziqa-bga-direct` — desktop, dock, cursor, and built-in apps.

#![cfg(feature = "ziqa-bga-direct")]

use std::cmp;
use std::collections::BTreeMap;
use std::time::Instant;

use crate::config::Config;
use crate::core::display::Display;
use crate::window::{Window, WindowId};
use orbclient::{
    Color, Mode, Renderer,
    image::{Image, ImageAligned},
    rect::{Rect, RectEdge},
};
use orbfont::Font;

// Desktop chrome geometry, in screen-pixel coordinates. Shared by draw_desktop
// and hit_test so the dock layout the user sees and the dock layout click
// detection uses cannot drift apart. Sizes are `u32` (matching `Rect`'s w/h);
// cast to `i32` where added to a position.
const BAR_H: u32 = 32;
const DOCK_H: u32 = 40;
const DOCK_PAD: u32 = 24;
const DOCK_BOTTOM_GAP: u32 = 16;
const DOCK_SPACING: u32 = 40;
const DOCK_ICON_SIZE: u32 = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum NativeAppKind {
    Terminal,
    Files,
    Settings,
}

pub enum ShellAction {
    None,
    Launch(NativeAppKind),
}

struct NativeAppState {
    kind: NativeAppKind,
    lines: Vec<String>,
    input: String,
}
pub struct NativeShell {
    apps: BTreeMap<WindowId, NativeAppState>,
    start_time: Instant,
}

impl NativeShell {
    pub fn new() -> Self {
        Self {
            apps: BTreeMap::new(),
            start_time: Instant::now(),
        }
    }

    pub fn running_apps(&self) -> Vec<NativeAppKind> {
        self.apps.values().map(|a| a.kind).collect()
    }

    pub fn draw_desktop(
        &mut self,
        display: &mut Display,
        clip: &Rect,
        screen: &Rect,
        font: &Font,
        config: &Config,
        show_welcome: bool,
        cursor_x: i32,
        cursor_y: i32,
        focused_title: &str,
        running_apps: &[NativeAppKind],
        focused_app_kind: Option<NativeAppKind>,
    ) {
        // Desktop chrome (bar/dock/icons) is positioned in SCREEN-absolute
        // coordinates, then every primitive is clipped to `clip` so only the
        // damaged region is written. Anchoring at `clip` (the dirty rect) was
        // the old bug: every small redraw (e.g. a cursor move) repainted the
        // bar/dock at the cursor's position, smearing the screen. The damaged
        // region is cleared to the desktop background by the caller
        // (OrbitalScheme::redraw) before this is invoked.
        // Nucleus desktop background (gradient + faint grid + nucleus mark +
        // field dots), painted before chrome so bar/dock/windows sit on top.
        Self::draw_desktop_background(display, clip, screen);
        let bar = Rect::new(screen.left(), screen.top(), screen.width(), BAR_H);
        Self::draw_rect(display, clip, bar, Color::from(config.bar_color));

        // ── Top Rail: ◆ ZiqaOS  [ capsule ]  UP 00:00 ◌ ──
        let text_color = Color::from(config.text_color);
        let bar_center_y = bar.top() + 8;

        // LEFT: ◆ ZiqaOS — diamond mark + label
        let nucleus_cx = bar.left() + 12;
        Self::draw_diamond(
            display,
            clip,
            nucleus_cx + 4,
            bar_center_y + 6,
            5,
            Color::rgb(0x7D, 0xBD, 0xFF),
        ); // light blue nucleus
        Self::draw_label(
            display,
            clip,
            font,
            "ZiqaOS",
            nucleus_cx + 12,
            bar_center_y,
            text_color,
        );
        let left_end = nucleus_cx + 12 + 48; // approximate end of "ZiqaOS" (~6 chars × 8px)

        // RIGHT: UP 02:14 ◌
        let elapsed = self.start_time.elapsed();
        let total_secs = elapsed.as_secs();
        let uptime_str = if total_secs < 3600 {
            format!("UP {:02}:{:02}", total_secs / 60, total_secs % 60)
        } else {
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            let secs = total_secs % 60;
            format!("UP {:02}:{:02}:{:02}", hours, mins, secs)
        };
        // Render uptime label to measure its width, then draw at precise position
        {
            let uptime_label = font.render(&uptime_str, 16.0);
            let uptime_w = uptime_label.width() as i32;
            let uptime_x = bar.right() - 32 - uptime_w; // 32px for dot + gap
            let mut img = Image::from_color(
                uptime_label.width(),
                uptime_label.height(),
                Color::rgba(0, 0, 0, 0),
            );
            img.mode().set(Mode::Overwrite);
            uptime_label.draw(&mut img, 0, 0, text_color);
            let label_rect = Rect::new(uptime_x, bar_center_y, img.width(), img.height());
            let clipped = clip.intersection(&label_rect);
            if !clipped.is_empty() {
                display
                    .roi_mut(&clipped)
                    .blend(&img.roi(&clipped.translate(-uptime_x, -bar_center_y)));
            }
            // System status dot ◌ — rightmost
            Self::draw_dot(
                display,
                clip,
                bar.right() - 16,
                bar_center_y + 6,
                3,
                Color::rgb(0x7D, 0xBD, 0xFF),
            );
        }
        let right_start = bar.right() - 96; // rough left edge of right section

        // CENTER: focused title capsule [ Title — Active ] or [ Desktop ]
        {
            let cap_text = if !focused_title.is_empty() {
                format!("[ {} — Active ]", focused_title)
            } else {
                format!("[ Desktop ]")
            };
            let cap_label = font.render(&cap_text, 16.0);
            let cap_w = cap_label.width() as i32;
            let center_x = bar.left() + (bar.width() / 2) as i32;
            let cap_x = center_x - (cap_w / 2);
            // Only draw if it fits between left_end and right_start
            if cap_x >= left_end && cap_x + cap_w <= right_start {
                let mut img = Image::from_color(
                    cap_label.width(),
                    cap_label.height(),
                    Color::rgba(0, 0, 0, 0),
                );
                img.mode().set(Mode::Overwrite);
                cap_label.draw(&mut img, 0, 0, text_color);
                let label_rect = Rect::new(cap_x, bar_center_y, img.width(), img.height());
                let clipped = clip.intersection(&label_rect);
                if !clipped.is_empty() {
                    display
                        .roi_mut(&clipped)
                        .blend(&img.roi(&clipped.translate(-cap_x, -bar_center_y)));
                }
            } else {
                // Fallback: just draw plain title left of center if capsule won't fit
                let fallback_x = bar.left() + (bar.width() / 3) as i32;
                Self::draw_label(
                    display,
                    clip,
                    font,
                    focused_title,
                    fallback_x,
                    bar_center_y,
                    text_color,
                );
            }
        }

        // ── Bottom Dock: Nucleus Dock ──
        let dock = Rect::new(
            screen.left() + DOCK_PAD as i32,
            screen.top() + screen.height().saturating_sub(DOCK_BOTTOM_GAP + DOCK_H) as i32,
            screen.width().saturating_sub(DOCK_PAD * 2),
            DOCK_H,
        );
        Self::draw_rect(display, clip, dock, Color::from(config.bar_color));
        Self::draw_border_rect(
            display,
            clip,
            dock,
            Color::from(config.bar_highlight_color),
            1,
        );

        let hovered = match self.hit_test(cursor_x, cursor_y, screen) {
            ShellAction::Launch(kind) => Some(kind),
            ShellAction::None => None,
        };

        let icon_y = dock.top() + 8;
        let start_x = dock.left() + 14;

        let term_icon = Rect::new(start_x, icon_y, DOCK_ICON_SIZE, DOCK_ICON_SIZE);
        let files_icon = Rect::new(
            start_x + DOCK_SPACING as i32,
            icon_y,
            DOCK_ICON_SIZE,
            DOCK_ICON_SIZE,
        );
        let settings_icon = Rect::new(
            start_x + DOCK_SPACING as i32 * 2,
            icon_y,
            DOCK_ICON_SIZE,
            DOCK_ICON_SIZE,
        );

        // Dock icon data for unified iteration
        let app_icons: [(NativeAppKind, Rect, Color, &str); 3] = [
            (
                NativeAppKind::Terminal,
                term_icon,
                Color::rgb(0x39, 0x7D, 0xD8),
                "Terminal",
            ),
            (
                NativeAppKind::Files,
                files_icon,
                Color::rgb(0x2F, 0xB3, 0x72),
                "Files",
            ),
            (
                NativeAppKind::Settings,
                settings_icon,
                Color::rgb(0xA8, 0x72, 0xD8),
                "Settings",
            ),
        ];

        for &(kind, icon_rect, icon_color, label) in &app_icons {
            let is_hovered = hovered == Some(kind);
            let is_running = running_apps.contains(&kind);
            let is_focused = focused_app_kind == Some(kind);

            let icon_cx = icon_rect.left() + (DOCK_ICON_SIZE as i32 / 2);
            let icon_cy = icon_rect.top() + (DOCK_ICON_SIZE as i32 / 2);

            // Hover field: ◌◆◌ pattern for nucleus magnetic field feel
            if is_hovered {
                // Left dot ◌
                Self::draw_dot(display, clip, icon_cx - 10, icon_cy, 3, icon_color);
                // Center diamond ◆ (the icon itself)
                // Right dot ◌
                Self::draw_dot(display, clip, icon_cx + 10, icon_cy, 3, icon_color);
            }

            // Icon core: colored rect background
            Self::draw_rect(display, clip, icon_rect, icon_color);
            // Icon nucleus: diamond ◈ in center
            Self::draw_diamond(
                display,
                clip,
                icon_cx,
                icon_cy,
                9,
                Color::rgb(
                    icon_color.r().saturating_add(0x40).min(0xFF),
                    icon_color.g().saturating_add(0x40).min(0xFF),
                    icon_color.b().saturating_add(0x40).min(0xFF),
                ),
            );

            // Label
            Self::draw_label(
                display,
                clip,
                font,
                label,
                icon_rect.left(),
                icon_rect.bottom() + 2,
                text_color,
            );

            // Running nucleus dot or focused diamond
            let dot_cx = icon_rect.left() + (DOCK_ICON_SIZE as i32 / 2);
            let dot_cy = icon_rect.bottom() + 6;
            if is_focused {
                // Focused app: diamond ◆ below icon
                Self::draw_diamond(display, clip, dot_cx, dot_cy, 5, icon_color);
            } else if is_running {
                // Running app: dot • below icon
                Self::draw_dot(display, clip, dot_cx, dot_cy, 3, icon_color);
            }
        }

        if show_welcome {
            let panel_w = cmp::min(screen.width().saturating_sub(64), 620);
            let panel_h = cmp::min(screen.height().saturating_sub(96), 154);
            let panel = Rect::new(
                screen.left() + (screen.width().saturating_sub(panel_w) / 2) as i32,
                screen.top() + (screen.height().saturating_sub(panel_h) / 2) as i32,
                panel_w,
                panel_h,
            );
            Self::draw_rect(display, clip, panel, Color::rgb(0x20, 0x2A, 0x38));
            Self::draw_border_rect(display, clip, panel, Color::rgb(0x90, 0xA4, 0xB8), 1);
            Self::draw_label(
                display,
                clip,
                font,
                "Ziqa Native Desktop",
                panel.left() + 20,
                panel.top() + 20,
                Color::rgb(0xE7, 0xE7, 0xE7),
            );
            Self::draw_label(
                display,
                clip,
                font,
                "Orbital renderer is now the Ziqa GUI shell.",
                panel.left() + 20,
                panel.top() + 52,
                Color::rgb(0xB8, 0xC7, 0xD6),
            );
            Self::draw_label(
                display,
                clip,
                font,
                "No Redox launcher, inputd, or namespace server is required here.",
                panel.left() + 20,
                panel.top() + 78,
                Color::rgb(0xB8, 0xC7, 0xD6),
            );
            Self::draw_label(
                display,
                clip,
                font,
                "Click Terminal, Files, or Settings in the dock.",
                panel.left() + 20,
                panel.top() + 104,
                Color::rgb(0x8E, 0xA7, 0xC2),
            );
        }
    }

    /// Nucleus desktop background — calm, technical, low-contrast. Painted
    /// over the caller's flat clear, anchored to `screen`, clipped to `clip`
    /// so a tiny dirty rect (e.g. a cursor move) only touches the few
    /// primitives that intersect it. All tones are opaque pre-mixed colors
    /// (no alpha blending), so every layer is a cheap solid fill.
    fn draw_desktop_background(display: &mut Display, clip: &Rect, screen: &Rect) {
        let w = screen.width();
        let h = screen.height();
        let left = screen.left();
        let top = screen.top();

        // Layer 1 — subtle vertical gradient, 8px bands. Only the bands
        // overlapping the dirty region are visited.
        let band_h: i32 = 8;
        let n_bands = ((h as i32) + band_h - 1) / band_h;
        let bg_top = (0x13i32, 0x1fi32, 0x2ei32);
        let bg_bot = (0x0bi32, 0x13i32, 0x1ei32);
        let band0 = (((clip.top() - top) / band_h).max(0)).min(n_bands.max(1));
        let band1 = (((clip.bottom() - top + band_h - 1) / band_h).max(0)).min(n_bands.max(1));
        for i in band0..band1 {
            let r = (bg_top.0 + ((bg_bot.0 - bg_top.0) * i) / n_bands) as u8;
            let g = (bg_top.1 + ((bg_bot.1 - bg_top.1) * i) / n_bands) as u8;
            let b = (bg_top.2 + ((bg_bot.2 - bg_top.2) * i) / n_bands) as u8;
            Self::draw_rect(
                display,
                clip,
                Rect::new(left, top + i * band_h, w, band_h as u32),
                Color::rgb(r, g, b),
            );
        }

        // Layer 2 — faint grid every 48px. Only the lines crossing the dirty
        // region are visited.
        let grid = Color::rgb(0x17, 0x24, 0x35);
        let spacing: i32 = 48;
        let mut j = ((clip.left() - left) / spacing).max(0);
        let x_max = (clip.right() - 1 - left).max(0);
        while left + j * spacing <= x_max {
            Self::draw_rect(
                display,
                clip,
                Rect::new(left + j * spacing, top, 1, h),
                grid,
            );
            j += 1;
        }
        let mut k = ((clip.top() - top) / spacing).max(0);
        let y_max = (clip.bottom() - 1 - top).max(0);
        while top + k * spacing <= y_max {
            Self::draw_rect(
                display,
                clip,
                Rect::new(left, top + k * spacing, w, 1),
                grid,
            );
            k += 1;
        }

        // Layer 3 — large low-contrast nucleus mark, centered.
        // ponytail: draw_diamond loops `size` bands per frame even for tiny
        // dirty rects; at 150 bands this is ~9k trivial intersection tests/sec
        // — fine. Band-range-clamp if it ever shows in profiling.
        let cx = left + (w as i32) / 2;
        let cy = top + (h as i32) / 2;
        Self::draw_diamond(display, clip, cx, cy, 150, Color::rgb(0x16, 0x30, 0x4a));
        Self::draw_diamond(display, clip, cx, cy, 64, Color::rgb(0x1d, 0x3c, 0x5c));

        // Layer 4 — static, deterministic field dots (no randomness).
        let dot = Color::rgb(0x1f, 0x33, 0x54);
        const DOTS: [(i32, i32); 11] = [
            (120, 90),
            (310, 160),
            (520, 70),
            (700, 210),
            (900, 120),
            (1080, 200),
            (180, 360),
            (1050, 420),
            (260, 540),
            (820, 600),
            (640, 720),
        ];
        for &(dx, dy) in &DOTS {
            Self::draw_dot(display, clip, left + dx, top + dy, 2, dot);
        }
    }

    pub fn hit_test(&self, x: i32, y: i32, screen: &Rect) -> ShellAction {
        // Geometry must match draw_desktop exactly (same consts).
        let dock_left = screen.left() + DOCK_PAD as i32;
        let dock_top =
            screen.top() + screen.height().saturating_sub(DOCK_BOTTOM_GAP + DOCK_H) as i32;
        let start_x = dock_left + 14;
        let slot_y = dock_top;
        let slot_h = DOCK_H + 16; // icon plus its label row

        let in_slot = |i: i32| {
            Rect::new(
                start_x + DOCK_SPACING as i32 * i,
                slot_y,
                DOCK_SPACING,
                slot_h,
            )
            .contains(x, y)
        };
        if in_slot(0) {
            ShellAction::Launch(NativeAppKind::Terminal)
        } else if in_slot(1) {
            ShellAction::Launch(NativeAppKind::Files)
        } else if in_slot(2) {
            ShellAction::Launch(NativeAppKind::Settings)
        } else {
            ShellAction::None
        }
    }

    pub fn register_app(&mut self, id: WindowId, kind: NativeAppKind) {
        self.apps.insert(
            id,
            NativeAppState {
                kind,
                lines: Vec::new(),
                input: String::new(),
            },
        );
    }

    pub fn has_app(&self, id: WindowId) -> bool {
        self.apps.contains_key(&id)
    }

    pub fn render_app(&mut self, id: WindowId, window: &mut Window, font: &Font) {
        let Some(app) = self.apps.get_mut(&id) else {
            return;
        };
        let image = window.image_mut();
        Self::clear_image(image, Color::rgb(0x18, 0x18, 0x20));

        match app.kind {
            NativeAppKind::Terminal => Self::render_terminal(app, image, font),
            NativeAppKind::Files => Self::render_files(image, font),
            NativeAppKind::Settings => Self::render_settings(image, font),
        }
    }

    pub fn handle_key(
        &mut self,
        id: WindowId,
        event: orbclient::KeyEvent,
        _window: &mut Window,
        _font: &Font,
    ) -> bool {
        let changed = {
            let Some(app) = self.apps.get_mut(&id) else {
                return false;
            };
            if app.kind != NativeAppKind::Terminal || !event.pressed {
                return false;
            }

            match event.character {
                '\n' | '\r' => {
                    app.lines.push(format!("ziqa> {}", app.input));
                    let cmd = app.input.trim().to_string();
                    Self::exec_terminal_command(app, &cmd);
                    app.input.clear();
                    true
                }
                '\x08' | '\x7f' => app.input.pop().is_some(),
                '\0' => false,
                c => {
                    app.input.push(c);
                    true
                }
            }
        };

        changed
    }

    fn render_terminal(app: &NativeAppState, image: &mut ImageAligned, font: &Font) {
        let mut y = 8;
        for line in app
            .lines
            .iter()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            Self::draw_label_image(image, font, line, 8, y, Color::rgb(0xE7, 0xE7, 0xE7));
            y += 20;
        }
        Self::draw_label_image(
            image,
            font,
            &format!("ziqa> {}", app.input),
            8,
            y,
            Color::rgb(0xE7, 0xE7, 0xE7),
        );
    }

    /// Minimal in-process command interpreter for the Terminal app.
    ///
    /// This is *not* a real shell — there is no userspace shell binary in this
    /// tree, and wiring one up requires spawning a shell ELF plus a PTY
    /// (`src/scheme/pty.rs`) back to this window. As an interim it handles a
    /// small built-in command set so the Terminal is interactive rather than a
    /// dead stub. TODO: replace with a spawned shell once a shell ELF exists.
    fn exec_terminal_command(app: &mut NativeAppState, cmd: &str) {
        let mut parts = cmd.split_whitespace();
        let name = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();
        let out = match name {
            "" => return,
            "help" => String::from("commands: help, echo <text>, ver, whoami, clear"),
            "echo" => args.join(" "),
            "ver" => String::from("ZiqaKernel 0.1.0 — native desktop (BGA direct)"),
            "whoami" => String::from("root"),
            "clear" => {
                app.lines.clear();
                return;
            }
            other => format!("ziqa: {}: command not found (try 'help')", other),
        };
        app.lines.push(out);
    }

    fn render_files(image: &mut ImageAligned, font: &Font) {
        let rows = ["/", "/bin/orbital", "/fat", "/etc/motd"];
        for (i, row) in rows.iter().enumerate() {
            Self::draw_label_image(
                image,
                font,
                row,
                8,
                8 + (i as i32 * 20),
                Color::rgb(0xE7, 0xE7, 0xE7),
            );
        }
    }

    fn render_settings(image: &mut ImageAligned, font: &Font) {
        let rows = [
            "Resolution: 1280x960",
            "Backend: BGA direct",
            "Input path: input:",
        ];
        for (i, row) in rows.iter().enumerate() {
            Self::draw_label_image(
                image,
                font,
                row,
                8,
                8 + (i as i32 * 20),
                Color::rgb(0xE7, 0xE7, 0xE7),
            );
        }
    }

    fn clear_image(image: &mut ImageAligned, color: Color) {
        for pixel in image.data_mut() {
            *pixel = color;
        }
    }

    fn draw_rect(display: &mut Display, clip: &Rect, rect: Rect, color: Color) {
        let clipped = clip.intersection(&rect);
        if !clipped.is_empty() {
            display.rect(&clipped, color);
        }
    }

    /// Draw a 1px-style border around `rect`, clipped to `clip`. Unlike
    /// `Display::border_rect` (which writes all four edges unconditionally),
    /// this only touches pixels inside the dirty region.
    fn draw_border_rect(
        display: &mut Display,
        clip: &Rect,
        rect: Rect,
        color: Color,
        thickness: u32,
    ) {
        Self::draw_rect(display, clip, rect.edge(thickness, 0, RectEdge::Top), color);
        Self::draw_rect(
            display,
            clip,
            rect.edge(thickness, 0, RectEdge::Bottom),
            color,
        );
        Self::draw_rect(
            display,
            clip,
            rect.edge(thickness, 0, RectEdge::Left),
            color,
        );
        Self::draw_rect(
            display,
            clip,
            rect.edge(thickness, 0, RectEdge::Right),
            color,
        );
    }

    fn draw_label(
        display: &mut Display,
        clip: &Rect,
        font: &Font,
        text: &str,
        x: i32,
        y: i32,
        color: Color,
    ) {
        // Cheap reject before `font.render()`: cursor damage is usually a tiny
        // rect nowhere near desktop labels. Rendering every label for every
        // mouse move allocates images and makes the cursor lag badly.
        let approx = Rect::new(x, y, (text.len() as u32).saturating_mul(16), 22);
        if clip.intersection(&approx).is_empty() {
            return;
        }

        let label = font.render(text, 16.0);
        let mut image = Image::from_color(label.width(), label.height(), Color::rgba(0, 0, 0, 0));
        image.mode().set(Mode::Overwrite);
        label.draw(&mut image, 0, 0, color);
        let label_rect = Rect::new(x, y, image.width(), image.height());
        let clipped = clip.intersection(&label_rect);
        if !clipped.is_empty() {
            display
                .roi_mut(&clipped)
                .blend(&image.roi(&clipped.translate(-label_rect.left(), -label_rect.top())));
        }
    }

    fn draw_label_image(
        image: &mut ImageAligned,
        font: &Font,
        text: &str,
        x: i32,
        y: i32,
        color: Color,
    ) {
        let label = font.render(text, 14.0);
        let mut text_image =
            Image::from_color(label.width(), label.height(), Color::rgba(0, 0, 0, 0));
        text_image.mode().set(Mode::Overwrite);
        label.draw(&mut text_image, 0, 0, color);
        let label_rect = Rect::new(x, y, text_image.width(), text_image.height());
        let bounds = Rect::new(0, 0, image.width(), image.height());
        let clipped = bounds.intersection(&label_rect);
        if !clipped.is_empty() {
            image
                .roi_mut(&clipped)
                .blend(&text_image.roi(&clipped.translate(-label_rect.left(), -label_rect.top())));
        }
    }

    /// Draw a small dot (circle approximation via filled rect — at ≤4px it
    /// looks like a dot at screen resolution).
    fn draw_dot(display: &mut Display, clip: &Rect, cx: i32, cy: i32, size: u32, color: Color) {
        let half = (size / 2) as i32;
        Self::draw_rect(
            display,
            clip,
            Rect::new(cx - half, cy - half, size, size),
            color,
        );
    }

    /// Draw a small diamond shape using horizontal rect bands.
    /// Size should be odd (3 or 5). With size=5 it's a clear diamond ◆.
    fn draw_diamond(display: &mut Display, clip: &Rect, cx: i32, cy: i32, size: u32, color: Color) {
        let half = (size / 2) as i32;
        for row in 0..size as i32 {
            let offset = if row <= half { half - row } else { row - half };
            let line_w = (size as i32) - offset * 2;
            if line_w > 0 {
                Self::draw_rect(
                    display,
                    clip,
                    Rect::new(cx - half + offset, cy - half + row, line_w as u32, 1),
                    color,
                );
            }
        }
    }

    pub fn app_kind_for_window(&self, id: WindowId) -> Option<NativeAppKind> {
        self.apps.get(&id).map(|a| a.kind)
    }
}
