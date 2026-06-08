import sys

with open("src/abi/syscall.rs", "r") as f:
    content = f.read()

# Add constants
content = content.replace("pub const ZIQA_CAP_CLOSE: u64 = 1003;", """pub const ZIQA_CAP_REQUEST: u64 = 1000;
    pub const ZIQA_CAP_READ: u64 = 1001;
    pub const ZIQA_CAP_WRITE: u64 = 1002;
    pub const ZIQA_CAP_CLOSE: u64 = 1003;""")

# Add match cases
content = content.replace("nr::ZIQA_CAP_CLOSE => return ziqa_cap_close(ctx),", """nr::ZIQA_CAP_REQUEST => return ziqa_cap_request(ctx),
        nr::ZIQA_CAP_READ    => return ziqa_cap_read(ctx),
        nr::ZIQA_CAP_WRITE   => return ziqa_cap_write(ctx),
        nr::ZIQA_CAP_CLOSE => return ziqa_cap_close(ctx),""")

# Replace ziqa_cap_close
old_close = """fn ziqa_cap_close(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    let fd = ctx.args[0] as usize;
    // Refuse to close stdin/stdout/stderr via this path — use dup2 to redirect.
    if fd < 3 {
        klog_syscall("ziqa_cap_close", fd as u64);
        return Ok((-1_i64) as u64); // -EPERM
    }
    let ok = ctx.process.fds.close(fd);
    klog_syscall("ziqa_cap_close", fd as u64);
    if ok {
        Ok(0)
    } else {
        Ok((-9_i64) as u64) // -EBADF
    }
}"""
new_close = """fn ziqa_cap_close(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::capability::{CapabilityId, CapabilitySpace};
    let cap_id = CapabilityId(ctx.args[0]);

    let target = {
        match ctx.process.capabilities.lookup(cap_id) {
            Some(c) => c.target,
            None => return Ok((-9_i64) as u64), // -EBADF
        }
    };

    let fd = target as usize;
    if fd >= 3 {
        ctx.process.fds.close(fd);
    }
    CapabilitySpace::revoke_global(cap_id);
    klog_syscall("ziqa_cap_close", cap_id.0);
    Ok(0)
}"""
content = content.replace(old_close, new_close)

# Replace ziqa_cap_seek
old_seek = """fn ziqa_cap_seek(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::process::FdTarget;
    let fd      = ctx.args[0] as usize;
    let offset  = ctx.args[1] as i64;
    let whence  = ctx.args[2] as i32;"""
new_seek = """fn ziqa_cap_seek(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::capability::CapabilityId;
    use crate::process::FdTarget;
    let cap_id = CapabilityId(ctx.args[0]);
    let offset = ctx.args[1] as i64;
    let whence = ctx.args[2] as i32;

    let target = {
        match ctx.process.capabilities.lookup(cap_id) {
            Some(c) => c.target,
            None => return Ok((-9_i64) as u64), // -EBADF
        }
    };
    let fd = target as usize;"""
content = content.replace(old_seek, new_seek)

