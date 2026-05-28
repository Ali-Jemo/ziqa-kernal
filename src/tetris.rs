/// Graphical Tetris Game using Zig assembly blitter and downsampled VGA output
///
/// Written in Rust, calling the optimized Zig blitter FFI functions for clear/draw/fill_rect,
/// and downsampling the virtual 32-bit framebuffer to the VGA text screen at 0xb8000.

use alloc::vec;
use crate::println;

static mut RNG_STATE: u32 = 12345;

fn rand_init() {
    unsafe {
        RNG_STATE = crate::timer::uptime_ticks() as u32;
    }
}

fn rand_num() -> u32 {
    unsafe {
        RNG_STATE = RNG_STATE.wrapping_mul(1103515245).wrapping_add(12345);
        RNG_STATE
    }
}

fn next_piece_idx() -> usize {
    (rand_num() % 7) as usize
}

// 7 standard Tetris pieces represented in 4x4 grids
const SHAPE_TEMPLATES: [[[u8; 4]; 4]; 7] = [
    // 0: I
    [[0,0,0,0],
     [1,1,1,1],
     [0,0,0,0],
     [0,0,0,0]],
    // 1: O
    [[1,1,0,0],
     [1,1,0,0],
     [0,0,0,0],
     [0,0,0,0]],
    // 2: T
    [[0,1,0,0],
     [1,1,1,0],
     [0,0,0,0],
     [0,0,0,0]],
    // 3: S
    [[0,1,1,0],
     [1,1,0,0],
     [0,0,0,0],
     [0,0,0,0]],
    // 4: Z
    [[1,1,0,0],
     [0,1,1,0],
     [0,0,0,0],
     [0,0,0,0]],
    // 5: J
    [[1,0,0,0],
     [1,1,1,0],
     [0,0,0,0],
     [0,0,0,0]],
    // 6: L
    [[0,0,1,0],
     [1,1,1,0],
     [0,0,0,0],
     [0,0,0,0]],
];

const PIECE_COLORS: [u32; 7] = [
    0x00FFFF, // 0: I (Cyan)
    0xFFFF00, // 1: O (Yellow)
    0x9900FF, // 2: T (Purple/Magenta)
    0x00FF00, // 3: S (Green)
    0xFF0000, // 4: Z (Red)
    0x0000FF, // 5: J (Blue)
    0xFF8800, // 6: L (Orange)
];

// Returns a rotated version of the shape grid
fn get_rotated_grid(template: &[[u8; 4]; 4], size: usize, rotation: usize) -> [[u8; 4]; 4] {
    let mut current = *template;
    for _ in 0..rotation {
        let mut next = [[0u8; 4]; 4];
        for r in 0..size {
            for c in 0..size {
                next[c][size - 1 - r] = current[r][c];
            }
        }
        current = next;
    }
    current
}

// Check if a piece fits on the board
fn can_place(board: &[[u32; 10]; 20], shape_idx: usize, rotation: usize, px: i32, py: i32) -> bool {
    let template = SHAPE_TEMPLATES[shape_idx];
    let size = if shape_idx == 0 { 4 } else if shape_idx == 1 { 2 } else { 3 };
    let rotated = get_rotated_grid(&template, size, rotation);
    
    for r in 0..size {
        for c in 0..size {
            if rotated[r][c] != 0 {
                let board_x = px + c as i32;
                let board_y = py + r as i32;
                
                // Out of bounds horizontally
                if board_x < 0 || board_x >= 10 {
                    return false;
                }
                // Out of bounds vertically (below bottom)
                if board_y >= 20 {
                    return false;
                }
                // If it's above the board, it's fine unless it hits an existing block
                if board_y >= 0 {
                    if board[board_y as usize][board_x as usize] != 0 {
                        return false;
                    }
                }
            }
        }
    }
    true
}

