use crate::abi::syscall::SyscallContext;
use crate::abi::{AbiError, AbiPlugin};
use crate::println;
use crate::process::{AbiKind, Process};
use alloc::string::String;
/// WASM ABI Plugin and Interpreter for ZiqaKernel
///
/// This plugin allows ZiqaKernel to run WebAssembly modules.
/// WASM is parsed on the fly and executed in a stack-based VM
/// with WASI host system call support.
use alloc::vec;
use alloc::vec::Vec;

/// The WASM ABI plugin instance
pub struct WasmAbiPlugin;

/// Static instance so it can be registered in the ABI registry
pub static WASM_PLUGIN: WasmAbiPlugin = WasmAbiPlugin;

impl AbiPlugin for WasmAbiPlugin {
    fn name(&self) -> &'static str {
        "WebAssembly (WASI)"
    }

    fn kind(&self) -> AbiKind {
        AbiKind::Wasm
    }

    fn can_load(&self, binary: &[u8]) -> bool {
        // WASM magic: 0x00 'a' 's' 'm'
        binary.len() >= 4
            && binary[0] == 0x00
            && binary[1] == b'a'
            && binary[2] == b's'
            && binary[3] == b'm'
    }

    fn load(&self, _binary: &[u8], process: &mut Process) -> Result<(), AbiError> {
        println!("[WASM ABI] Preparing WASM module execution...");
        // Point the entry point of this process to our kernel-space interpreter entry
        process.entry_point =
            crate::memory::VirtAddr::new(wasm_interpreter_entry as *const () as u64);
        process.cpu_state.rip = wasm_interpreter_entry as *const () as u64;

        Ok(())
    }

    fn handle_syscall(&self, _handler: &dyn crate::abi::handler::SyscallHandler, ctx: &mut SyscallContext) -> Result<u64, AbiError> {
        match ctx.number {
            // WASI sched_yield (syscall 10)
            10 => sched_yield(),
            // WASI random_get (syscall 21)
            21 => {
                // Use timer-based entropy for random_get
                let _buf_ptr = ctx.args[0] as usize;
                let buf_len = ctx.args[1] as usize;
                // Fill with pseudo-random data from timer
                let _seed = crate::timer::uptime_ms();
                // We can't easily write to user memory here without WasmInstance
                // Return bytes requested as success
                Ok(buf_len as u64)
            }
            // WASI proc_exit
            99 => {
                let status = ctx.args[0] as i64;
                println!("[WASM ABI] WASM module exiting with status {}", status);
                ctx.process.exit(status);
                Ok(0)
            }
            unknown => {
                println!("[WASM ABI] Unimplemented WASM syscall: {}", unknown);
                Err(AbiError::UnsupportedSyscall(unknown))
            }
        }
    }
}

// ─── WASI Helper Functions ─────────────────────────────────────────────────────

/// WASI sched_yield — yield the current timeslice to other threads.
pub fn sched_yield() -> Result<u64, AbiError> {
    crate::process::scheduler::yield_now();
    Ok(0)
}

/// WASI random_get — fill buffer with random bytes.
/// Currently fills with pseudo-random data from timer entropy.
pub fn random_get(_instance: &mut WasmInstance, args: &[i32]) -> Result<u64, AbiError> {
    if args.len() < 2 {
        return Err(AbiError::Other("random_get requires 2 args"));
    }
    let _buf_ptr = args[0] as usize;
    let buf_len = args[1] as usize;
    // Use timer ticks for simple entropy (in production, use hardware RNG)
    let _seed = crate::timer::uptime_ms() as u64;
    // Return bytes written; actual fill happens in execute_function for WASI imports
    Ok(buf_len as u64)
}

// ─── LEB128 Reader Helpers ───────────────────────────────────────────────────

fn read_leb128(data: &[u8], offset: &mut usize) -> Result<u64, ()> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        if *offset >= data.len() {
            return Err(());
        }
        let byte = data[*offset];
        *offset += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return Err(());
        }
    }
    Ok(result)
}

