use crate::println;
/// DOOM Fire Effect — classic fire propagation algorithm
///
/// Uses the Zig blitter for the hot-path fire step and rendering.
/// Can render to a real framebuffer (if available) or output ASCII to serial.
use alloc::vec;
use alloc::vec::Vec;

/// Fire dimensions (small enough to run fast, big enough to look good)
pub const FIRE_W: u32 = 80;
pub const FIRE_H: u32 = 50;

/// DOOM fire palette — 37 colors from black → red → orange → yellow → white
/// Each entry is XRGB8888 (0x00RRGGBB)
pub static DOOM_PALETTE: [u32; 37] = [
    0x00070707, // 0  — near black
    0x001F0707, // 1
    0x002F0F07, // 2
    0x00470F07, // 3
    0x00571707, // 4
    0x00671F07, // 5
    0x00772707, // 6
    0x007F2F07, // 7
    0x008F3707, // 8
    0x009F2F07, // 9
    0x00AF3F07, // 10
    0x00BF4707, // 11
    0x00C74707, // 12
    0x00DF4F07, // 13
    0x00DF5707, // 14
    0x00DF5707, // 15
    0x00D75F07, // 16
    0x00D7670F, // 17
    0x00CF6F0F, // 18
    0x00CF770F, // 19
    0x00CF7F0F, // 20
    0x00CF8717, // 21
    0x00C78717, // 22
    0x00C78F17, // 23
    0x00C7971F, // 24
    0x00BF9F1F, // 25
    0x00BF9F1F, // 26
    0x00BFA727, // 27
    0x00BFAF2F, // 28
    0x00B7AF2F, // 29
    0x00B7B72F, // 30
    0x00B7B737, // 31
    0x00CFCF6F, // 32
    0x00DFDF9F, // 33
    0x00EFEFC7, // 34
    0x00FFFFFF, // 35 — white
    0x00FFFFFF, // 36 — white (overflow guard)
];

pub static DOOM_PALETTE_CYCLED: [u32; 37] = [
    0x00070707,
    0x001F0707,
    0x002F0F07,
    0x004F0F07,
    0x005F1707,
    0x00771F07,
    0x00872707,
    0x00972F07,
    0x00AF3F0F,
    0x00BF4707,
    0x00CF4F0F,
    0x00D7570F,
    0x00DF5F17,
    0x00DF6717,
    0x00DF6F1F,
    0x00D7771F,
    0x00CF7F27,
    0x00CF8727,
    0x00C78F2F,
    0x00C79737,
    0x00BF9F3F,
    0x00BFA747,
    0x00B7AF4F,
    0x00B7B757,
    0x00AFBF5F,
    0x00AFC767,
    0x00A7CF6F,
    0x00A7D777,
    0x009FDF7F,
    0x009FE787,
    0x0097EF8F,
    0x0097F797,
    0x00AFFF9F,
    0x00BFFFAF,
    0x00DFFFCF,
    0x00FFFFFF,
    0x00FFFFFF,
];

/// Initialize the fire buffer: all zeros except the bottom row = max intensity.
pub fn init_fire_buf(buf: &mut [u8], w: u32, h: u32) {
    for b in buf.iter_mut() {
        *b = 0;
    }
    let bottom_start = (h - 1) * w;
    for x in 0..w {
        buf[(bottom_start + x) as usize] = 36;
    }
}

const LOGO_ROWS: [&str; 5] = [
    "ZZZZZ III  QQQ   A  ",
    "   Z   I  Q   Q A A ",
    "  Z    I  Q   Q AAAAA",
    " Z     I  Q  QQ A   A",
    "ZZZZZ III  QQQQ A   A",
];

fn stamp_ziqa_logo(buf: &mut [u8], w: u32, h: u32, tick: usize) {
    let logo_w = LOGO_ROWS.iter().map(|row| row.len()).max().unwrap_or(0) as u32;
    let x0 = w.saturating_sub(logo_w) / 2;
    let y0 = 3;
    for (row, pattern) in LOGO_ROWS.iter().enumerate() {
        let y = y0 + row as u32;
        if y >= h { break; }
        for (col, b) in pattern.as_bytes().iter().enumerate() {
            if *b == b' ' { continue; }
            let x = x0 + col as u32;
            if x >= w { continue; }
            let shimmer = ((tick + row + col) & 0x3) as u8;
            buf[(y * w + x) as usize] = 31 + shimmer;
        }
    }
}

