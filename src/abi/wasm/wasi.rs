use crate::abi::wasm::{WasmInstance};
use crate::capability::ResourceKind;
use crate::process::Process;

pub fn wasi_fd_write(process: &Process, instance: &mut WasmInstance, args: &[i32]) -> Result<Option<i32>, &'static str> {
    if !process.capabilities.has_permission(ResourceKind::File, true, false) {
        return Err("Permission denied: No write access to file");
    }
    
    let fd = args[0];
    let iovs_ptr = args[1] as usize;
    let iovs_len = args[2] as usize;
    let nwritten_ptr = args[3] as usize;

    let mut nwritten = 0u32;
    for i in 0..iovs_len {
        let off = iovs_ptr + i * 8;
        if off + 8 > instance.memory.len() {
            return Err("Memory read out of bounds on iovs");
        }
        let buf_ptr = u32::from_le_bytes([
            instance.memory[off],
            instance.memory[off + 1],
            instance.memory[off + 2],
            instance.memory[off + 3],
        ]) as usize;
        let buf_len = u32::from_le_bytes([
            instance.memory[off + 4],
            instance.memory[off + 5],
            instance.memory[off + 6],
            instance.memory[off + 7],
        ]) as usize;

        if buf_ptr + buf_len > instance.memory.len() {
            return Err("Memory read out of bounds on buffer");
        }

        let slice = &instance.memory[buf_ptr..buf_ptr + buf_len];

        if fd == 1 || fd == 2 {
            for &b in slice {
                crate::print!("{}", b as char);
            }
        }
        nwritten += buf_len as u32;
    }

    if nwritten_ptr + 4 > instance.memory.len() {
        return Err("Memory write out of bounds on nwritten");
    }
    instance.memory[nwritten_ptr..nwritten_ptr + 4].copy_from_slice(&nwritten.to_le_bytes());

    Ok(Some(0))
}

pub fn wasi_fd_read(process: &Process, instance: &mut WasmInstance, args: &[i32]) -> Result<Option<i32>, &'static str> {
    if !process.capabilities.has_permission(ResourceKind::File, false, false) {
        return Err("Permission denied: No read access to file");
    }

    let fd = args[0];
    let iovs_ptr = args[1] as usize;
    let iovs_len = args[2] as usize;
    let nread_ptr = args[3] as usize;

    let mut nread = 0u32;
    for i in 0..iovs_len {
        let off = iovs_ptr + i * 8;
        if off + 8 > instance.memory.len() {
            return Err("Memory read out of bounds on iovs");
        }
        let buf_ptr = u32::from_le_bytes([
            instance.memory[off],
            instance.memory[off + 1],
            instance.memory[off + 2],
            instance.memory[off + 3],
        ]) as usize;
        let buf_len = u32::from_le_bytes([
            instance.memory[off + 4],
            instance.memory[off + 5],
            instance.memory[off + 6],
            instance.memory[off + 7],
        ]) as usize;

        if buf_ptr + buf_len > instance.memory.len() {
            return Err("Memory read out of bounds on buffer");
        }

        if fd == 0 {
            let mut temp = vec![0u8; buf_len];
            let count = crate::drivers::keyboard::read_stdin(&mut temp);
            instance.memory[buf_ptr..buf_ptr + count].copy_from_slice(&temp[..count]);
            nread += count as u32;
            if count < buf_len {
                break;
            }
        }
    }

    if nread_ptr + 4 > instance.memory.len() {
        return Err("Memory write out of bounds on nread");
    }
    instance.memory[nread_ptr..nread_ptr + 4].copy_from_slice(&nread.to_le_bytes());

    Ok(Some(0))
}

pub fn wasi_args_sizes_get(instance: &mut WasmInstance, args: &[i32]) -> Result<Option<i32>, &'static str> {
    let argc_ptr = args[0] as usize;
    let argv_buf_size_ptr = args[1] as usize;

    if argc_ptr + 4 > instance.memory.len() || argv_buf_size_ptr + 4 > instance.memory.len() {
        return Err("Memory write out of bounds on args_sizes_get");
    }

    let argc = 2u32;
    let argv_buf_size = 20u32; // "hello.wasm\0" (11) + "test-arg\0" (9) = 20

    instance.memory[argc_ptr..argc_ptr + 4].copy_from_slice(&argc.to_le_bytes());
    instance.memory[argv_buf_size_ptr..argv_buf_size_ptr + 4].copy_from_slice(&argv_buf_size.to_le_bytes());

    Ok(Some(0))
}

pub fn wasi_args_get(instance: &mut WasmInstance, args: &[i32]) -> Result<Option<i32>, &'static str> {
    let argv_ptr = args[0] as usize;
    let argv_buf_ptr = args[1] as usize;

    let args_list = ["hello.wasm", "test-arg"];
    let mut current_buf_offset = argv_buf_ptr;

    for (i, arg_str) in args_list.iter().enumerate() {
        let bytes = arg_str.as_bytes();
        let len = bytes.len();

        if current_buf_offset + len + 1 > instance.memory.len() {
            return Err("Memory write out of bounds on args_get buffer");
        }

        instance.memory[current_buf_offset..current_buf_offset + len].copy_from_slice(bytes);
        instance.memory[current_buf_offset + len] = 0;

        let ptr_off = argv_ptr + i * 4;
        if ptr_off + 4 > instance.memory.len() {
            return Err("Memory write out of bounds on args_get argv");
        }

        let wasm_ptr = current_buf_offset as u32;
        instance.memory[ptr_off..ptr_off + 4].copy_from_slice(&wasm_ptr.to_le_bytes());

        current_buf_offset += len + 1;
    }

    Ok(Some(0))
}

pub fn wasi_proc_exit(args: &[i32]) -> Result<Option<i32>, &'static str> {
    let status = args[0] as i64;
    crate::println!("[WASM Runtime] proc_exit called with status {}", status);
    let current_pid = crate::process::scheduler::with_current_task(|p| p.pid);
    if let Some(pid) = current_pid {
        x86_64::instructions::interrupts::without_interrupts(|| {
            crate::process::scheduler::SCHEDULER.exit_process(pid, status);
        });
    }
    Ok(Some(0))
}