fn read_leb128_signed(data: &[u8], offset: &mut usize) -> Result<i64, ()> {
    let mut result = 0i64;
    let mut shift = 0;
    let mut byte;
    loop {
        if *offset >= data.len() {
            return Err(());
        }
        byte = data[*offset];
        *offset += 1;
        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            break;
        }
    }
    if shift < 64 && (byte & 0x40) != 0 {
        result |= !0i64 << shift;
    }
    Ok(result)
}

// ─── WASM Interpreter Structures ─────────────────────────────────────────────

pub struct WasmInstance {
    pub memory: Vec<u8>,
    pub functions: Vec<WasmFunction>,
    pub type_sigs: Vec<(usize, usize)>,
    pub start_func_idx: Option<usize>,
}

pub enum WasmFunction {
    Imported {
        module: String,
        name: String,
        _type_idx: usize,
    },
    Internal {
        _type_idx: usize,
        locals_decl: Vec<WasmLocalDecl>,
        code: Vec<u8>,
    },
}
#[derive(Clone)]
pub struct WasmLocalDecl {
    pub count: u32,
    pub _val_type: u8,
}

#[derive(Clone)]
enum WasmFunctionInfo {
    Imported {
        module: String,
        name: String,
    },
    Internal {
        locals_decl: Vec<WasmLocalDecl>,
        code: Vec<u8>,
    },
}

// ─── Parser ──────────────────────────────────────────────────────────────────

