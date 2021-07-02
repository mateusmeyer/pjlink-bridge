use pjlink_bridge::{
    PjLinkListener,
    PjLinkHandler,
    PjLinkCommand,
    PjLinkRawPayload,
    PjLinkResponse,
    PjLinkPowerCommandParameter,
    PJLINK_HEADER_CHAR,
};

pub fn main() {
    let listener = PjLinkListener::new();
    listener.listen();
}


#[derive(Clone)]
struct PjLinkMockProjectorState{
    power_on: PjLinkPowerCommandParameter
}

struct PjLinkMockProjector {
    state: PjLinkMockProjectorState
}

impl PjLinkHandler for PjLinkMockProjector {
    fn new() -> Self {
        return PjLinkMockProjector {
            state: PjLinkMockProjectorState {
                power_on: PjLinkPowerCommandParameter::Off
            }
        }
    }

    fn handle_command(&mut self, command: PjLinkCommand, raw_command: PjLinkRawPayload) -> PjLinkResponse {
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
}