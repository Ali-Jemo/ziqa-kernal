use crate::drivers::vga::{self, Color};

const WIDTH: usize = 80;
const HEIGHT: usize = 25;

const H: u8 = 0xCD;
const V: u8 = 0xBA;
const TL: u8 = 0xC9;
const TR: u8 = 0xBB;
const BL: u8 = 0xC8;
const BR: u8 = 0xBC;
const FULL: u8 = 0xDB;
const SHADE_LIGHT: u8 = 0xB0;
const SHADE_MED: u8 = 0xB1;
const SHADE_DARK: u8 = 0xB2;

const LOGO: [&str; 6] = [
    "  ZZZZZ  III   QQQQ     A    ",
    "     Z    I   Q    Q   A A   ",
    "    Z     I   Q    Q  AAAAA  ",
    "   Z      I   Q  Q Q  A   A  ",
    "  ZZZZZ  III   QQQQ   A   A  ",
    "                 Q            ",
];

struct BootStage {
    roman: &'static str,
    title: &'static str,
    color: Color,
    steps: &'static [&'static str],
}

const STAGE1_STEPS: [&str; 5] = [
    "firmware handoff accepted",
    "GDT loaded",
    "IDT vectors installed",
    "PIC remapped",
    "interrupt gate ready",
];

const STAGE2_STEPS: [&str; 6] = [
    "physical memory map captured",
    "frame allocator primed",
    "heap allocator online",
    "higher-half mapper ready",
    "DRM resources enumerated",
    "scheduler bootstrap complete",
];

const STAGE3_STEPS: [&str; 8] = [
    "ABI registry: Linux + WASM",
    "VFS root mounted",
    "ZiqaFS journal checked",
    "UART + VGA consoles active",
    "VirtIO / ATA probes complete",
    "IPC + SHM channels armed",
    "eBPF verifier standing by",
    "shell handoff prepared",
];

const BOOT_STAGES: [BootStage; 3] = [
    BootStage {
        roman: "I",
        title: "CPU + INTERRUPTS",
        color: Color::LightCyan,
        steps: &STAGE1_STEPS,
    },
    BootStage {
        roman: "II",
        title: "MEMORY + SCHEDULER",
        color: Color::LightGreen,
        steps: &STAGE2_STEPS,
    },
    BootStage {
        roman: "III",
        title: "SERVICES + SHELL",
        color: Color::Yellow,
        steps: &STAGE3_STEPS,
    },
];

fn wait_ms(ms: u64) {
    let start = crate::timer::uptime_ms();
    while crate::timer::uptime_ms() - start < ms {
        core::hint::spin_loop();
    }
}

fn put_str(writer: &mut vga::Writer, row: usize, col: usize, s: &str, fg: Color, bg: Color) {
    for (i, byte) in s.bytes().enumerate() {
        if col + i >= WIDTH { break; }
        writer.write_char_at(row, col + i, byte, fg, bg);
    }
}

fn clear_line(writer: &mut vga::Writer, row: usize, left: usize, width: usize) {
    for col in left..core::cmp::min(left + width, WIDTH) {
        writer.write_char_at(row, col, b' ', Color::LightGray, Color::Black);
    }
}

fn put_centered(writer: &mut vga::Writer, row: usize, s: &str, fg: Color, bg: Color) {
    let col = WIDTH.saturating_sub(s.len()) / 2;
    put_str(writer, row, col, s, fg, bg);
}

fn draw_box(writer: &mut vga::Writer, top: usize, left: usize, height: usize, width: usize, color: Color) {
    let bottom = top + height - 1;
    let right = left + width - 1;
    writer.write_char_at(top, left, TL, color, Color::Black);
    writer.write_char_at(top, right, TR, color, Color::Black);
    writer.write_char_at(bottom, left, BL, color, Color::Black);
    writer.write_char_at(bottom, right, BR, color, Color::Black);
    for col in (left + 1)..right {
        writer.write_char_at(top, col, H, color, Color::Black);
        writer.write_char_at(bottom, col, H, color, Color::Black);
    }
    for row in (top + 1)..bottom {
        writer.write_char_at(row, left, V, color, Color::Black);
        writer.write_char_at(row, right, V, color, Color::Black);
    }
}