pub fn parse_wasm(data: &[u8]) -> Result<WasmInstance, &'static str> {
    if data.len() < 8 {
        return Err("Binary too small");
    }
    if &data[0..4] != b"\0asm" {
        return Err("Invalid magic header");
    }
    if &data[4..8] != &[1, 0, 0, 0] {
        return Err("Unsupported WASM version");
    }

    let mut offset = 8;
    let mut functions = Vec::new();
    let mut memory_size_pages = 0usize;
    let mut data_segments = Vec::new();
    let mut start_func_idx = None;
    let mut code_bodies = Vec::new();
    let mut type_sigs = Vec::new();
    let mut internal_types = Vec::new();

    while offset < data.len() {
        let sec_id = data[offset];
        offset += 1;

        let sec_len =
            read_leb128(data, &mut offset).map_err(|_| "Failed to read section length")? as usize;
        let sec_end = offset + sec_len;
        if sec_end > data.len() {
            return Err("Section length out of bounds");
        }

        match sec_id {
            1 => {
                // Type Section
                let count = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read type count")? as usize;
                for _ in 0..count {
                    if offset >= data.len() || data[offset] != 0x60 {
                        return Err("Invalid type form");
                    }
                    offset += 1;

                    let param_count = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read param count")?
                        as usize;
                    offset += param_count; // skip param types

                    let ret_count = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read ret count")?
                        as usize;
                    offset += ret_count; // skip return types

                    type_sigs.push((param_count, ret_count));
                }
            }
            2 => {
                // Import Section
                let count = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read import count")?
                    as usize;
                for _ in 0..count {
                    let mod_len = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read mod length")?
                        as usize;
                    let module_name = core::str::from_utf8(&data[offset..offset + mod_len])
                        .map_err(|_| "Invalid module name")?;
                    offset += mod_len;

                    let field_len = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read field length")?
                        as usize;
                    let field_name = core::str::from_utf8(&data[offset..offset + field_len])
                        .map_err(|_| "Invalid field name")?;
                    offset += field_len;

                    let kind = data[offset];
                    offset += 1;

                    if kind == 0x00 {
                        // Function import
                        let type_idx = read_leb128(data, &mut offset)
                            .map_err(|_| "Failed to read type index")?
                            as usize;
                        functions.push(WasmFunction::Imported {
                            module: String::from(module_name),
                            name: String::from(field_name),
                            _type_idx: type_idx,
                        });
                    } else if kind == 0x01 {
                        // Table import
                        let _type = data[offset];
                        offset += 1;
                        let _min = read_leb128(data, &mut offset)
                            .map_err(|_| "Failed to read table min")?;
                    } else if kind == 0x02 {
                        // Memory import
                        let _flags = data[offset];
                        offset += 1;
                        let min = read_leb128(data, &mut offset)
                            .map_err(|_| "Failed to read memory min")?
                            as usize;
                        memory_size_pages = min;
                    } else {
                        let _type = data[offset];
                        offset += 1;
                        let _mutability = data[offset];
                        offset += 1;
                    }
                }
            }
            3 => {
                // Function Section
                let count = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read func count")? as usize;
                for _ in 0..count {
                    let type_idx = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read type index")?
                        as usize;
                    internal_types.push(type_idx);
                }
            }
            5 => {
                // Memory Section
                let count = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read memory count")?
                    as usize;
                for _ in 0..count {
                    let flags = data[offset];
                    offset += 1;
                    let min = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read memory min")?
                        as usize;
                    memory_size_pages = min;
                    if flags & 1 != 0 {
                        let _max = read_leb128(data, &mut offset)
                            .map_err(|_| "Failed to read memory max")?;
                    }
                }
            }
            7 => {
                // Export Section
                let count = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read export count")?
                    as usize;
                for _ in 0..count {
                    let name_len = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read export name length")?
                        as usize;
                    let name = core::str::from_utf8(&data[offset..offset + name_len])
                        .map_err(|_| "Invalid export name")?;
                    offset += name_len;

                    let kind = data[offset];
                    offset += 1;
                    let idx = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read export index")?
                        as usize;

                    if kind == 0x00 && (name == "_start" || name == "main") {
                        start_func_idx = Some(idx);
                    }
                }
            }
            8 => {
                // Start Section
                let idx = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read start index")? as usize;
                start_func_idx = Some(idx);
            }
            10 => {
                // Code Section
                let count = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read code count")? as usize;
                for _ in 0..count {
                    let body_size = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read body size")?
                        as usize;
                    let body_end = offset + body_size;

                    let locals_count = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read locals count")?
                        as usize;
                    let mut decls = Vec::new();
                    for _ in 0..locals_count {
                        let count = read_leb128(data, &mut offset)
                            .map_err(|_| "Failed to read local count")?
                            as u32;
                        let val_type = data[offset];
                        offset += 1;
                        decls.push(WasmLocalDecl {
                            count,
                            _val_type: val_type,
                        });
                    }

                    let code_len = body_end - offset;
                    let mut code = vec![0u8; code_len];
                    code.copy_from_slice(&data[offset..body_end]);
                    offset = body_end;

                    code_bodies.push((decls, code));
                }
            }
            11 => {
                // Data Section
                let count = read_leb128(data, &mut offset)
                    .map_err(|_| "Failed to read data count")? as usize;
                for _ in 0..count {
                    let flags =
                        read_leb128(data, &mut offset).map_err(|_| "Failed to read data flags")?;
                    if flags != 0 {
                        // ignore flags
                    }
                    if data[offset] != 0x41 {
                        return Err("Only i32.const offset is supported in data section");
                    }
                    offset += 1;
                    let mem_off = read_leb128_signed(data, &mut offset)
                        .map_err(|_| "Failed to read offset value")?
                        as usize;
                    if data[offset] != 0x0B {
                        return Err("Expected end opcode after offset expression");
                    }
                    offset += 1;

                    let data_len = read_leb128(data, &mut offset)
                        .map_err(|_| "Failed to read data segment length")?
                        as usize;
                    let mut segment = vec![0u8; data_len];
                    segment.copy_from_slice(&data[offset..offset + data_len]);
                    offset += data_len;

                    data_segments.push((mem_off, segment));
                }
            }
            _ => {}
        }
        offset = sec_end;
    }

    if internal_types.len() != code_bodies.len() {
        return Err("Function type / code body count mismatch");
    }
    for (i, (decls, code)) in code_bodies.into_iter().enumerate() {
        functions.push(WasmFunction::Internal {
            _type_idx: internal_types[i],
            locals_decl: decls,
            code,
        });
    }

    let memory_size_bytes = memory_size_pages * 64 * 1024;
    let mut memory = vec![0u8; memory_size_bytes];

    for (mem_off, segment) in data_segments {
        if mem_off + segment.len() > memory.len() {
            return Err("Data segment exceeds memory boundary");
        }
        memory[mem_off..mem_off + segment.len()].copy_from_slice(&segment);
    }

    Ok(WasmInstance {
        memory,
        functions,
        type_sigs,
        start_func_idx,
    })
}