# Append new functions
functions = """
/// ZIQA_CAP_REQUEST (1000)
fn ziqa_cap_request(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::capability::{ResourceKind, Permissions};
    let kind = ctx.args[0];
    if kind != 1 { return Ok((-22_i64) as u64); }
    let path_addr = ctx.args[1] as *const u8;
    let path_len = ctx.args[2] as usize;
    let flags = ctx.args[3] as u32;
    if path_len > 1024 { return Ok((-22_i64) as u64); }
    let mut tmp = alloc::vec![0u8; path_len];
    unsafe { core::ptr::copy_nonoverlapping(path_addr, tmp.as_mut_ptr(), path_len); }
    let path_str = core::str::from_utf8(&tmp).unwrap_or("");
    let o_creat = (flags & 0x40) != 0;
    if o_creat {
        let mut vfs = crate::fs::vfs::VFS.write();
        if !vfs.exists(path_str) { vfs.create(path_str); }
    }
    let is_known = matches!(path_str, "/dev/null" | "/dev/zero" | "/dev/random" | "/dev/urandom" | "/dev/tty" | "/dev/console") || path_str.starts_with("/proc/") || path_str.starts_with("/etc/");
    if is_known || o_creat || crate::fs::vfs::VFS.read().exists(path_str) {
        if let Some(fd) = ctx.process.fds.alloc_file(path_str.as_bytes(), flags) {
            let perms = Permissions::full();
            if let Some(id) = ctx.process.capabilities.grant(ResourceKind::File, perms, fd as u64, None) {
                klog_syscall("ziqa_cap_request (file)", id.0);
                return Ok(id.0);
            }
            ctx.process.fds.close(fd);
            return Ok((-24_i64) as u64);
        }
        return Ok((-24_i64) as u64);
    }
    Ok((-2_i64) as u64)
}

/// ZIQA_CAP_READ (1001)
fn ziqa_cap_read(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::capability::CapabilityId;
    use crate::process::FdTarget;
    let cap_id = CapabilityId(ctx.args[0]);
    let buf_addr = ctx.args[1] as *mut u8;
    let count = ctx.args[2] as usize;
    let offset = ctx.args[3] as usize;
    let fd = match ctx.process.capabilities.lookup(cap_id) {
        Some(c) => c.target as usize,
        None => return Ok((-9_i64) as u64),
    };
    let path = match ctx.process.fds.get(fd) {
        Some(desc) => match &desc.target {
            FdTarget::File(p) => ctx.process.fds.get_path(*p),
            _ => return Ok((-29_i64) as u64),
        },
        None => return Ok((-9_i64) as u64),
    };
    if path == "/dev/tty" || path == "/dev/console" || fd == 0 {
        let mut tmp = alloc::vec![0u8; count.min(256)];
        let n = crate::drivers::keyboard::read_stdin(&mut tmp);
        if n > 0 { unsafe { core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_addr, n); } }
        return Ok(n as u64);
    }
    let mut buf = alloc::vec![0u8; count];
    let vfs = crate::fs::vfs::VFS.read();
    match vfs.read_raw(path, &mut buf, offset) {
        Ok(bytes_read) => {
            unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), buf_addr, bytes_read); }
            Ok(bytes_read as u64)
        }
        Err(_) => Ok((-5_i64) as u64),
    }
}

/// ZIQA_CAP_WRITE (1002)
fn ziqa_cap_write(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::capability::CapabilityId;
    use crate::process::FdTarget;
    let cap_id = CapabilityId(ctx.args[0]);
    let buf_addr = ctx.args[1] as *const u8;
    let count = ctx.args[2] as usize;
    let offset = ctx.args[3] as usize;
    let fd = match ctx.process.capabilities.lookup(cap_id) {
        Some(c) => c.target as usize,
        None => return Ok((-9_i64) as u64),
    };
    let path = match ctx.process.fds.get(fd) {
        Some(desc) => match &desc.target {
            FdTarget::File(p) => ctx.process.fds.get_path(*p),
            _ => return Ok((-29_i64) as u64),
        },
        None => return Ok((-9_i64) as u64),
    };
    if path == "/dev/tty" || path == "/dev/console" || path == "/dev/stdout" || path == "/dev/stderr" {
        let mut tmp = alloc::vec![0u8; count];
        unsafe { core::ptr::copy_nonoverlapping(buf_addr, tmp.as_mut_ptr(), count); }
        if let Ok(s) = core::str::from_utf8(&tmp) { crate::print!("{}", s); }
        return Ok(count as u64);
    }
    if path == "/dev/null" || path == "/dev/zero" { return Ok(count as u64); }
    let mut buf = alloc::vec![0u8; count];
    unsafe { core::ptr::copy_nonoverlapping(buf_addr, buf.as_mut_ptr(), count); }
    let mut vfs = crate::fs::vfs::VFS.write();
    match vfs.write_raw(path, &buf, offset) {
        Ok(bytes_written) => Ok(bytes_written as u64),
        Err(_) => Ok((-5_i64) as u64),
    }
}
"""
content += functions

with open("src/abi/syscall.rs", "w") as f:
    f.write(content)
