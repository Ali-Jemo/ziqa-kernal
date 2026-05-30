use super::{nr, SyscallContext, AbiError};
#[cfg(feature = "net")]
use crate::net::socket::{SOCKETS, SocketState, SockAddrInet};

pub(super) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_SOCKET => super::sys_socket(ctx),
        nr::SYS_BIND => sys_bind(ctx),
        nr::SYS_LISTEN => sys_listen(ctx),
        nr::SYS_CONNECT => sys_connect(ctx),
        nr::SYS_ACCEPT => sys_accept(ctx),
        nr::SYS_SENDTO => super::sys_sendto(ctx),
        nr::SYS_RECVFROM => super::sys_recvfrom(ctx),
        nr::SYS_SETSOCKOPT | nr::SYS_GETSOCKOPT => Ok(0),
        _ => return None,
    })
}

/// sys_bind(sockfd, addr, addrlen) → 0 / -EINVAL
fn sys_bind(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let addr_ptr = ctx.args[1] as *const u8;
    let _addrlen = ctx.args[2] as usize;

    let mut socks = SOCKETS.lock();
    let entry = match socks.get_mut(fd) {
        Some(e) => e,
        None => return Ok((-9_i64) as u64), // -EBADF
    };
    if entry.state != SocketState::Created {
        return Ok((-22_i64) as u64); // -EINVAL (already bound)
    }
    if !addr_ptr.is_null() {
        let raw = unsafe { core::ptr::read_unaligned(addr_ptr as *const SockAddrInet) };
        entry.local_addr = Some(raw);
    }
    entry.state = SocketState::Bound;
    Ok(0)
}

/// sys_listen(sockfd, backlog) → 0
fn sys_listen(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let mut socks = SOCKETS.lock();
    let entry = match socks.get_mut(fd) {
        Some(e) => e,
        None => return Ok((-9_i64) as u64), // -EBADF
    };
    entry.state = SocketState::Listening;
    Ok(0)
}

/// sys_connect(sockfd, addr, addrlen) → 0 / -ECONNREFUSED / -EINPROGRESS
fn sys_connect(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let addr_ptr = ctx.args[1] as *const u8;
    let _addrlen = ctx.args[2] as usize;

    if addr_ptr.is_null() {
        return Ok((-14_i64) as u64); // -EFAULT
    }
    let raw: SockAddrInet = unsafe { core::ptr::read_unaligned(addr_ptr as *const SockAddrInet) };

    let mut socks = SOCKETS.lock();

    // Validate socket exists and is not closed
    match socks.get(fd) {
        Some(e) if e.state == SocketState::Closed => return Ok((-9_i64) as u64),
        None => return Ok((-9_i64) as u64),
        _ => {}
    }

    // Store remote address
    socks.get_mut(fd).unwrap().remote_addr = Some(raw);

    let is_local = raw.family == 1
        || (raw.family == 2 && raw.addr == [127, 0, 0, 1])
        || raw.addr == [0u8; 4];

    if is_local {
        let listener = match socks.find_listener(raw.port, raw.addr) {
            Some(l) => l,
            None => match socks.find_any_listener() {
                Some(l) => l,
                None => return Ok((-111_i64) as u64),
            },
        };
        socks.get_mut(listener).unwrap().pending.push(fd);
    }

    socks.get_mut(fd).unwrap().state = SocketState::Connected;
    Ok(0)
}

/// sys_accept(sockfd, addr, addrlen) → new_fd / -EAGAIN
fn sys_accept(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let _addr_ptr = ctx.args[1] as *mut u8;
    let _addrlen_ptr = ctx.args[2] as *mut u32;

    let mut socks = SOCKETS.lock();
    let entry = match socks.get_mut(fd) {
        Some(e) => e,
        None => return Ok((-9_i64) as u64), // -EBADF
    };
    if entry.state != SocketState::Listening {
        return Ok((-22_i64) as u64); // -EINVAL
    }

    // Check for pending connections
    let client_fd = match entry.pending.pop() {
        Some(f) => f,
        None => return Ok((-11_i64) as u64), // -EAGAIN
    };

    // Allocate a new fd in the calling process for the accepted socket
    let accept_fd = match ctx.process.fds.alloc_file(b"socket:", 1) {
        Some(f) => f,
        None => return Ok((-24_i64) as u64), // -EMFILE
    };

    // Register the new socket
    socks.create(accept_fd, 2, 1, 0);

    // Copy address info (avoid borrow conflicts with separate extraction)
    let server_local = socks.get(fd).and_then(|s| s.local_addr);
    let client_local = socks.get(client_fd).and_then(|s| s.local_addr);

    // Set up the bidirectional pair
    socks.get_mut(accept_fd).unwrap().state = SocketState::Connected;
    socks.get_mut(accept_fd).unwrap().paired = Some(client_fd);
    socks.get_mut(accept_fd).unwrap().local_addr = server_local;
    socks.get_mut(accept_fd).unwrap().remote_addr = client_local;
    socks.get_mut(client_fd).unwrap().paired = Some(accept_fd);

    Ok(accept_fd as u64)
}