fn lerp_color(a: u32, b: u32, t: u32) -> u32 {
    let ar = ((a >> 16) & 0xFF) as i32;
    let ag = ((a >> 8) & 0xFF) as i32;
    let ab = (a & 0xFF) as i32;
    let br = ((b >> 16) & 0xFF) as i32;
    let bg = ((b >> 8) & 0xFF) as i32;
    let bb = (b & 0xFF) as i32;
    let t = t as i32;
    let r = (ar + (br - ar) * t / 255) as u32;
    let g = (ag + (bg - ag) * t / 255) as u32;
    let b2 = (ab + (bb - ab) * t / 255) as u32;
    (r << 16) | (g << 8) | b2
}

fn make_ember_pack(
    x: u16, y: u16, color: u32, size: u16,
) -> [u8; 12] {
    [
        x as u8, (x >> 8) as u8,
        y as u8, (y >> 8) as u8,
        color as u8, (color >> 8) as u8, (color >> 16) as u8, (color >> 24) as u8,
        size as u8, (size >> 8) as u8,
        0, 0,
    ]
}

/// Run the DOOM fire demo for `steps` iterations, outputting ASCII to serial.
pub fn run_serial(steps: usize) {
    println!("\n━━━ ZIQA Inferno (Zig-powered fire + embers + tornado + smoke, {} steps) ━━━", steps);

    let buf_size = (FIRE_W * FIRE_H) as usize;
    let mut fire_buf: Vec<u8> = vec![0u8; buf_size];
    init_fire_buf(&mut fire_buf, FIRE_W, FIRE_H);

    let fb_pitch = FIRE_W * 4;
    let fb_size = (fb_pitch * FIRE_H) as usize;
    let mut dummy_fb: Vec<u8> = vec![0u8; fb_size];

    let ascii_buf_size = ((FIRE_W + 1) * (FIRE_H / 2) + 16) as usize;
    let mut ascii_buf: Vec<u8> = vec![0u8; ascii_buf_size];

    // ── Enhanced animation state ──
    let max_embers: usize = 32;
    let mut ember_x: Vec<u16> = vec![0u16; max_embers];
    let mut ember_y: Vec<u16> = vec![0u16; max_embers];
    let mut ember_vx: Vec<i16> = vec![0i16; max_embers];
    let mut ember_vy: Vec<i16> = vec![0i16; max_embers];
    let mut ember_life: Vec<u8> = vec![0u8; max_embers];
    let mut ember_hue: Vec<u8> = vec![0u8; max_embers];
    let mut ember_pack: Vec<u8> = vec![0u8; max_embers * 12];
    let mut palette_phase: u32 = 0;
    let mut gust_timer: u32 = 0;
    let mut gust_wind: i32 = 0;

    // ── Smoke state ──
    let max_smoke: usize = 16;
    let mut smoke_x: Vec<u16> = vec![0u16; max_smoke];
    let mut smoke_y: Vec<u16> = vec![0u16; max_smoke];
    let mut smoke_vx: Vec<i16> = vec![0i16; max_smoke];
    let mut smoke_vy: Vec<i16> = vec![0i16; max_smoke];
    let mut smoke_life: Vec<u8> = vec![0u8; max_smoke];
    let mut smoke_size: Vec<u8> = vec![0u8; max_smoke];
    let mut smoke_pack: Vec<u8> = vec![0u8; max_smoke * 12];

    // ── Fire tornado state ──
    let mut tornado_active: bool = false;
    let mut tornado_timer: u32 = 0;
    let tornado_center_x: u32 = FIRE_W / 2;

    // ── Keyboard interaction ──
    let mut blow_strength: i32 = 0;

    for step in 0..steps {
        let s = step as u32;
        let mut extra_wind: i32 = 0;

        // ── Keyboard interaction ──────────────────────────────────────────
        let mut key_buf = [0u8; 4];
        let nread = crate::drivers::keyboard::read_stdin(&mut key_buf);
        for i in 0..nread {
            match key_buf[i] {
                b' ' => blow_strength = 8,
                b't' | b'T' => tornado_active = !tornado_active,
                _ => {}
            }
        }
        if blow_strength > 0 {
            blow_strength -= 1;
            extra_wind = blow_strength;
        }

        // ── Fire tornado ──────────────────────────────────────────────────
        if tornado_active {
            tornado_timer = tornado_timer.wrapping_add(1);
            // Spiral convection: suck fire upward at center, rotate around
            // This affects both the fire_buf directly and the wind
            let t_angle = (tornado_timer as usize).wrapping_mul(3) / 20;
            let t_wind = (t_angle % 5) as i32 - 2; // oscillate -2..2
            extra_wind += t_wind;

            // Inject extra heat in the center column of the fire
            let center_w = 8usize;
            let start_x = (tornado_center_x as usize).saturating_sub(center_w / 2);
            for fy in (FIRE_H as usize / 2)..FIRE_H as usize {
                for fx in start_x..(start_x + center_w).min(FIRE_W as usize) {
                    let idx = fy * FIRE_W as usize + fx;
                    if idx < fire_buf.len() {
                        // Boost heat in the tornado column
                        let boost = (fy * 10 / FIRE_H as usize) as u8;
                        fire_buf[idx] = fire_buf[idx].saturating_add(boost).min(36);
                    }
                }
            }
        }

        // ── Wind gusts ────────────────────────────────────────────────────
        gust_timer += 1;
        if gust_timer > 30 {
            gust_timer = 0;
            gust_wind = match (s / 60) % 3 {
                0 => 0,
                1 => 2,
                _ => -2,
            };
        }
        let base_wind: i32 = match (s / 12) % 5 {
            0 => -1, 1 => 0, 2 => 1, 3 => 1, _ => 0,
        };
        let wind = base_wind + gust_wind + extra_wind;

        // ── Palette animation ─────────────────────────────────────────────
        palette_phase = palette_phase.wrapping_add(1);
        let cycle = (palette_phase / 4) % 37;
        let mut animated_palette = [0u32; 37];
        for i in 0..37 {
            let src_i = (i + cycle as usize) % 37;
            animated_palette[i] = DOOM_PALETTE[src_i];
        }

        // ── Fire step ─────────────────────────────────────────────────────
        crate::zig_ffi::doom_fire_step_seeded(
            dummy_fb.as_mut_ptr(),
            fb_pitch,
            FIRE_W,
            FIRE_H,
            &animated_palette,
            &mut fire_buf,
            s,
            wind,
        );

        // ── Ember particles ───────────────────────────────────────────────
        for i in 0..max_embers {
            if ember_life[i] == 0 {
                if (s.wrapping_add(i as u32 * 7) & 0x3) == 0 {
                    ember_x[i] = (crate::zig_ffi::hash32(s + i as u32 * 13) % FIRE_W) as u16;
                    ember_y[i] = (FIRE_H - 2) as u16;
                    ember_vx[i] = ((crate::zig_ffi::hash32(s + i as u32 * 17) % 7) as i16) - 3;
                    ember_vy[i] = -(((crate::zig_ffi::hash32(s + i as u32 * 19) % 5) as i16) + 1);
                    ember_life[i] = 60 + (crate::zig_ffi::hash32(s + i as u32 * 23) % 40) as u8;
                    ember_hue[i] = (crate::zig_ffi::hash32(s + i as u32 * 29) % 4) as u8;
                }
            } else {
                // Apply tornado vortex force to embers
                if tornado_active {
                    let dx = (ember_x[i] as i32) - (tornado_center_x as i32);
                    let dist = dx.abs() as u32;
                    if dist < 20 {
                        // Spiral: tangential velocity + upward suction
                        let tang = if dx > 0 { -2 } else { 2 };
                        ember_vx[i] = (ember_vx[i] as i32 + tang) as i16;
                        ember_vy[i] = (ember_vy[i] as i32 - 2) as i16; // pull up
                    }
                }
                ember_x[i] = (ember_x[i] as i32 + ember_vx[i] as i32) as u16;
                if ember_vx[i] > 0 { ember_vx[i] -= 1; }
                else if ember_vx[i] < 0 { ember_vx[i] += 1; }
                ember_y[i] = (ember_y[i] as i32 + ember_vy[i] as i32) as u16;
                ember_vy[i] -= 1;
                ember_life[i] = ember_life[i].saturating_sub(1);
                if ember_y[i] > (FIRE_H - 1) as u16 || ember_x[i] > (FIRE_W - 1) as u16 {
                    ember_life[i] = 0;
                }
            }
        }

        // Pack embers for Zig draw
        let mut ember_count: u32 = 0;
        for i in 0..max_embers {
            if ember_life[i] > 0 {
                let life_ratio = ember_life[i] as u32 * 255 / 100;
                let colors: [u32; 4] = [0xFFFFCC, 0xFFAA44, 0xFF6600, 0xFF4400];
                let ci = (ember_hue[i] as usize) % 4;
                let color = lerp_color(colors[ci], 0xFF2200, if life_ratio < 128 { life_ratio * 2 } else { 255 - (life_ratio - 128) * 2 });
                let sz: u16 = if ember_life[i] > 40 { 4 } else if ember_life[i] > 20 { 3 } else { 2 };
                let pack = make_ember_pack(ember_x[i], ember_y[i], color, sz);
                let offset = ember_count as usize * 12;
                if offset + 12 <= ember_pack.len() {
                    ember_pack[offset..offset + 12].copy_from_slice(&pack);
                }
                ember_count += 1;
            }
        }
        if ember_count > 0 {
            crate::zig_ffi::draw_embers(
                dummy_fb.as_mut_ptr(),
                fb_pitch,
                FIRE_W,
                FIRE_H,
                &ember_pack[..ember_count as usize * 12],
                ember_count,
                200,
            );
        }

        // ── Smoke particles ───────────────────────────────────────────────
        for i in 0..max_smoke {
            if smoke_life[i] == 0 {
                if (s.wrapping_add(i as u32 * 11) & 0x7) == 0 {
                    smoke_x[i] = (crate::zig_ffi::hash32(s + i as u32 * 31) % FIRE_W) as u16;
                    smoke_y[i] = (crate::zig_ffi::hash32(s + i as u32 * 37) % 8) as u16; // near top
                    smoke_vx[i] = ((crate::zig_ffi::hash32(s + i as u32 * 41) % 5) as i16) - 2;
                    smoke_vy[i] = -(((crate::zig_ffi::hash32(s + i as u32 * 43) % 3) as i16) + 1);
                    smoke_life[i] = 30 + (crate::zig_ffi::hash32(s + i as u32 * 47) % 20) as u8;
                    smoke_size[i] = 3 + (crate::zig_ffi::hash32(s + i as u32 * 53) % 4) as u8;
                }
            } else {
                smoke_x[i] = (smoke_x[i] as i32 + smoke_vx[i] as i32 + wind) as u16;
                smoke_y[i] = (smoke_y[i] as i32 + smoke_vy[i] as i32) as u16;
                if smoke_vy[i] > -5 { smoke_vy[i] -= 1; }
                smoke_life[i] = smoke_life[i].saturating_sub(1);
                if smoke_y[i] > (FIRE_H - 1) as u16 || smoke_x[i] > (FIRE_W - 1) as u16 {
                    smoke_life[i] = 0;
                }
            }
        }

        // Pack smoke for Zig draw
        let mut smoke_count: u32 = 0;
        for i in 0..max_smoke {
            if smoke_life[i] > 0 {
                let life_ratio = smoke_life[i] as u32 * 255 / 50;
                let gray = 40 + (life_ratio * 80 / 255);
                let color: u32 = (gray << 16) | (gray << 8) | gray;
                let sz = smoke_size[i] as u16;
                let pack = make_ember_pack(smoke_x[i], smoke_y[i], color, sz);
                let offset = smoke_count as usize * 12;
                if offset + 12 <= smoke_pack.len() {
                    smoke_pack[offset..offset + 12].copy_from_slice(&pack);
                }
                smoke_count += 1;
            }
        }
        if smoke_count > 0 {
            crate::zig_ffi::draw_embers(
                dummy_fb.as_mut_ptr(),
                fb_pitch,
                FIRE_W,
                FIRE_H,
                &smoke_pack[..smoke_count as usize * 12],
                smoke_count,
                120,
            );
        }

        // ── Apply scanline + vignette to framebuffer ──────────────────────
        crate::zig_ffi::scanline_overlay(dummy_fb.as_mut_ptr(), fb_pitch, FIRE_W, FIRE_H, 20, s);
        crate::zig_ffi::vignette(dummy_fb.as_mut_ptr(), fb_pitch, FIRE_W, FIRE_H, FIRE_H / 3, 60);

        // ── ZIQA wordmark ─────────────────────────────────────────────────
        stamp_ziqa_logo(&mut fire_buf, FIRE_W, FIRE_H, step);

        // ── Ember particles also burn into the fire buffer ─────────────────
        for i in 0..max_embers {
            if ember_life[i] > 0 {
                let fx = ember_x[i] as usize;
                let fy = ember_y[i] as usize;
                if fy < FIRE_H as usize && fx < FIRE_W as usize {
                    let idx = fy * FIRE_W as usize + fx;
                    if idx < fire_buf.len() {
                        fire_buf[idx] = 36u8.saturating_sub(ember_life[i] >> 2);
                    }
                }
            }
        }

        // ── ASCII output ──────────────────────────────────────────────────
        if step % 3 == 0 || step == steps - 1 {
            let written = crate::zig_ffi::doom_fire_to_ascii(&fire_buf, FIRE_W, FIRE_H, &mut ascii_buf);
            if written > 0 {
                if let Ok(s) = core::str::from_utf8(&ascii_buf[..written]) {
                    let tornado_str = if tornado_active { "TORNADO" } else { "    " };
                    println!("── frame {:03} | wind {:+} | {} embers | {} smoke | {} ──", step, wind, ember_count, smoke_count, tornado_str);
                    crate::print!("{}", s);
                }
            }
        }
    }
    println!("━━━ ZIQA Inferno complete ({} steps) ━━━", steps);
}

/// Run the fire demo with framebuffer rendering.
/// Press SPACE to blow the fire, T to toggle fire tornado.
pub fn run(steps: usize) {
    crate::drivers::keyboard::clear_stdin();
    run_serial(steps);
}