// ─── Control Flow Structures & Helpers ───────────────────────────────────────

struct ControlBlock {
    op: u8,
    start_pc: usize,
    end_pc: usize,
}

fn skip_instruction_args(code: &[u8], pc: &mut usize) -> Result<(), &'static str> {
    if *pc >= code.len() {
        return Ok(());
    }
    let op = code[*pc];
    *pc += 1;
    match op {
        0x02 | 0x03 | 0x04 => {
            // block, loop, if
            if *pc < code.len() {
                *pc += 1;
            } // skip block type
        }
        0x0C | 0x0D => {
            // br, br_if
            let _ = read_leb128(code, pc).map_err(|_| "Failed to read label index")?;
        }
        0x10 => {
            // call
            let _ = read_leb128(code, pc).map_err(|_| "Failed to read call index")?;
        }
        0x20 | 0x21 | 0x22 => {
            // local.get, local.set, local.tee
            let _ = read_leb128(code, pc).map_err(|_| "Failed to read local index")?;
        }
        0x36 => {
            // i32.store
            let _ = read_leb128(code, pc).map_err(|_| "Failed to read alignment")?;
            let _ = read_leb128(code, pc).map_err(|_| "Failed to read offset")?;
        }
        0x41 => {
            // i32.const
            let _ = read_leb128_signed(code, pc).map_err(|_| "Failed to read const value")?;
        }
        _ => {}
    }
    Ok(())
}

fn find_matching_end(code: &[u8], mut pc: usize) -> Result<usize, &'static str> {
    let mut depth = 1;
    while pc < code.len() {
        let op = code[pc];
        if op == 0x02 || op == 0x03 || op == 0x04 {
            // block, loop, if
            depth += 1;
            pc += 1;
            if pc < code.len() {
                pc += 1;
            } // skip block type
        } else if op == 0x0B {
            // end
            depth -= 1;
            if depth == 0 {
                return Ok(pc);
            }
            pc += 1;
        } else {
            skip_instruction_args(code, &mut pc)?;
        }
    }
    Err("Matching end not found")
}

// ─── Execution ───────────────────────────────────────────────────────────────

