pub mod protocol;
pub mod server;
pub mod serialization;

use crate::ipc::driver::protocol::{DriverRequest, DriverResponse};
use crate::ipc::{get_channel, Message};
use crate::process::Pid;
use crate::ipc::driver::serialization::Serializable;

pub struct DriverClient {
    pub channel_id: usize,
}

impl DriverClient {
    pub fn send_request(&self, req: DriverRequest) -> DriverResponse {
        let channel = get_channel(self.channel_id).expect("IPC channel not found");
        
        let data = req.serialize();
        let msg = Message::new(Pid(0), &data); 
        channel.send(msg).expect("IPC send failed");
        
        // Mock response wait for now, in real impl we wait on a response channel
        DriverResponse::Status(0)
    }
}
