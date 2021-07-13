use pjlink_bridge::*;

use std::sync::{Arc, Mutex};
use clap::{AppSettings, Clap};
use log::{LevelFilter};
use simple_logger::{SimpleLogger};

#[derive(Clap)]
#[clap(version = "0.1.0", author = "Mateus Meyer Jiacomelli")]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    #[clap(short, long, default_value = "0.0.0.0")]
    listen_address: String,
    #[clap(short, long, default_value = "4352")]
    port: String,
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
    #[clap(long)]
    no_log: bool,
    #[clap(short, long)]
    udp: bool,
    #[clap(long, default_value = "0.0.0.0")]
    udp_listen_address: String,
    #[clap(long, default_value = "2")]
    class_type: String,
    #[clap(long, default_value = "mateusmeyer mocks")]
    manufacturer_name: String,
    #[clap(long, default_value = "projector-mock")]
    product_name: String,
    #[clap(long, default_value = "projector-001")]
    projector_name: String,
    #[clap(long, default_value = "faa13ebee21677a2c064fd6ce067b50e")]
    serial_number: String,
    #[clap(long, default_value = "1.0")]
    software_version: String,
    #[clap(long, default_value = "1920x1080")]
    screen_resolution: String,
    #[clap(long, default_value = "1920x1080")]
    recommended_screen_resolution: String,
    #[clap(long)]
    password: Option<String>,
}

pub fn main() {
    let opts = Opts::parse();

    if !opts.no_log {
        SimpleLogger::new()
            .with_level(match opts.verbose {
                1 => LevelFilter::Error,
                2 => LevelFilter::Warn,
                3 => LevelFilter::Debug,
                4 => LevelFilter::Trace,
                _ => LevelFilter::Warn
            })
            .init()
            .unwrap();
    }

    let tcp_bind_address = opts.listen_address;
    let password = opts.password;

    let handler = PjLinkMockProjector::new(PjLinkMockProjectorOptions {
        password,
        class_type: opts.class_type.as_bytes()[0],
        manufacturer_name: Vec::from(opts.manufacturer_name.as_bytes()),
        product_name: Vec::from(opts.product_name.as_bytes()),
        projector_name: Vec::from(opts.projector_name.as_bytes()),
        serial_number: Vec::from(opts.serial_number.as_bytes()),
        software_version: Vec::from(opts.software_version.as_bytes()),
        screen_resolution: Vec::from(opts.screen_resolution.as_bytes()),
        recommended_screen_resolution: Vec::from(opts.recommended_screen_resolution.as_bytes()),
    });

    let shared_handler = Arc::new(Mutex::new(handler));

    if opts.udp {
        let udp_bind_address = opts.udp_listen_address;
        PjLinkServer::listen_tcp_udp(shared_handler, tcp_bind_address, udp_bind_address, opts.port);
    } else {
        PjLinkServer::listen_tcp_only(shared_handler, tcp_bind_address, opts.port);
    }

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
    filter_hours: Vec<u8>,
    mute_status: [u8; 2],
    input_status: [u8; 2],
    available_inputs: Vec<u8>,
    freeze_status: u8,
}

struct PjLinkMockProjectorOptions {
    password: Option<String>,
    class_type: u8,
    manufacturer_name: Vec<u8>,
    product_name: Vec<u8>,
    projector_name: Vec<u8>,
    serial_number: Vec<u8>,
    software_version: Vec<u8>,
    screen_resolution: Vec<u8>,
    recommended_screen_resolution: Vec<u8>,
}

struct PjLinkMockProjector {
    options: PjLinkMockProjectorOptions,
    state: PjLinkMockProjectorState
}