// Maps 32-bit colors to 16 standard VGA text mode colors
fn get_closest_vga_color(color: u32) -> u8 {
    let vga_colors: [u32; 16] = [
        0x000000, // 0: Black
        0x0000AA, // 1: Blue
        0x00AA00, // 2: Green
        0x00AAAA, // 3: Cyan
        0xAA0000, // 4: Red
        0xAA00AA, // 5: Magenta
        0xAA5500, // 6: Brown (Dark Yellow)
        0xAAAAAA, // 7: Light Gray
        0x555555, // 8: Dark Gray
        0x5555FF, // 9: Light Blue
        0x55FF55, // 10: Light Green
        0x55FFFF, // 11: Light Cyan
        0xFF5555, // 12: Light Red
        0xFF55FF, // 13: Light Magenta
        0xFFFF55, // 14: Yellow
        0xFFFFFF, // 15: White
    ];

    let r = ((color >> 16) & 0xFF) as i32;
    let g = ((color >> 8) & 0xFF) as i32;
    let b = (color & 0xFF) as i32;

    let mut best_idx = 0;
    let mut min_dist = i32::MAX;

    for (i, &vc) in vga_colors.iter().enumerate() {
        let vr = ((vc >> 16) & 0xFF) as i32;
        let vg = ((vc >> 8) & 0xFF) as i32;
        let vb = (vc & 0xFF) as i32;

        let dist = (r - vr) * (r - vr) + (g - vg) * (g - vg) + (b - vb) * (b - vb);
        if dist < min_dist {
            min_dist = dist;
            best_idx = i;
        }
    }

    best_idx as u8
}

// Pixel representation in 80x25 virtual framebuffer:
// - `0x00000000`: Blank cell/empty
// - `0xFF000000 | (bg << 16) | (fg << 8) | ascii`: Text cell
// - Any other `u32`: Graphic block with RGB color (mapped to closest VGA color and 0xDB solid block)
fn make_text_pixel(c: char, fg: u8, bg: u8) -> u32 {
    0xFF000000 | ((bg as u32) << 16) | ((fg as u32) << 8) | (c as u32 & 0xFF)
}

fn write_char_cell(v_fb: &mut [u32], x: usize, y: usize, c: char, fg: u8, bg: u8) {
    if x < 80 && y < 25 {
        v_fb[y * 80 + x] = make_text_pixel(c, fg, bg);
    }
}

fn write_string_cell(v_fb: &mut [u32], x: usize, y: usize, s: &str, fg: u8, bg: u8) {
    for (i, c) in s.chars().enumerate() {
        write_char_cell(v_fb, x + i, y, c, fg, bg);
    }
}

fn write_number_cell(v_fb: &mut [u32], x: usize, y: usize, num: u32, fg: u8, bg: u8) {
    let mut buf = [0u8; 12];
    let mut val = num;
    let mut len = 0;
    if val == 0 {
        buf[0] = b'0';
        len = 1;
    } else {
        while val > 0 {
            buf[len] = (val % 10) as u8 + b'0';
            val /= 10;
            len += 1;
        }
    }
    for i in 0..len {
        let c = buf[len - 1 - i] as char;
        write_char_cell(v_fb, x + i, y, c, fg, bg);
    }
}

// Present the virtual 80x25 framebuffer onto the physical 80x25 VGA text screen
fn present_to_vga(v_fb: &[u32]) {
    let vga_ptr = 0xb8000 as *mut u16;
    for y in 0..25 {
        for x in 0..80 {
            let val = v_fb[y * 80 + x];
            let char_val = if val == 0 {
                // Completely blank space cell
                (b' ' as u16) | (0x00u16 << 8)
            } else if (val & 0xFF000000) == 0xFF000000 {
                // Text cell!
                let ascii = (val & 0xFF) as u8;
                let fg = ((val >> 8) & 0xFF) as u8;
                let bg = ((val >> 16) & 0xFF) as u8;
                let color_code = (bg << 4) | fg;
                (ascii as u16) | ((color_code as u16) << 8)
            } else {
                // Graphic block pixel!
                let vga_color = get_closest_vga_color(val);
                0xDBu16 | ((vga_color as u16) << 8)
            };
            unsafe {
                core::ptr::write_volatile(vga_ptr.add(y * 80 + x), char_val);
            }
        }
    }
}

