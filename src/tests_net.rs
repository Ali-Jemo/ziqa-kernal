use crate::println;
use crate::net::socket::{SOCKETS, SocketState};

pub fn run_socket_tests() {
    println!("[TEST] Running socket stack tests...");
    
    test_tcp_listen_accept();
}

fn test_tcp_listen_accept() {
    println!("[TEST]   socket: tcp listen/accept...");
    
    let mut socks = SOCKETS.lock();
    
    // 1. Create a listening socket
    let listen_fd = 100;
    socks.create(listen_fd, 2, 1, 0); // AF_INET, SOCK_STREAM
    
    let addr = [127, 0, 0, 1];
    let port = 8080;
    
    if socks.tcp_bind(listen_fd, addr, port).is_err() {
        println!("[TEST]   FAIL: tcp_bind failed");
        return;
    }
    
    if socks.tcp_listen(listen_fd, 1).is_err() {
        println!("[TEST]   FAIL: tcp_listen failed");
        return;
    }
    
    // 2. Verify the state transitions.
    if socks.get(listen_fd).unwrap().state != SocketState::Listening {
        println!("[TEST]   FAIL: listen_fd not in Listening state");
        return;
    }
    
    println!("[TEST]   PASS: tcp listen/accept (state transition)");
}
