//! Linux eBPF (sys_bpf) syscall family.

use super::{nr, SyscallContext, AbiError};
use crate::ebpf::attach::{EBPF_ATTACHMENTS, TracepointType, BpfProgram};
use crate::ebpf::BpfInstruction;
use crate::ebpf::map::{BPF_MAPS, BpfMap, BpfMapType};
use alloc::vec::Vec;
use alloc::boxed::Box;

pub(super) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_BPF => sys_bpf(ctx),
        _ => return None,
    })
}

/// Linux bpf(cmd, attr, size) syscall
fn sys_bpf(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let cmd = ctx.args[0] as u32;
    let attr_ptr = ctx.args[1] as *const u8;
    let size = ctx.args[2] as usize;

    if attr_ptr.is_null() || size == 0 {
        return Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64);
    }

    match cmd {
        0 => sys_bpf_prog_load(attr_ptr, size),
        1 => sys_bpf_prog_attach(attr_ptr, size),
        2 => sys_bpf_map_create(attr_ptr, size),
        3 => sys_bpf_map_lookup(attr_ptr, size),
        4 => sys_bpf_map_update(attr_ptr, size),
        5 => sys_bpf_map_delete(attr_ptr, size),
        _ => {
            crate::println!("[BPF] Unsupported command: {}", cmd);
            Ok(-(crate::abi::syscall::errno::ENOSYS as i64) as u64)
        }
    }
}

/// BPF_MAP_CREATE (2)
fn sys_bpf_map_create(attr_ptr: *const u8, _size: usize) -> Result<u64, AbiError> {
    // attr: [map_type: u32, key_size: u32, value_size: u32, max_entries: u32]
    let attr = unsafe { *(attr_ptr as *const [u32; 4]) };
    let map_type = match attr[0] {
        1 => BpfMapType::Array,
        2 => BpfMapType::Hash,
        3 => BpfMapType::RingBuf,
        4 => BpfMapType::ProgArray,
        _ => return Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64),
    };
    
    let map = BpfMap::new(map_type, attr[1], attr[2], attr[3]);
    let id = BPF_MAPS.register(map);
    Ok(id as u64)
}

/// BPF_MAP_LOOKUP_ELEM (3)
fn sys_bpf_map_lookup(attr_ptr: *const u8, _size: usize) -> Result<u64, AbiError> {
    // attr: [map_id: u64, key_ptr: u64, value_ptr: u64]
    let attr = unsafe { *(attr_ptr as *const [u64; 3]) };
    let map_id = attr[0] as usize;
    let key_ptr = attr[1];
    let value_ptr = attr[2];
    
    if let Some(map) = BPF_MAPS.get(map_id) {
        match map.lookup(key_ptr) {
            Ok(ptr) if ptr != 0 => {
                // Copy result to user-provided value_ptr
                unsafe {
                    core::ptr::copy_nonoverlapping(ptr as *const u8, value_ptr as *mut u8, map.value_size as usize);
                }
                Ok(0)
            }
            _ => Ok(-(crate::abi::syscall::errno::ENOENT as i64) as u64),
        }
    } else {
        Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64)
    }
}

/// BPF_MAP_UPDATE_ELEM (4)
fn sys_bpf_map_update(attr_ptr: *const u8, _size: usize) -> Result<u64, AbiError> {
    // attr: [map_id: u64, key_ptr: u64, value_ptr: u64]
    let attr = unsafe { *(attr_ptr as *const [u64; 3]) };
    let map_id = attr[0] as usize;
    let key_ptr = attr[1];
    let value_ptr = attr[2];
    
    if let Some(map) = BPF_MAPS.get(map_id) {
        match map.update(key_ptr, value_ptr) {
            Ok(0) => Ok(0),
            _ => Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64),
        }
    } else {
        Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64)
    }
}

/// BPF_MAP_DELETE_ELEM (5)
fn sys_bpf_map_delete(attr_ptr: *const u8, _size: usize) -> Result<u64, AbiError> {
    // attr: [map_id: u64, key_ptr: u64]
    let attr = unsafe { *(attr_ptr as *const [u64; 2]) };
    let map_id = attr[0] as usize;
    let key_ptr = attr[1];
    
    if let Some(map) = BPF_MAPS.get(map_id) {
        match map.delete(key_ptr) {
            Ok(0) => Ok(0),
            _ => Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64),
        }
    } else {
        Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64)
    }
}

/// BPF_PROG_LOAD (0) - Load and verify a BPF program.
fn sys_bpf_prog_load(attr_ptr: *const u8, size: usize) -> Result<u64, AbiError> {
    if size % 8 != 0 {
        return Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64);
    }
    
    let insn_count = size / 8;
    let mut instructions = Vec::with_capacity(insn_count);
    
    unsafe {
        let src = core::slice::from_raw_parts(attr_ptr as *const BpfInstruction, insn_count);
        instructions.extend_from_slice(src);
    }
    
    if let Err(e) = crate::ebpf::verifier::BpfVerifier::new(&instructions).verify() {
        crate::println!("[BPF] Verification failed: {:?}", e);
        return Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64);
    }
    
    let prog = Box::new(BpfProgram::new(instructions));
    let handle = Box::into_raw(prog) as u64;
    
    Ok(handle)
}

/// BPF_PROG_ATTACH (1) - Attach a loaded program to a tracepoint.
fn sys_bpf_prog_attach(attr_ptr: *const u8, _size: usize) -> Result<u64, AbiError> {
    let attr = unsafe { *(attr_ptr as *const [u64; 2]) };
    let handle = attr[0];
    let tp_raw = attr[1] as u32;
    
    let tp = match tp_raw {
        0 => TracepointType::SyscallEntry,
        1 => TracepointType::SyscallExit,
        _ => return Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64),
    };
    
    let prog_ptr = handle as *mut BpfProgram;
    if prog_ptr.is_null() {
        return Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64);
    }
    
    let prog = unsafe { &*prog_ptr };
    let new_prog = BpfProgram::new(prog.instructions.clone());
    
    match EBPF_ATTACHMENTS.attach(tp, new_prog) {
        Ok(id) => Ok(id as u64),
        Err(_) => Ok(-(crate::abi::syscall::errno::EINVAL as i64) as u64),
    }
}
