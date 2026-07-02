/// Minimal Orbital terminal for ZiqaKernel.
///
/// Opens an Orbital window, reads keyboard events via orbclient,
/// draws typed characters to the window buffer.

use orbclient::{Color, EventOption, Window, WindowFlag};
use std::io::{self, Write};
use std::process;

fn main() -> io::Result<()> {
    // Create a resizable terminal window
    let mut window = match Window::new(
        false,       // not frameless
        100, 100,    // x, y
        800, 600,    // width, height
        "Terminal",  // title
        &[WindowFlag::Async],
    ) {
        Ok(w) => w,
        Err(e) => {
            let _ = writeln!(io::stdout(), "terminal: failed to create window: {}", e);
            process::exit(1);
        }
    };

    let _ = writeln!(io::stdout(), "terminal: window created");

    // Fill background
    window.set(Color::rgb(0x10, 0x18, 0x24));
    window.sync();

    let mut cursor_x: i32 = 10;
    let mut cursor_y: i32 = 10;
    let char_w: i32 = 8;
    let char_h: i32 = 16;
    let fg = Color::rgb(0xE7, 0xE7, 0xE7);
    let bg = Color::rgb(0x10, 0x18, 0x24);

    loop {
        // Poll for events (non-blocking)
        let events = window.events();
        if events.is_empty() {
            // Small sleep to avoid busy-wait
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }

        for event in events {
            match event.to_option() {
                EventOption::Key(key) => {
                    if key.pressed {
                        let ch = key.character;
                        if ch == '\n' || ch == '\r' {
                            cursor_x = 10;
                            cursor_y += char_h;
                        } else if ch == '\x08' || ch == '\x7f' {
                            // Backspace
                            if cursor_x > 10 {
                                cursor_x -= char_w;
                                window.rect(cursor_x, cursor_y - 2, char_w, char_h, bg);
                                window.sync();
                            }
                        } else if ch.is_ascii() && !ch.is_control() {
                            window.char(cursor_x, cursor_y, ch, fg);
                            window.sync();
                            cursor_x += char_w;
                        }

                        // Wrap / scroll
                        if cursor_x >= 780 {
                            cursor_x = 10;
                            cursor_y += char_h;
                        }
                        if cursor_y >= 580 {
                            cursor_y = 10;
                            window.set(bg);
                            window.sync();
                        }
                    }
                }
                EventOption::Quit(_) => {
                    let _ = writeln!(io::stdout(), "terminal: quit event received");
                    process::exit(0);
                }
                _ => {}
            }
        }
    }
}