fn draw_progress(writer: &mut vga::Writer, row: usize, left: usize, width: usize, filled: usize, color: Color, phase: usize) {
    writer.write_char_at(row, left, b'[', Color::DarkGray, Color::Black);
    writer.write_char_at(row, left + width + 1, b']', Color::DarkGray, Color::Black);
    for i in 0..width {
        let (ch, fg) = if i < filled {
            // Pulse the last few filled blocks for a "busy" feel
            let pulse_dist = filled.saturating_sub(i);
            if pulse_dist <= 3 && (phase + i) % 6 < 3 {
                (SHADE_MED, color)
            } else {
                (FULL, color)
            }
        } else if i == filled && filled < width {
            (SHADE_DARK, Color::White)
        } else {
            (SHADE_LIGHT, Color::DarkGray)
        };
        writer.write_char_at(row, left + 1 + i, ch, fg, Color::Black);
    }
}

fn draw_scanlines(writer: &mut vga::Writer, phase: usize) {
    for row in [1usize, 23usize] {
        for col in 2..78 {
            // Integer-based sine-like wave for scanlines (no_std safe)
            let wave = ((col * 3 + phase * 5) % 40) as i32;
            let wave_val = (wave - 20).abs().min(20);
            let ch_idx = (wave_val * 4 / 20) as usize;
            let ch = match ch_idx {
                0 => SHADE_DARK,
                1 => SHADE_MED,
                2 => SHADE_LIGHT,
                _ => b' ',
            };
            writer.write_char_at(row, col, ch, Color::DarkGray, Color::Black);
        }
    }
}

fn draw_sparkle_logo(writer: &mut vga::Writer, phase: usize) {
    let sparkle_positions: [(usize, usize); 8] = [
        (7, 21), (7, 45), (8, 41), (9, 38),
        (10, 23), (10, 52), (11, 43), (12, 54),
    ];
    for (row, col) in &sparkle_positions {
        let sparkle = ((phase + row + col) % 12) < 3;
        if sparkle {
            let ch = match (phase + row + col) % 3 {
                0 => b'.', 1 => b'*', _ => b'+',
            };
            writer.write_char_at(*row, *col, ch, Color::White, Color::Black);
        }
    }
}

fn draw_stage_tabs(writer: &mut vga::Writer, active: usize, phase: usize) {
    let tabs = [" I ", " II ", " III "];
    let mut col = 28;
    for (idx, tab) in tabs.iter().enumerate() {
        let fg = if idx == active {
            BOOT_STAGES[idx].color
        } else {
            Color::DarkGray
        };
        let bg = if idx == active {
            Color::Blue
        } else {
            Color::Black
        };
        // Active tab has a subtle glow pulse
        let glow = if idx == active && phase % 8 < 4 { Color::LightCyan } else { fg };
        put_str(writer, 5, col, tab, glow, bg);
        col += tab.len() + 2;
    }
}

fn draw_feature_grid(writer: &mut vga::Writer, active_count: usize, phase: usize) {
    let features = [
        "VFS", "ZiqaFS", "ELF", "WASM", "eBPF", "IPC", "SHM", "NET",
        "DRM", "ATA", "UART", "SHELL",
    ];
    for (i, feature) in features.iter().enumerate() {
        let row = 14 + (i / 4);
        let col = 16 + (i % 4) * 13;
        let fg = if i < active_count {
            // Pulse the most recently activated feature
            if i == active_count - 1 && phase % 6 < 3 {
                Color::White
            } else {
                Color::LightGreen
            }
        } else {
            Color::DarkGray
        };
        writer.write_char_at(row, col, b'[', fg, Color::Black);
        put_str(writer, row, col + 1, feature, fg, Color::Black);
        writer.write_char_at(row, col + 1 + feature.len(), b']', fg, Color::Black);
    }
}

fn draw_percent(writer: &mut vga::Writer, row: usize, col: usize, percent: usize, color: Color) {
    let hundreds = (percent / 100) as u8;
    let tens = ((percent / 10) % 10) as u8;
    let ones = (percent % 10) as u8;
    if hundreds > 0 {
        writer.write_char_at(row, col, b'0' + hundreds, color, Color::Black);
    } else {
        writer.write_char_at(row, col, b' ', color, Color::Black);
    }
    writer.write_char_at(row, col + 1, b'0' + tens, color, Color::Black);
    writer.write_char_at(row, col + 2, b'0' + ones, color, Color::Black);
    writer.write_char_at(row, col + 3, b'%', color, Color::Black);
}

