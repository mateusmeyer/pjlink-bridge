use pjlink_bridge::{
    PjLinkListener,
    PjLinkHandler,
    PjLinkCommand,
    PjLinkRawPayload,
    PjLinkResponse,
    PjLinkPowerCommandParameter,
};

use std::net::TcpListener;

pub fn main() {
    let handler = PjLinkMockProjector::new();
    let tcp_listener = TcpListener::bind("127.0.0.1:4352").unwrap();
    let mut listener: PjLinkListener = PjLinkListener::new(handler, tcp_listener);
    listener.listen();
}


#[derive(Clone)]
struct PjLinkMockProjectorState{
    power_on: PjLinkPowerCommandParameter
}

struct PjLinkMockProjector<'a> {
    password: Option<&'a String>,
    state: PjLinkMockProjectorState
}

impl PjLinkMockProjector<'_> {
    fn new() -> Self {
        return PjLinkMockProjector {
            password: Option::None,
            state: PjLinkMockProjectorState {
                power_on: PjLinkPowerCommandParameter::Off
            }
        }
    }
}

impl PjLinkHandler for PjLinkMockProjector<'_> {

    fn handle_command(&mut self, command: PjLinkCommand, _raw_command: PjLinkRawPayload) -> PjLinkResponse {
        return match command {
            PjLinkCommand::Power1(PjLinkPowerCommandParameter::On) => {
                println!("Power On Projector");
                self.state.power_on = PjLinkPowerCommandParameter::On;
                return PjLinkResponse::OK;
            }
            PjLinkCommand::Power1(PjLinkPowerCommandParameter::Off) => {
                println!("Power Off Projector");
                self.state.power_on = PjLinkPowerCommandParameter::Off;
                return PjLinkResponse::OK;
            }
            PjLinkCommand::Power1(PjLinkPowerCommandParameter::Query) => {
                println!("Query Power Status");
                return PjLinkResponse::Response(Box::new(self.state.power_on))
            }
            _ => PjLinkResponse::OutOfParameter
        }
    }

    fn get_password(&mut self) -> Option<&String> {
        return self.password;
    }
}