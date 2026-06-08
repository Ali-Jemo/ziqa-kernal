use crate::memory::swap::{init, alloc_slot, free_slot, capacity_pages};

pub fn run_tests() {
    crate::println!("[TEST] Running swap tests...");
    init(false); // Only RAM backend for unit tests
    
    let cap = capacity_pages();
    let slot = alloc_slot().unwrap();
    assert!(slot.is_valid());
    
    let slot2 = alloc_slot().unwrap();
    assert_ne!(slot, slot2);
    
    free_slot(slot);
    let slot3 = alloc_slot().unwrap();
    assert_eq!(slot, slot3);
    
    free_slot(slot2);
    free_slot(slot3);
    
    crate::println!("[TEST] Swap tests passed!");
}
