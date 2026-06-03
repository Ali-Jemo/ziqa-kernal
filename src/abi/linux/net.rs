use super::{nr, SyscallContext, AbiError};
#[cfg(feature = "net")]
use crate::net::socket::{SOCKETS, SocketState, SockAddrInet, SockDomain, SockType};

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
    let backlog = ctx.args[1] as i32;
    let mut socks = SOCKETS.lock();

    let domain = socks.get(fd).map(|e| e.domain);
    match domain {
        Some(SockDomain::Inet) => {
            if let Err(e) = socks.tcp_listen(fd, backlog) {
                crate::println!("[net] sys_listen failed: {}", e);
                return Ok((-22_i64) as u64); // -EINVAL
            }
            Ok(0)
        }
        Some(_) => {
            let entry = socks.get_mut(fd).unwrap();
            entry.state = SocketState::Listening;
            Ok(0)
        }
        None => Ok((-9_i64) as u64), // -EBADF
    }
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
    let (domain, socktype) = match socks.get(fd) {
        Some(e) if e.state == SocketState::Closed => return Ok((-9_i64) as u64),
        Some(e) => (e.domain, e.socktype),
        None => return Ok((-9_i64) as u64),
    };

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
        socks.get_mut(fd).unwrap().state = SocketState::Connected;
        return Ok(0);
    }

    if domain == SockDomain::Inet && socktype == SockType::Stream {
        let addr = raw.addr;
        let port = raw.port;
        // Drop lock before blocking connect
        drop(socks);
        match SOCKETS.lock().tcp_connect(fd, addr, port) {
            Ok(_) => Ok(0),
            Err(e) => {
                crate::println!("[net] sys_connect failed: {}", e);
                Ok((-111_i64) as u64) // -ECONNREFUSED
            }
        }
    } else {
        socks.get_mut(fd).unwrap().state = SocketState::Connected;
        Ok(0)
    }
}

/// sys_accept(sockfd, addr, addrlen) → new_fd / -EAGAIN
fn sys_accept(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let addr_ptr = ctx.args[1] as *mut u8;
    let addrlen_ptr = ctx.args[2] as *mut u32;

    let mut socks = SOCKETS.lock();
    let entry = match socks.get_mut(fd) {
        Some(e) => e,
        None => return Ok((-9_i64) as u64), // -EBADF
    };
    if entry.state != SocketState::Listening {
        return Ok((-22_i64) as u64); // -EINVAL
    }

    // 1. Check for local pending connections (loopback/unix)
    if let Some(client_fd) = entry.pending.pop() {
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
        let acc_entry = socks.get_mut(accept_fd).unwrap();
        acc_entry.state = SocketState::Connected;
        acc_entry.paired = Some(client_fd);
        acc_entry.local_addr = server_local;
        acc_entry.remote_addr = client_local;

        socks.get_mut(client_fd).unwrap().paired = Some(accept_fd);

        return Ok(accept_fd as u64);
    }

    // 2. Check for smoltcp connections (real network)
    if entry.domain == SockDomain::Inet && entry.socktype == SockType::Stream {
        // Drop lock before FD allocation to avoid potential deadlocks
        drop(socks);
        let accept_fd = match ctx.process.fds.alloc_file(b"socket:", 1) {
            Some(f) => f,
            None => return Ok((-24_i64) as u64), // -EMFILE
        };

        let mut socks = SOCKETS.lock();
        socks.create(accept_fd, 2, 1, 0); // AF_INET, SOCK_STREAM

        match socks.tcp_accept(fd, accept_fd) {
            Ok((addr, port)) => {
                if !addr_ptr.is_null() {
                    let sa = SockAddrInet {
                        family: 2,
                        port,
                        addr,
                        zero: [0; 8],
                    };
                    unsafe { core::ptr::write_unaligned(addr_ptr as *mut SockAddrInet, sa) };
                }
                if !addrlen_ptr.is_null() {
                    unsafe { *addrlen_ptr = core::mem::size_of::<SockAddrInet>() as u32 };
                }
                Ok(accept_fd as u64)
            }
            Err(e) => {
                // If it's just EAGAIN, clean up the pre-allocated FD and return error
                socks.remove(accept_fd);
                if e == "EAGAIN" {
                    Ok((-11_i64) as u64) // -EAGAIN
                } else {
                    Ok((-22_i64) as u64) // -EINVAL
                }
            }
        }
    } else {
        Ok((-11_i64) as u64) // -EAGAIN
    }
}

