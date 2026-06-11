#![allow(dead_code)]

use crate::drivers::vga::{self, Color};
const WIDTH: usize = 80;
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






fn wait_ms(ms: u64) {
    let start = crate::timer::uptime_ms();
    while crate::timer::uptime_ms() - start < ms {
        // Enable interrupts so the timer ISR can fire and advance the tick counter,
        // then immediately halt until the next interrupt before re-checking.
        x86_64::instructions::interrupts::enable_and_hlt();
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


pub fn show_boot_screen() {
    let mut writer = vga::WRITER.lock();
    writer.clear_screen();
    vga::hide_cursor();
    
    // Header
    clear_line(&mut writer, 0, 0, 80);
    put_str(&mut writer, 0, 0, "ZIQA Kernel Boot Display", Color::DarkGray, Color::Black);

    // Logo Area
    put_str(&mut writer, 2, 25, "  ░░░░  ZIQA KERNEL  ░░░░  v1.0  ", Color::Brown, Color::Black);
    put_str(&mut writer, 3, 25, "  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  ", Color::Brown, Color::Black);
    put_centered(&mut writer, 5, "FROM SCRATCH · FOR LEARNING · IRAQ", Color::Brown, Color::Black);

    // Phases
    let phases = [
        ("GDT / IDT / APIC", "BSP ready"),
        ("Memory", "496 MB usable · Buddy allocator"),
        ("PCI Subsystem", "7 devices detected"),
        ("VirtIO Block", "/dev/vda · 64 MB"),
    ];
    for (i, (name, detail)) in phases.iter().enumerate() {
        put_str(&mut writer, 7 + i, 2, "✓", Color::Green, Color::Black);
        put_str(&mut writer, 7 + i, 5, name, Color::LightGray, Color::Black);
        put_str(&mut writer, 7 + i, 25, detail, Color::DarkGray, Color::Black);
    }

    // Divider
    for col in 0..80 { writer.write_char_at(18, col, b'-', Color::DarkGray, Color::Black); }

    // Progress Area
    put_str(&mut writer, 20, 2, "Boot progress", Color::Brown, Color::Black);
    draw_progress(&mut writer, 21, 2, 70, 50, Color::Brown, 0);

    // Footer
    put_str(&mut writer, 23, 2, "ZiqaKernel v1.0 · x86_64", Color::DarkGray, Color::Black);
    put_str(&mut writer, 23, 60, "uptime: 0.0s", Color::DarkGray, Color::Black);
}
