use pjlink_bridge::*;

use std::net::TcpListener;

pub fn main() {
    let handler = PjLinkMockProjector::new();
    let tcp_listener = TcpListener::bind("0.0.0.0:4352").unwrap();
    tcp_listener.set_nonblocking(false).unwrap();
    let mut listener: PjLinkListener = PjLinkListener::new(handler, tcp_listener);
    listener.listen();
}


#[derive(Clone)]
struct PjLinkMockProjectorState{
    power_on: u8,
    error_fan_status: u8,
    error_lamp_status: u8,
    error_temperature_status: u8,
    error_cover_open_status: u8,
    error_filter_status: u8,
    error_other_status: u8,
    lamp_hours: Vec<u8>,
    mute_status: [u8; 2],
    input_status: [u8; 2],
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
                power_on: PjLinkPowerCommandStatus::Off,
                error_fan_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_lamp_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_temperature_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_cover_open_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_filter_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_other_status: PjLinkErrorStatusCommandStatusItem::Normal,
                lamp_hours: Vec::from([b'1', b'2', b'0']),
                mute_status: [PjLinkMuteCommandStatus::AudioAndVideo, PjLinkMuteCommandStatus::NonMute],
                input_status: [PjLinkInputCommandStatus::RGB, b'1'],
            }
        }
    }
}

impl PjLinkHandler for PjLinkMockProjector<'_> {

    fn handle_command(&mut self, command: PjLinkCommand, _raw_command: &PjLinkRawPayload) -> PjLinkResponse {
        return match command {
            PjLinkCommand::Power1(PjLinkPowerCommandParameter::On) => {
                println!("Power On Projector");
                self.state.power_on = PjLinkPowerCommandStatus::On;
                return PjLinkResponse::Ok;
            }
            PjLinkCommand::Power1(PjLinkPowerCommandParameter::Off) => {
                println!("Power Off Projector");
                self.state.power_on = PjLinkPowerCommandStatus::Off;
                return PjLinkResponse::Ok;
            }
            PjLinkCommand::Power1(PjLinkPowerCommandParameter::Query) => {
                println!("Query Power Status");
                return PjLinkResponse::OkSingle(self.state.power_on)
            }
            PjLinkCommand::Class1 => {
                println!("Query Projector Class");
                return PjLinkResponse::OkSingle(PjLinkClassCommandStatus::Class1)
            }
            PjLinkCommand::ErrorStatus1 => {
                println!("Error Status Query");
                return PjLinkResponse::OkMultiple(Vec::from([
                    self.state.error_fan_status,
                    self.state.error_lamp_status,
                    self.state.error_temperature_status,
                    self.state.error_cover_open_status,
                    self.state.error_filter_status,
                    self.state.error_other_status
                ]));
            }
            PjLinkCommand::Lamp1 => {
                println!("Lamp Query");
                let mut hours = self.state.lamp_hours.clone();
                hours.push(b' ');
                hours.push(self.state.power_on);
                return PjLinkResponse::OkMultiple(hours);
            }
            PjLinkCommand::InfoManufacturer1 => {
                println!("Info Manufacturer Query");
                return PjLinkResponse::OkMultiple(Vec::from("mateusmeyer mocks"));
            }
            PjLinkCommand::InfoProductName1 => {
                println!("Info Product Name Query");
                return PjLinkResponse::OkMultiple(Vec::from("pjlink-bridge-mock"));
            }
            PjLinkCommand::Name1 => {
                println!("Name Query");
                return PjLinkResponse::OkMultiple(Vec::from("projector-001"));
            }
            PjLinkCommand::InfoOther1 => {
                println!("Info Other Query");
                return PjLinkResponse::OkMultiple(Vec::from([]));
            }
            PjLinkCommand::Input1(PjLinkInputCommandParameter::Query) | PjLinkCommand::Input2(PjLinkInputCommandParameter::Query) => {
                println!("Input1|2 Query");
                return PjLinkResponse::OkMultiple(Vec::from(self.state.input_status));
            },
            PjLinkCommand::Input1(input) | PjLinkCommand::Input2(input) => {
                println!("Input1|2 Set");

                match input {
                    PjLinkInputCommandParameter::RGB(value) => {
                        self.state.input_status = [PjLinkInputCommandStatus::RGB, value];
                    }
                    PjLinkInputCommandParameter::Video(value) => {
                        self.state.input_status = [PjLinkInputCommandStatus::Video, value];
                    }
                    PjLinkInputCommandParameter::Digital(value) => {
                        self.state.input_status = [PjLinkInputCommandStatus::Digital, value];
                    }
                    PjLinkInputCommandParameter::Storage(value) => {
                        self.state.input_status = [PjLinkInputCommandStatus::Storage, value];
                    }
                    PjLinkInputCommandParameter::Network(value) => {
                        self.state.input_status = [PjLinkInputCommandStatus::Network, value];
                    }
                    PjLinkInputCommandParameter::Internal(value) => {
                        self.state.input_status = [PjLinkInputCommandStatus::Internal, value];
                    }
                    _ => return PjLinkResponse::OutOfParameter
                };

                return PjLinkResponse::Ok;
            },
            PjLinkCommand::AvMute1(PjLinkMuteCommandParameter::Query) => {
                println!("AV Mute Query");
                return PjLinkResponse::OkMultiple(Vec::from(self.state.mute_status))
            }
            PjLinkCommand::AvMute1(parameter) => {
                println!("AV Mute Set");
                let is_muted = self.state.mute_status[1] == PjLinkMuteCommandStatus::Mute;
                let current_muted_item = self.state.mute_status[0];

                match parameter {
                    PjLinkMuteCommandParameter::Audio(mute) => {
                        self.state.mute_status = if current_muted_item == PjLinkMuteCommandStatus::Video && is_muted && mute {
                            [PjLinkMuteCommandStatus::AudioAndVideo, PjLinkMuteCommandStatus::Mute]
                        } else {
                            if current_muted_item == PjLinkMuteCommandStatus::AudioAndVideo && is_muted && !mute {
                                [PjLinkMuteCommandStatus::Video, PjLinkMuteCommandStatus::Mute]
                            } else {
                                [current_muted_item, if mute {PjLinkMuteCommandStatus::Mute} else {PjLinkMuteCommandStatus::NonMute}]
                            }
                        }
                    }
                    PjLinkMuteCommandParameter::Video(mute) => {
                        self.state.mute_status = if current_muted_item == PjLinkMuteCommandStatus::Audio && is_muted && mute {
                            [PjLinkMuteCommandStatus::AudioAndVideo, PjLinkMuteCommandStatus::Mute]
                        } else {
                            if current_muted_item == PjLinkMuteCommandStatus::AudioAndVideo && is_muted && !mute {
                                [PjLinkMuteCommandStatus::Audio, PjLinkMuteCommandStatus::Mute]
                            } else {
                                [current_muted_item, if mute {PjLinkMuteCommandStatus::Mute} else {PjLinkMuteCommandStatus::NonMute}]
                            }
                        }
                    }
                    PjLinkMuteCommandParameter::AudioAndVideo(mute) => {
                        self.state.mute_status = [
                            PjLinkMuteCommandStatus::AudioAndVideo,
                            if mute {PjLinkMuteCommandStatus::Mute} else {PjLinkMuteCommandStatus::NonMute}
                        ];
                    },
                    _ => {
                        return PjLinkResponse::OutOfParameter;
                    }
                }

                return PjLinkResponse::Ok;
            }
            _ => PjLinkResponse::OutOfParameter
        }
    }

    fn get_password(&mut self) -> Option<&String> {
        return self.password;
    }
}