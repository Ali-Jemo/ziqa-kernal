use virtio_drivers::{Hal, BufferDirection};
use crate::memory::FRAME_ALLOCATOR;
use x86_64::structures::paging::FrameAllocator;
use core::ptr::NonNull;

pub struct VirtioHal;

unsafe impl Hal for VirtioHal {
    fn dma_alloc(_pages: usize, _direction: BufferDirection) -> (virtio_drivers::PhysAddr, NonNull<u8>) {
        let mut fa_guard = FRAME_ALLOCATOR.lock();
        let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
        
        let frame = fa.allocate_frame().expect("Failed to allocate DMA frame");
        
        let paddr = frame.start_address().as_u64() as usize;
        let vaddr = (paddr as u64 + crate::memory::paging::phys_offset().as_u64()) as *mut u8;
        
        (paddr, NonNull::new(vaddr).unwrap())
    }

    unsafe fn dma_dealloc(_paddr: virtio_drivers::PhysAddr, _vaddr: NonNull<u8>, _pages: usize) -> i32 {
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: virtio_drivers::PhysAddr, _size: usize) -> NonNull<u8> {
        let vaddr = (paddr as u64 + crate::memory::paging::phys_offset().as_u64()) as *mut u8;
        NonNull::new(vaddr).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> virtio_drivers::PhysAddr {
        buffer.as_ptr() as *mut u8 as usize - crate::memory::paging::phys_offset().as_u64() as usize
    }
    
    unsafe fn unshare(_paddr: virtio_drivers::PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {
    }
}