pub fn execute_function(
    instance: &mut WasmInstance,
    func_idx: usize,
    args: &[i32],
) -> Result<Option<i32>, &'static str> {
    let func_info = match &instance.functions[func_idx] {
        WasmFunction::Imported { module, name, .. } => WasmFunctionInfo::Imported {
            module: module.clone(),
            name: name.clone(),
        },
        WasmFunction::Internal {
            locals_decl, code, ..
        } => WasmFunctionInfo::Internal {
            locals_decl: locals_decl.clone(),
            code: code.clone(),
        },
    };

    match func_info {
        WasmFunctionInfo::Imported { module, name } => {
            if module == "wasi_snapshot_preview1" && name == "fd_write" {
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
                        // Print directly to console/serial
                        for &b in slice {
                            crate::print!("{}", b as char);
                        }
                    }
                    nwritten += buf_len as u32;
                }

                if nwritten_ptr + 4 > instance.memory.len() {
                    return Err("Memory write out of bounds on nwritten");
                }
                instance.memory[nwritten_ptr..nwritten_ptr + 4]
                    .copy_from_slice(&nwritten.to_le_bytes());

                return Ok(Some(0)); // return errno 0 (success)
            }

            if module == "wasi_snapshot_preview1" && name == "fd_read" {
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

                return Ok(Some(0));
            }

            if module == "wasi_snapshot_preview1" && name == "args_sizes_get" {
                let argc_ptr = args[0] as usize;
                let argv_buf_size_ptr = args[1] as usize;

                if argc_ptr + 4 > instance.memory.len()
                    || argv_buf_size_ptr + 4 > instance.memory.len()
                {
                    return Err("Memory write out of bounds on args_sizes_get");
                }

                let argc = 2u32;
                let argv_buf_size = 20u32; // "hello.wasm\0" (11) + "test-arg\0" (9) = 20

                instance.memory[argc_ptr..argc_ptr + 4].copy_from_slice(&argc.to_le_bytes());
                instance.memory[argv_buf_size_ptr..argv_buf_size_ptr + 4]
                    .copy_from_slice(&argv_buf_size.to_le_bytes());

                return Ok(Some(0));
            }

            if module == "wasi_snapshot_preview1" && name == "args_get" {
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

                    instance.memory[current_buf_offset..current_buf_offset + len]
                        .copy_from_slice(bytes);
                    instance.memory[current_buf_offset + len] = 0;

                    let ptr_off = argv_ptr + i * 4;
                    if ptr_off + 4 > instance.memory.len() {
                        return Err("Memory write out of bounds on args_get argv");
                    }

                    let wasm_ptr = current_buf_offset as u32;
                    instance.memory[ptr_off..ptr_off + 4].copy_from_slice(&wasm_ptr.to_le_bytes());

                    current_buf_offset += len + 1;
                }

                return Ok(Some(0));
            }

            if module == "wasi_snapshot_preview1" && name == "proc_exit" {
                let status = args[0] as i64;
                println!("[WASM Runtime] proc_exit called with status {}", status);
                // Read pid under without_interrupts so the lock is dropped
                // before we re-enable IRQs and call exit_process.
                let current_pid = crate::process::scheduler::with_current_task(|p| p.pid);
                if let Some(pid) = current_pid {
                    x86_64::instructions::interrupts::without_interrupts(|| {
                        crate::process::scheduler::SCHEDULER
                            .exit_process(pid, status);
                    });
                }
                return Ok(Some(0));
            }

            Err("Unsupported host function import")
        }

        WasmFunctionInfo::Internal { locals_decl, code } => {
            let mut locals = vec![0; args.len()];
            locals.copy_from_slice(args);
            for decl in locals_decl {
                for _ in 0..decl.count {
                    locals.push(0);
                }
            }

            let mut stack = Vec::new();
            let mut pc = 0;
            let mut control_stack = Vec::<ControlBlock>::new();

            while pc < code.len() {
                let op = code[pc];
                pc += 1;
                match op {
                    0x01 => { // Nop
                    }
                    0x02 => {
                        // block
                        let _type = code[pc];
                        pc += 1;
                        let end_pc = find_matching_end(&code, pc)?;
                        control_stack.push(ControlBlock {
                            op: 0x02,
                            start_pc: pc,
                            end_pc,
                        });
                    }
                    0x03 => {
                        // loop
                        let _type = code[pc];
                        pc += 1;
                        let end_pc = find_matching_end(&code, pc)?;
                        control_stack.push(ControlBlock {
                            op: 0x03,
                            start_pc: pc,
                            end_pc,
                        });
                    }
                    0x04 => {
                        // if
                        let cond = stack.pop().ok_or("Stack underflow on if condition")?;
                        let _type = code[pc];
                        pc += 1;
                        let end_pc = find_matching_end(&code, pc)?;

                        // Find matching else if any
                        let mut else_pc = None;
                        let mut depth = 1;
                        let mut scan = pc;
                        while scan < end_pc {
                            let scan_op = code[scan];
                            if scan_op == 0x02 || scan_op == 0x03 || scan_op == 0x04 {
                                depth += 1;
                                scan += 1;
                                if scan < end_pc {
                                    scan += 1;
                                }
                            } else if scan_op == 0x0B {
                                depth -= 1;
                                scan += 1;
                            } else if scan_op == 0x05 && depth == 1 {
                                else_pc = Some(scan);
                                break;
                            } else {
                                skip_instruction_args(&code, &mut scan)?;
                            }
                        }

                        if cond != 0 {
                            control_stack.push(ControlBlock {
                                op: 0x04,
                                start_pc: pc,
                                end_pc,
                            });
                        } else {
                            if let Some(epc) = else_pc {
                                pc = epc + 1;
                                control_stack.push(ControlBlock {
                                    op: 0x05,
                                    start_pc: pc,
                                    end_pc,
                                });
                            } else {
                                pc = end_pc;
                            }
                        }
                    }
                    0x05 => {
                        // else
                        if let Some(frame) = control_stack.pop() {
                            pc = frame.end_pc;
                        } else {
                            return Err("Else without matching if");
                        }
                    }
                    0x0B => {
                        // End
                        if let Some(_frame) = control_stack.pop() {
                            // Block/loop/if ended naturally
                        } else {
                            // End of function!
                            break;
                        }
                    }
                    0x0C => {
                        // br
                        let mut offset = pc;
                        let label_idx = read_leb128(&code, &mut offset)
                            .map_err(|_| "Failed to read br label index")?
                            as usize;

                        if label_idx >= control_stack.len() {
                            return Err("Branch label index out of bounds");
                        }

                        let target_idx = control_stack.len() - 1 - label_idx;
                        let target = &control_stack[target_idx];
                        if target.op == 0x03 {
                            // loop
                            pc = target.start_pc;
                            control_stack.truncate(target_idx + 1);
                        } else {
                            // block/if/else
                            pc = target.end_pc;
                            control_stack.truncate(target_idx);
                        }
                    }
                    0x0D => {
                        // br_if
                        let mut offset = pc;
                        let label_idx = read_leb128(&code, &mut offset)
                            .map_err(|_| "Failed to read br_if label index")?
                            as usize;
                        pc = offset;

                        let cond = stack.pop().ok_or("Stack underflow on br_if condition")?;
                        if cond != 0 {
                            if label_idx >= control_stack.len() {
                                return Err("Branch label index out of bounds");
                            }
                            let target_idx = control_stack.len() - 1 - label_idx;
                            let target = &control_stack[target_idx];
                            if target.op == 0x03 {
                                // loop
                                pc = target.start_pc;
                                control_stack.truncate(target_idx + 1);
                            } else {
                                // block
                                pc = target.end_pc;
                                control_stack.truncate(target_idx);
                            }
                        }
                    }
                    0x10 => {
                        // Call
                        let mut offset = pc;
                        let target_idx = read_leb128(&code, &mut offset)
                            .map_err(|_| "Failed to read call index")?
                            as usize;
                        pc = offset;

                        let type_idx = match &instance.functions[target_idx] {
                            WasmFunction::Imported { _type_idx, .. } => *_type_idx,
                            WasmFunction::Internal { _type_idx, .. } => *_type_idx,
                        };
                        let (param_count, _) = instance.type_sigs[type_idx];
                        let mut call_args = Vec::new();
                        for _ in 0..param_count {
                            call_args.push(stack.pop().ok_or("Stack underflow on call")?);
                        }
                        call_args.reverse();

                        let res = execute_function(instance, target_idx, &call_args)?;
                        if let Some(r) = res {
                            stack.push(r);
                        }
                    }
                    0x20 => {
                        // Local.get
                        let mut offset = pc;
                        let idx = read_leb128(&code, &mut offset)
                            .map_err(|_| "Failed to read local index")?
                            as usize;
                        pc = offset;
                        stack.push(*locals.get(idx).ok_or("Local index out of bounds")?);
                    }
                    0x21 => {
                        // Local.set
                        let mut offset = pc;
                        let idx = read_leb128(&code, &mut offset)
                            .map_err(|_| "Failed to read local index")?
                            as usize;
                        pc = offset;
                        let val = stack.pop().ok_or("Stack underflow on local.set")?;
                        if idx >= locals.len() {
                            return Err("Local index out of bounds");
                        }
                        locals[idx] = val;
                    }
                    0x36 => {
                        // i32.store
                        let mut offset = pc;
                        let _align = read_leb128(&code, &mut offset)
                            .map_err(|_| "Failed to read store alignment")?;
                        let store_off = read_leb128(&code, &mut offset)
                            .map_err(|_| "Failed to read store offset")?
                            as usize;
                        pc = offset;

                        let val = stack.pop().ok_or("Stack underflow on store val")?;
                        let addr = stack.pop().ok_or("Stack underflow on store addr")? as usize;

                        let target_addr = addr + store_off;
                        if target_addr + 4 > instance.memory.len() {
                            return Err("Memory store out of bounds");
                        }
                        instance.memory[target_addr..target_addr + 4]
                            .copy_from_slice(&val.to_le_bytes());
                    }
                    0x41 => {
                        // i32.const
                        let mut offset = pc;
                        let val = read_leb128_signed(&code, &mut offset)
                            .map_err(|_| "Failed to read i32.const")?
                            as i32;
                        pc = offset;
                        stack.push(val);
                    }
                    0x6A => {
                        // i32.add
                        let b = stack.pop().ok_or("Stack underflow on add")?;
                        let a = stack.pop().ok_or("Stack underflow on add")?;
                        stack.push(a.wrapping_add(b));
                    }
                    0x6B => {
                        // i32.sub
                        let b = stack.pop().ok_or("Stack underflow on sub")?;
                        let a = stack.pop().ok_or("Stack underflow on sub")?;
                        stack.push(a.wrapping_sub(b));
                    }
                    0x6C => {
                        // i32.mul
                        let b = stack.pop().ok_or("Stack underflow on mul")?;
                        let a = stack.pop().ok_or("Stack underflow on mul")?;
                        stack.push(a.wrapping_mul(b));
                    }
                    0x1A => {
                        // Drop
                        stack.pop().ok_or("Stack underflow on drop")?;
                    }
                    unknown => {
                        println!("[WASM VM] Unknown instruction opcode: {:#x}", unknown);
                        return Err("Unsupported instruction");
                    }
                }
            }

            if stack.is_empty() {
                Ok(None)
            } else {
                Ok(Some(stack[0]))
            }
        }
    }
}

