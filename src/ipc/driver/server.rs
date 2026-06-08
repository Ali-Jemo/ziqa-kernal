use crate::ipc::driver::protocol::{DriverRequest, DriverResponse};

pub trait DriverServer {
    fn handle_request(&mut self, req: DriverRequest) -> DriverResponse;
}
