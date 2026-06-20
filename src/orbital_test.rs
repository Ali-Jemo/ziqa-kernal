//! Ziqa-Orbital GUI Integration Test

use crate::ziqa_orbclient::*;

pub fn run_test() {
    let window = create_window(100, 100, 400, 300, "GUI Integration Test");
    
    // Draw a button
    window.draw_rect(150, 150, 100, 40, 0xC8C8C8);
    
    window.flush();
    crate::println!("[OrbitalTest] Test complete");
}
