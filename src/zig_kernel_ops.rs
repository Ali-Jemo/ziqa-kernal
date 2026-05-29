// Rust FFI bindings for ZiqaKernel kernel_ops Zig library.
//
// Safe wrappers around the C-ABI functions exported by `src/zig/kernel_ops.zig`.

extern "C" {
    fn zig_bitmap_find_clear(bitmap: *const u8, len_bytes: u32, start_bit: u32) -> u32;
    fn zig_block_copy(dst: *mut u8, src: *const u8, len: usize);
    fn zig_block_zero(dst: *mut u8, len: usize);
    fn zig_crc32(data: *const u8, len: usize, init_crc: u32) -> u32;
    fn zig_inode_find_free(buf: *const u8, count: u32, stride: u32, start_id: u32) -> u32;
    fn zig_bitmap_count_leaked(bitmap: *const u8, reachable: *const u8, start_bit: u32, end_bit: u32) -> u32;
    fn zig_inet_checksum(data: *const u8, len: usize) -> u16;
    fn zig_packet_copy(dst: *mut u8, src: *const u8, src_len: usize, max_len: usize) -> usize;
}

pub fn bitmap_find_clear(bitmap: &[u8], start_bit: u32) -> Option<u32> {
    let result = unsafe { zig_bitmap_find_clear(bitmap.as_ptr(), bitmap.len() as u32, start_bit) };
    if result == 0xFFFF_FFFF { None } else { Some(result) }
}

pub fn block_copy(dst: &mut [u8], src: &[u8]) {
    let len = dst.len().min(src.len());
    unsafe { zig_block_copy(dst.as_mut_ptr(), src.as_ptr(), len) }
}

pub fn block_zero(dst: &mut [u8]) {
    unsafe { zig_block_zero(dst.as_mut_ptr(), dst.len()) }
}

pub fn crc32(data: &[u8]) -> u32 {
    unsafe { zig_crc32(data.as_ptr(), data.len(), 0) }
}

pub fn inode_find_free(buf: &[u8], count: u32, stride: u32, start_id: u32) -> Option<u32> {
    let result = unsafe { zig_inode_find_free(buf.as_ptr(), count, stride, start_id) };
    if result == 0xFFFF_FFFF { None } else { Some(result) }
}

pub fn bitmap_count_leaked(bitmap: &[u8], reachable: &[u8], start_bit: u32, end_bit: u32) -> u32 {
    unsafe { zig_bitmap_count_leaked(bitmap.as_ptr(), reachable.as_ptr(), start_bit, end_bit) }
}

pub fn inet_checksum(data: &[u8]) -> u16 {
    unsafe { zig_inet_checksum(data.as_ptr(), data.len()) }
}

pub fn packet_copy(dst: &mut [u8], src: &[u8]) -> usize {
    unsafe { zig_packet_copy(dst.as_mut_ptr(), src.as_ptr(), src.len(), dst.len()) }
}