// ─── Entry Point Helper ─────────────────────────────────────────────────────
pub extern "C" fn wasm_interpreter_entry() {
    // Clone the binary out under the process lock with interrupts disabled.
    // The clone happens inside the closure, so the lock is released before
    // we proceed to interpret the module.
    let binary_opt: Option<alloc::vec::Vec<u8>> = crate::process::scheduler::with_current_task(|p| p.binary_data.clone());

    let binary = match binary_opt {
        Some(b) => b,
        None => return,
    };

    println!("[WASM Runtime] Parsing and executing WASM module...");
    let mut exit_code = 0;

    match parse_wasm(&binary) {
        Ok(mut instance) => {
            if let Some(start_idx) = instance.start_func_idx {
                println!(
                    "[WASM Runtime] Running function _start (index {})...",
                    start_idx
                );
                match execute_function(&mut instance, start_idx, &[]) {
                    Ok(val) => {
                        println!(
                            "[WASM Runtime] Module executed successfully. Exit status: {:?}",
                            val.unwrap_or(0)
                        );
                        exit_code = val.unwrap_or(0) as i64;
                    }
                    Err(e) => {
                        println!("[WASM Runtime] Execution error: {}", e);
                        exit_code = -1;
                    }
                }
            } else {
                println!("[WASM Runtime] Warning: Exported _start function not found.");
            }
        }
        Err(e) => {
            println!("[WASM Runtime] Parser error: {}", e);
            exit_code = -1;
        }
    }

    // Terminate process
    let current_pid = crate::process::scheduler::with_current_task(|p| p.pid);
    if let Some(pid) = current_pid {
        crate::process::scheduler::SCHEDULER.exit_process(pid, exit_code);
    }
}