impl PjLinkMockProjector {
    fn new(options: PjLinkMockProjectorOptions) -> Self {
        return PjLinkMockProjector {
            options,
            state: PjLinkMockProjectorState {
                power_on: PjLinkPowerCommandStatus::Off,
                error_fan_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_lamp_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_temperature_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_cover_open_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_filter_status: PjLinkErrorStatusCommandStatusItem::Normal,
                error_other_status: PjLinkErrorStatusCommandStatusItem::Normal,
                lamp_hours: vec![b'1', b'2', b'0'],
                filter_hours: vec![b'0'],
                mute_status: [PjLinkMuteCommandStatus::AudioAndVideo, PjLinkMuteCommandStatus::NonMute],
                input_status: [PjLinkInputCommandStatus::RGB, b'1'],
                available_inputs: vec![
                    PjLinkInputCommandStatus::RGB, b'1', b' ',
                    PjLinkInputCommandStatus::RGB, b'2', b' ',
                    PjLinkInputCommandStatus::Digital, b'1', b' ',
                    PjLinkInputCommandStatus::Storage, b'1',
                ],
                freeze_status: b'0'
            }
        }
    }
}

impl PjLinkHandler for PjLinkMockProjector{

    fn handle_command(&mut self, command: PjLinkCommand, _raw_command: &PjLinkRawPayload) -> PjLinkResponse {
        return match command {
            // #region Power Control Instruction / POWR
            PjLinkCommand::Power1(PjLinkPowerCommandParameter::Query) => {
                println!("Query Power Status");
                return PjLinkResponse::OkSingle(self.state.power_on)
            }
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
            // #endregion
            // #region Input Switch Instruction / INPT
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
            // #endregion
            // #region Mute Instruction / AVMT
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
            // #endregion  
            // #region Error Status Query / ERST
            PjLinkCommand::ErrorStatus1 => {
                println!("Error Status Query");
                return PjLinkResponse::OkMultiple(vec![
                    self.state.error_fan_status,
                    self.state.error_lamp_status,
                    self.state.error_temperature_status,
                    self.state.error_cover_open_status,
                    self.state.error_filter_status,
                    self.state.error_other_status
                ]);
            }
            // #endregion
            // #region Lamp Number/Lighting Hour Query / LAMP
            PjLinkCommand::Lamp1 => {
                println!("Lamp Query");
                let mut hours = self.state.lamp_hours.clone();
                hours.push(b' ');
                hours.push(self.state.power_on);
                return PjLinkResponse::OkMultiple(hours);
            }
            // #endregion
            // #region Input Toggling List Query / INST
            PjLinkCommand::InputTogglingList1 | PjLinkCommand::InputTogglingList2 => {
                println!("Input Toggling List Query");
                return PjLinkResponse::OkMultiple(self.state.available_inputs.clone())
            }
            // #endregion
            // #region Projector/Display Name Query / NAME
            PjLinkCommand::Name1 => {
                println!("Name Query");
                return PjLinkResponse::OkMultiple(self.options.projector_name.clone());
            }
            // #endregion
            // #region Manufacture Name Information Query / INF1
            PjLinkCommand::InfoManufacturer1 => {
                println!("Info Manufacturer Query");
                return PjLinkResponse::OkMultiple(self.options.manufacturer_name.clone());
            }
            // #endregion
            // #region Product Name Information Query / INF2
            PjLinkCommand::InfoProductName1 => {
                println!("Info Product Name Query");
                return PjLinkResponse::OkMultiple(self.options.product_name.clone());
            }
            // #endregion
            // #region Other Information Query / INFO
            PjLinkCommand::InfoOther1 => {
                println!("Info Other Query");
                return PjLinkResponse::OkMultiple(vec![]);
            }
            // #endregion
            // #region Class Information Query / CLSS
            PjLinkCommand::Class1 => {
                println!("Class Information Query");
                return PjLinkResponse::OkSingle(self.options.class_type)
            }
            // #endregion
            // #region Serial Number Query / SNUM
            PjLinkCommand::SerialNumber2 => {
                println!("Serial Number Query");
                return PjLinkResponse::OkMultiple(self.options.serial_number.clone());
            }
            // #endregion
            // #region Software Version Query / SVER
            PjLinkCommand::SoftwareVersion2 => {
                println!("Software Version Query");
                return PjLinkResponse::OkMultiple(self.options.software_version.clone());
            }
            // #endregion
            // #region Input Terminal Name Query / INNM
            PjLinkCommand::InputTerminalName2(input_type) => {
                println!("Input Terminal Name Query");
                match input_type {
                    PjLinkInputCommandParameter::RGB(input) => PjLinkResponse::OkMultiple(Vec::from(format!("VGA{}", input))),
                    PjLinkInputCommandParameter::Video(input) => PjLinkResponse::OkMultiple(Vec::from(format!("Analog{}", input))),
                    PjLinkInputCommandParameter::Digital(input) => PjLinkResponse::OkMultiple(Vec::from(format!("HDMI{}", input))),
                    PjLinkInputCommandParameter::Network(input) => PjLinkResponse::OkMultiple(Vec::from(format!("Network{}", input))),
                    PjLinkInputCommandParameter::Storage(input) => PjLinkResponse::OkMultiple(Vec::from(format!("Storage{}", input))),
                    PjLinkInputCommandParameter::Internal(input) => PjLinkResponse::OkMultiple(Vec::from(format!("Internal{}", input))),
                    _ => PjLinkResponse::OutOfParameter
                }
            }
            // #endregion
            // #region Input Resolution Query / IRES
            PjLinkCommand::InputResolution2 => {
                println!("Input Resolution Query");
                return PjLinkResponse::OkMultiple(self.options.screen_resolution.clone());
            }
            // #endregion
            // #region Recommend Resolution Query / RRES
            PjLinkCommand::RecommendResolution2 => {
                println!("Recommend Resolution Query");
                return PjLinkResponse::OkMultiple(self.options.recommended_screen_resolution.clone());
            }
            // #endregion
            // #region Filter Usage Time Query / FILT
            PjLinkCommand::FilterUsageTime2 => {
                println!("Filter Usage Time Query");
                return PjLinkResponse::OkMultiple(self.state.filter_hours.clone());
            }
            // #endregion
            // #region Lamp Replacement Model Number Query / RLMP
            PjLinkCommand::LampReplacementModelNumber2 => {
                println!("Lamp Replacement Model Number Query");
                return PjLinkResponse::OkEmpty
            }
            // #endregion
            // #region Filter Replacement Model Number Query / RFIL
            PjLinkCommand::FilterReplacementModelNumber2 => {
                println!("Filter Replacement Model Number Query");
                return PjLinkResponse::OkEmpty;
            }
            // #endregion
            // #region Speaker Volume Adjustment Instruction / SVOL
            PjLinkCommand::SpeakerVolumeAdjustment2(_) => {
                println!("Speaker Volume Adjustment Set");
                PjLinkResponse::Ok
            },
            // #endregion
            // #region Microphone Volume Adjustment Instruction / MVOL
            PjLinkCommand::MicrophoneVolumeAdjustment2(_) => {
                println!("Microphone Volume Adjustment Set");
                return PjLinkResponse::Undefined;
            }
            // #endregion
            // #region Freeze Instruction / FREZ
            PjLinkCommand::Freeze2(PjLinkFreezeCommandParameter::Query) => {
                println!("Freeze Instruction Query");
                return PjLinkResponse::OkSingle(self.state.freeze_status)
            }
            PjLinkCommand::Freeze2(instruction) => {
                println!("Freeze Instruction Set");
                self.state.freeze_status = match instruction {
                    PjLinkFreezeCommandParameter::Freeze => b'1',
                    PjLinkFreezeCommandParameter::Unfreeze => b'0',
                    _ => return PjLinkResponse::OutOfParameter
                };
                return PjLinkResponse::Ok;
            }
            // #endregion
            _ => PjLinkResponse::OutOfParameter
        }
    }

    fn get_password(&mut self) -> Option<String> {
        return self.options.password.clone();
    }
}