// Render board, pieces, score, level, next preview, and controls to the virtual FB
pub fn render_screen(
    fb: *mut u8,
    pitch: u32,
    v_fb: &mut [u32],
    board: &[[u32; 10]; 20],
    score: u32,
    level: u32,
    lines_cleared: u32,
    next_piece: usize,
    current_piece: usize,
    current_rotation: usize,
    current_x: i32,
    current_y: i32,
    is_game_over: bool,
) {
    // 1. Clear the screen to black using the FFI blitter
    crate::zig_ffi::clear(fb, (80 * 25 * 4) as usize, 0x000000);

    // 2. Draw board boundaries using FFI blitter fill_rect
    // Center the board: width is 20 (10 columns * 2 columns per block) starting at X=30.
    // So X=29 is left border, X=50 is right border, Y=22 is bottom border.
    crate::zig_ffi::fill_rect(fb, pitch, 29, 2, 1, 20, 0x777777); // Left border
    crate::zig_ffi::fill_rect(fb, pitch, 50, 2, 1, 20, 0x777777); // Right border
    crate::zig_ffi::fill_rect(fb, pitch, 29, 22, 22, 1, 0x777777); // Bottom border

    // 3. Draw board contents as 2x1 blocks
    for y in 0..20 {
        for x in 0..10 {
            let color = board[y][x];
            if color != 0 {
                crate::zig_ffi::fill_rect(fb, pitch, 30 + x as u32 * 2, 2 + y as u32, 2, 1, color);
            }
        }
    }

    // 4. Draw active falling piece as 2x1 blocks
    if !is_game_over {
        let template = SHAPE_TEMPLATES[current_piece];
        let size = if current_piece == 0 { 4 } else if current_piece == 1 { 2 } else { 3 };
        let rotated = get_rotated_grid(&template, size, current_rotation);
        let color = PIECE_COLORS[current_piece];
        for r in 0..size {
            for c in 0..size {
                if rotated[r][c] != 0 {
                    let board_x = current_x + c as i32;
                    let board_y = current_y + r as i32;
                    if board_y >= 0 && board_y < 20 {
                        crate::zig_ffi::fill_rect(fb, pitch, 30 + board_x as u32 * 2, 2 + board_y as u32, 2, 1, color);
                    }
                }
            }
        }
    }

    // 5. Draw left UI pane using standard character cells
    // colors: fg=11 (Light Cyan), bg=0 (Black). Other labels fg=7 (Light Gray), fg=14 (Yellow), fg=10 (Light Green)
    write_string_cell(v_fb, 5, 2, "ZIQA TETRIS", 11, 0);
    
    write_string_cell(v_fb, 5, 5, "SCORE", 7, 0);
    write_number_cell(v_fb, 5, 6, score, 14, 0);
    
    write_string_cell(v_fb, 5, 9, "LEVEL", 7, 0);
    write_number_cell(v_fb, 5, 10, level, 10, 0);

    write_string_cell(v_fb, 5, 13, "LINES", 7, 0);
    write_number_cell(v_fb, 5, 14, lines_cleared, 15, 0);

    // 6. Draw right UI pane
    write_string_cell(v_fb, 55, 2, "NEXT PIECE", 7, 0);
    
    // Draw next piece preview using FFI blitter fill_rect at X=55, Y=4..8
    let next_template = SHAPE_TEMPLATES[next_piece];
    let next_size = if next_piece == 0 { 4 } else if next_piece == 1 { 2 } else { 3 };
    let next_color = PIECE_COLORS[next_piece];
    for r in 0..next_size {
        for c in 0..next_size {
            if next_template[r][c] != 0 {
                crate::zig_ffi::fill_rect(fb, pitch, 55 + c as u32 * 2, 4 + r as u32, 2, 1, next_color);
            }
        }
    }

    write_string_cell(v_fb, 55, 10, "CONTROLS:", 14, 0);
    write_string_cell(v_fb, 55, 12, "A / D  - Move L / R", 7, 0);
    write_string_cell(v_fb, 55, 14, "W      - Rotate", 7, 0);
    write_string_cell(v_fb, 55, 16, "S      - Soft Drop", 7, 0);
    write_string_cell(v_fb, 55, 18, "Q      - Quit Game", 7, 0);
    write_string_cell(v_fb, 55, 20, "R      - Restart", 7, 0);

    // 7. Overlay GAME OVER banner if necessary
    if is_game_over {
        // Red banner box using fill_rect
        crate::zig_ffi::fill_rect(fb, pitch, 33, 9, 14, 5, 0xAA0000);
        // Write text overlay on top of it. Red background is Color index 4, White text is 15.
        write_string_cell(v_fb, 35, 10, "GAME OVER", 15, 4);
        write_string_cell(v_fb, 36, 12, "PRESS R", 15, 4);
    }
}

