const youcanvas = @import("youcanvas");
const client = @import("root.zig");
const syscall = @import("syscall.zig");

const Rect = youcanvas.Rect;
const draw_text = youcanvas.draw_text_large;

// Cover the whole 1280x960 screen but we'll use a slightly smaller surface to show compositor wallpaper
const SURFACE_W: u32 = 1000;
const SURFACE_H: u32 = 700;
const SURFACE_X: i32 = 140;
const SURFACE_Y: i32 = 130;

const NANOSLEEP: u64 = 230;
const FRAME_MS: u64 = 16; // 60 FPS cap

const C = struct {
    const wallpaper: u32 = 0x00000000; // Transparent to let compositor background show!
    const panel: u32 = 0x00F8FAFC;
    const titlebar: u32 = 0x001E293B;
    const text: u32 = 0x00F8FAFC;
    const text_dark: u32 = 0x000F172A;
    const muted: u32 = 0x0094A3B8;
    const gold: u32 = 0x00FACC15;
    const green: u32 = 0x0022C55E;
    const cyan: u32 = 0x0067E8F9;
    const red: u32 = 0x00EF4444;
};

fn sleep_ms(ms: u64) void {
    _ = syscall.syscall4(NANOSLEEP, ms, 0, 0, 0);
}

pub fn main() void {
    var client_obj = client.Client.connect(SURFACE_W, SURFACE_H, SURFACE_X, SURFACE_Y) orelse return;
    var click_count: u32 = 0;
    var key_count: u32 = 0;
    var event_count: u32 = 0;
    var marker_x: i32 = 360;
    var marker_y: i32 = 240;

    // Initial frame
    draw_desktop(client_obj.canvas, marker_x, marker_y, key_count, click_count, event_count);
    client_obj.flush(Rect.xywh(0, 0, client_obj.width, client_obj.height));

    while (true) {
        client_obj.poll();

        var changed = false;
        const next_x = client_obj.input.mouse_x;
        const next_y = client_obj.input.mouse_y;

        if (next_x != marker_x or next_y != marker_y) {
            marker_x = next_x;
            marker_y = next_y;
            event_count +%= 1;
            changed = true;
        }
        if (client_obj.input.left_pressed) {
            click_count +%= 1;
            event_count +%= 1;
            changed = true;
        }
        if (client_obj.input.key_pressed) {
            key_count +%= 1;
            event_count +%= 1;
            changed = true;
        }

        if (changed) {
            draw_desktop(client_obj.canvas, marker_x, marker_y, key_count, click_count, event_count);
            // ponytail: temporarily restore full-surface redraw/flush to isolate frame freeze issues.
            client_obj.flush(Rect.xywh(0, 0, client_obj.width, client_obj.height));
        }

        sleep_ms(FRAME_MS);
    }
}

fn draw_desktop(canvas: youcanvas.Canvas, marker_x: i32, marker_y: i32, keys: u32, clicks: u32, events: u32) void {
    // Fill surface with dark slate background
    canvas.fill(0x001E293B); 

    // App window/card - using the new rounded alpha rect!
    canvas.rounded_rect_alpha(Rect.xywh(50, 50, 800, 500), C.panel, 12, 240);
    
    // Titlebar (vertical gradient) using new gradient primitive!
    canvas.gradient_v(Rect.xywh(50, 50, 800, 40), 0x00334155, 0x000F172A);
    
    // Window controls using new circle primitive!
    canvas.circle(70, 70, 6, C.red);
    canvas.circle(92, 70, 6, C.gold);
    canvas.circle(114, 70, 6, C.green);
    
    draw_text(canvas, 140, 62, "YOU OS - HIGH RES EDITION (1280x960)", C.text);

    draw_text(canvas, 80, 120, "USERSPACE DESKTOP", C.text_dark);
    draw_text(canvas, 80, 160, "SHM IPC BGA OK", 0x002563EB);
    draw_text(canvas, 80, 200, "NEW 8x16 FONT LOADED", C.muted);

    canvas.rounded_rect_alpha(Rect.xywh(80, 250, 200, 40), if (clicks == 0) C.green else C.cyan, 8, 255);
    draw_text(canvas, 110, 262, if (clicks == 0) "SYSTEM RUNNING" else "MOUSE CLICKED", C.text);
    
    canvas.rounded_rect_alpha(Rect.xywh(300, 250, 200, 40), if (keys == 0) C.gold else C.green, 8, 255);
    draw_text(canvas, 330, 262, if (keys == 0) "WAITING FOR KEY" else "KEYBOARD ACTIVE", C.text_dark);

    // We DO NOT draw a cursor here! 
    // The kernel compositor now draws the hardware arrow cursor.
    _ = marker_x;
    _ = marker_y;

    canvas.rounded_rect_alpha(Rect.xywh(80, 320, 280, 32), if (events == 0) 0x00CBD5E1 else C.cyan, 4, 255);
    draw_text(canvas, 90, 328, "TOTAL EVENTS RECEIVED:", C.text_dark);
    draw_u32(canvas, 320, 328, events, C.text_dark);

    // Taskbar (floating dock style) using rounded rects
    canvas.rounded_rect_alpha(Rect.xywh(SURFACE_W / 2 - 200, SURFACE_H - 60, 400, 48), 0x000F172A, 16, 255);
    canvas.circle(@as(i32, @intCast(SURFACE_W / 2 - 160)), @as(i32, @intCast(SURFACE_H - 36)), 12, C.cyan);
    draw_text(canvas, @as(i32, @intCast(SURFACE_W / 2 - 130)), @as(i32, @intCast(SURFACE_H - 44)), "Start", C.text);
    draw_text(canvas, @as(i32, @intCast(SURFACE_W / 2 + 100)), @as(i32, @intCast(SURFACE_H - 44)), "12:00 PM", C.muted);
}

fn draw_u32(canvas: youcanvas.Canvas, x: i32, y: i32, value: u32, color: u32) void {
    var buf: [10]u8 = undefined;
    var n: usize = 0;
    var v = value;
    if (v == 0) {
        buf[buf.len - 1] = '0';
        n = 1;
    } else {
        while (true) {
            buf[buf.len - 1 - n] = '0' + @as(u8, @intCast(v % 10));
            n += 1;
            v /= 10;
            if (v == 0) break;
        }
    }
    draw_text(canvas, x, y, buf[buf.len - n ..], color);
}