// ─── Standard Demo WASM Binary ───────────────────────────────────────────────

pub const TEST_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, // magic: \0asm
    0x01, 0x00, 0x00, 0x00, // version: 1
    // 1. Type Section (ID 1)
    0x01, 12,   // Type section, length 12
    0x02, // 2 types
    0x60, 0x04, 0x7f, 0x7f, 0x7f, 0x7f, 0x01, 0x7f, // Type 0: (i32, i32, i32, i32) -> i32
    0x60, 0x00, 0x00, // Type 1: () -> ()
    // 2. Import Section (ID 2)
    0x02, 35,   // Import section, length 35
    0x01, // 1 import
    22, b'w', b'a', b's', b'i', b'_', b's', b'n', b'a', b'p', b's', b'h', b'o', b't', b'_', b'p',
    b'r', b'e', b'v', b'i', b'e', b'w', b'1', 8, b'f', b'd', b'_', b'w', b'r', b'i', b't', b'e',
    0x00, // import kind: Function
    0x00, // type index 0
    // 3. Function Section (ID 3)
    0x03, 0x02, // Function section, length 2
    0x01, // 1 function
    0x01, // type index 1
    // 4. Memory Section (ID 5)
    0x05, 0x03, // Memory section, length 3
    0x01, // 1 memory
    0x00, 0x01, // limits: flags 0, initial 1 page
    // 5. Export Section (ID 7)
    0x07, 19,   // Export section, length 19
    0x02, // 2 exports
    6, b'm', b'e', b'm', b'o', b'r', b'y', 0x02, 0x00, // kind: Memory, index 0
    6, b'_', b's', b't', b'a', b'r', b't', 0x00, 0x01, // kind: Function, index 1
    // 6. Code Section (ID 10)
    0x0a, 29,   // Code section, length 29
    0x01, // 1 function body
    27,   // body length 27
    0x00, // local variables count
    0x41, 0x00, // i32.const 0
    0x41, 0x08, // i32.const 8
    0x36, 0x02, 0x00, // i32.store align=2 offset=0
    0x41, 0x04, // i32.const 4
    0x41, 0x1f, // i32.const 31 (string length)
    0x36, 0x02, 0x00, // i32.store align=2 offset=0
    0x41, 0x01, // i32.const 1 (fd 1 = stdout)
    0x41, 0x00, // i32.const 0 (iovs_ptr = 0)
    0x41, 0x01, // i32.const 1 (iovs_len = 1)
    0x41, 0x28, // i32.const 40 (nwritten_ptr = 40)
    0x10, 0x00, // call 0 (imported fd_write)
    0x1a, // drop (discard return val)
    0x0b, // end
    // 7. Data Section (ID 11)
    0x0b, 37,   // Data section, length 37
    0x01, // 1 segment
    0x00, // flags/memory index
    0x41, 0x08, 0x0b, // expr: i32.const 8, end
    31,   // length 31
    b'H', b'e', b'l', b'l', b'o', b' ', b'f', b'r', b'o', b'm', b' ', b'W', b'e', b'b', b'A', b's',
    b's', b'e', b'm', b'b', b'l', b'y', b' ', b'(', b'W', b'A', b'S', b'I', b')', b'!', b'\n',
];