pub fn run() {
    rand_init();
    let mut board = [[0u32; 10]; 20];
    let mut score = 0u32;
    let mut level = 1u32;
    let mut lines_cleared = 0u32;
    
    let mut current_piece = next_piece_idx();
    let mut current_rotation = 0;
    let mut current_x = 3i32;
    let mut current_y = 0i32;
    let mut next_piece = next_piece_idx();
    let mut is_game_over = false;
    
    // Allocate 32-bit virtual framebuffer (80 columns * 25 rows = 2000 pixels)
    let mut v_fb = vec![0u32; 2000];
    let fb_ptr = v_fb.as_mut_ptr() as *mut u8;
    let pitch = 320u32; // 80 pixels * 4 bytes per pixel
    
    let mut last_tick = crate::timer::uptime_ms();
    let mut tick_interval = 500u64; // Starting tick rate
    
    println!("Starting Tetris on VGA. Press Q to quit.");
    
    // Clear keyboard input buffer to discard Enter keys or previous typing
    crate::drivers::keyboard::clear_stdin();
    
    crate::drivers::vga::clear_screen();
    
    loop {
        // 1. Scan keyboard input (non-blocking)
        let mut key_buf = [0u8; 1];
        if crate::drivers::keyboard::read_stdin(&mut key_buf) > 0 {
            let key = key_buf[0];
            if key == b'q' || key == b'Q' {
                break;
            }
            if !is_game_over {
                match key {
                    b'a' | b'A' => {
                        if can_place(&board, current_piece, current_rotation, current_x - 1, current_y) {
                            current_x -= 1;
                        }
                    }
                    b'd' | b'D' => {
                        if can_place(&board, current_piece, current_rotation, current_x + 1, current_y) {
                            current_x += 1;
                        }
                    }
                    b'w' | b'W' => {
                        let next_rot = (current_rotation + 1) % 4;
                        if can_place(&board, current_piece, next_rot, current_x, current_y) {
                            current_rotation = next_rot;
                        }
                    }
                    b's' | b'S' => {
                        if can_place(&board, current_piece, current_rotation, current_x, current_y + 1) {
                            current_y += 1;
                        }
                    }
                    _ => {}
                }
            } else {
                // If game over, press R to restart
                if key == b'r' || key == b'R' {
                    board = [[0u32; 10]; 20];
                    score = 0;
                    level = 1;
                    lines_cleared = 0;
                    current_piece = next_piece_idx();
                    current_rotation = 0;
                    current_x = 3;
                    current_y = 0;
                    next_piece = next_piece_idx();
                    is_game_over = false;
                }
            }
        }
        
        // 2. Gravitational game step
        let now = crate::timer::uptime_ms();
        if !is_game_over && now - last_tick >= tick_interval {
            last_tick = now;
            
            if can_place(&board, current_piece, current_rotation, current_x, current_y + 1) {
                current_y += 1;
            } else {
                // Lock piece into board
                let template = SHAPE_TEMPLATES[current_piece];
                let size = if current_piece == 0 { 4 } else if current_piece == 1 { 2 } else { 3 };
                let rotated = get_rotated_grid(&template, size, current_rotation);
                let color = PIECE_COLORS[current_piece];
                
                for r in 0..size {
                    for c in 0..size {
                        if rotated[r][c] != 0 {
                            let board_x = current_x + c as i32;
                            let board_y = current_y + r as i32;
                            if board_y >= 0 && board_y < 20 {
                                board[board_y as usize][board_x as usize] = color;
                            }
                        }
                    }
                }
                
                // Clear filled rows
                let mut lines_this_tick = 0;
                for y in (0..20).rev() {
                    let mut is_full = true;
                    for x in 0..10 {
                        if board[y][x] == 0 {
                            is_full = false;
                            break;
                        }
                    }
                    if is_full {
                        lines_this_tick += 1;
                        for ny in (1..=y).rev() {
                            board[ny] = board[ny - 1];
                        }
                        board[0] = [0u32; 10];
                    }
                }
                
                if lines_this_tick > 0 {
                    lines_cleared += lines_this_tick;
                    score += match lines_this_tick {
                        1 => 40 * level,
                        2 => 100 * level,
                        3 => 300 * level,
                        _ => 1200 * level,
                    };
                    level = 1 + lines_cleared / 10;
                    tick_interval = (500 - (level as i64 * 35).min(400) as u64) as u64; // Speed up drop speed
                }
                
                // Spawn next piece
                current_piece = next_piece;
                current_rotation = 0;
                current_x = 3;
                current_y = 0;
                next_piece = next_piece_idx();
                
                // Game over check
                if !can_place(&board, current_piece, current_rotation, current_x, current_y) {
                    is_game_over = true;
                }
            }
        }
        
        // 3. Render screen to the virtual 32-bit framebuffer
        render_screen(
            fb_ptr,
            pitch,
            &mut v_fb,
            &board,
            score,
            level,
            lines_cleared,
            next_piece,
            current_piece,
            current_rotation,
            current_x,
            current_y,
            is_game_over,
        );
        
        // 4. Downsample and blit to the physical VGA buffer
        present_to_vga(&v_fb);
        
        // 5. Halt the CPU until the next interrupt (yielding CPU to avoid burning cycles)
        x86_64::instructions::hlt();
    }
    
    // Clear screen on exit
    crate::drivers::vga::clear_screen();
    println!("Exited Tetris. Returned to Shell.");
}