fn draw_logo_rows(writer: &mut vga::Writer, phase: usize) {
    let logo_colors = [Color::Yellow, Color::LightCyan, Color::LightGreen, Color::LightRed, Color::Yellow, Color::DarkGray];
    for (row, line) in LOGO.iter().enumerate() {
        // Cycle logo color subtly over time
        let color_shift = (phase / 10) % 6;
        let base_color = logo_colors[(row + color_shift) % logo_colors.len()];
        // Draw each char individually for per-char shimmer
        let col = (WIDTH.saturating_sub(line.len())) / 2;
        for (ci, byte) in line.bytes().enumerate() {
            if byte == b' ' { continue; }
            let shimmer = if (phase + row + ci) % 8 < 2 { Color::White } else { base_color };
            writer.write_char_at(7 + row, col + ci, byte, shimmer, Color::Black);
        }
    }
}

pub fn show_boot_screen() {
    {
        let mut writer = vga::WRITER.lock();
        writer.clear_screen();
        vga::hide_cursor();

        draw_box(&mut writer, 0, 0, HEIGHT, WIDTH, Color::LightCyan);
        draw_box(&mut writer, 2, 6, 17, 68, Color::Blue);
        put_centered(&mut writer, 3, "ZIQA KERNEL", Color::White, Color::Black);
        put_centered(&mut writer, 4, "three-stage boot pipeline", Color::DarkGray, Color::Black);

        draw_logo_rows(&mut writer, 0);

        put_centered(&mut writer, 13, "x86_64 | Rust core | Zig blitter", Color::LightCyan, Color::Black);
        draw_feature_grid(&mut writer, 0, 0);
    }

    let bar_left = 16;
    let bar_width = 48;
    let spinner = [b'|', b'/', b'-', b'\\'];
    let mut global_frame = 0usize;

    for (stage_idx, stage) in BOOT_STAGES.iter().enumerate() {
        {
            let mut writer = vga::WRITER.lock();
            draw_stage_tabs(&mut writer, stage_idx, global_frame);
            clear_line(&mut writer, 6, 8, 64);
            put_centered(
                &mut writer,
                6,
                match stage_idx {
                    0 => "STAGE I   CPU + INTERRUPTS",
                    1 => "STAGE II  MEMORY + SCHEDULER",
                    _ => "STAGE III SERVICES + SHELL",
                },
                stage.color,
                Color::Black,
            );
        }

        for (step_idx, step) in stage.steps.iter().enumerate() {
            global_frame += 1;
            {
                let mut writer = vga::WRITER.lock();
                draw_scanlines(&mut writer, global_frame);
                draw_logo_rows(&mut writer, global_frame);
                draw_sparkle_logo(&mut writer, global_frame);

                let spin = spinner[global_frame & 3];
                writer.write_char_at(18, 20, spin, stage.color, Color::Black);
                put_str(&mut writer, 18, 22, "stage ", Color::LightGray, Color::Black);
                put_str(&mut writer, 18, 28, stage.roman, stage.color, Color::Black);
                put_str(&mut writer, 18, 34, stage.title, stage.color, Color::Black);

                clear_line(&mut writer, 19, 10, 60);
                put_str(&mut writer, 19, 18, step, Color::White, Color::Black);

                let completed_before = BOOT_STAGES[..stage_idx]
                    .iter()
                    .map(|s| s.steps.len())
                    .sum::<usize>();
                let completed = completed_before + step_idx + 1;
                let total = BOOT_STAGES.iter().map(|s| s.steps.len()).sum::<usize>();
                let filled = (completed * bar_width) / total;
                let percent = (completed * 100) / total;
                draw_progress(&mut writer, 20, bar_left, bar_width, filled, stage.color, global_frame);
                draw_percent(&mut writer, 20, 67, percent, stage.color);

                if stage_idx == 2 {
                    let active = core::cmp::min((step_idx + 1) * 12 / stage.steps.len(), 12);
                    draw_feature_grid(&mut writer, active, global_frame);
                }

                draw_stage_tabs(&mut writer, stage_idx, global_frame);
            }
            wait_ms(25);
        }
    }

    {
        let mut writer = vga::WRITER.lock();
        clear_line(&mut writer, 18, 8, 64);
        clear_line(&mut writer, 19, 8, 64);
        put_centered(&mut writer, 18, "STAGE III COMPLETE", Color::LightGreen, Color::Black);
        put_centered(&mut writer, 19, "READY - transferring control to interactive shell", Color::White, Color::Black);
        draw_progress(&mut writer, 20, bar_left, bar_width, bar_width, Color::LightGreen, global_frame);
        draw_percent(&mut writer, 20, 67, 100, Color::LightGreen);
    }
    wait_ms(200);
}
