use crate::ipc::driver::protocol::{DriverRequest, DriverResponse};
use crate::ipc::driver::server::DriverServer;
use crate::drivers::ps2_mouse;
use crate::ipc::driver::serialization::Serializable;

pub struct Ps2MouseServer;

impl DriverServer for Ps2MouseServer {
    fn handle_request(&mut self, req: DriverRequest) -> DriverResponse {
        match req {
            DriverRequest::GetMousePos => {
                let (x, y) = ps2_mouse::get_mouse_pos();
                DriverResponse::MousePos(x, y)
            }
            DriverRequest::GetMouseBtn => {
                let btn = ps2_mouse::get_mouse_btn();
                DriverResponse::MouseBtn(btn)
            }
            _ => DriverResponse::Status(-1), // Unhandled
        }
    }
}

pub fn run_mouse_server(channel_id: usize) {
    let channel = crate::ipc::get_channel(channel_id).expect("Mouse channel not registered");
    let mut server = Ps2MouseServer;

    crate::println!(" ~ PS/2 Mouse IPC Server starting on channel {}", channel_id);

    loop {
        if let Ok(msg) = channel.recv() {
            let req = Serializable::deserialize(&msg.data[..msg.len]);
            let res = server.handle_request(req);
            // In a full implementation, res would be serialized and sent to a response channel
        }
        crate::process::scheduler::yield_now();
    }
}
