extern crate rand;

use std::thread;
use std::sync::{RwLock, Arc};
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use rand::prelude::*;

pub fn teste() {
    println!("Teste")
}

pub const PJLINK_HEADER: u8 = b'%';
pub const PJLINK_COMMAND_SEPARATOR: u8 = 0x20; // space
pub const PJLINK_RESPONSE_SEPARATOR: u8 = 0x3d; // =
pub const PJLINK_TERMINATOR: u8 = b'\x0d'; // carriage return
pub const PJLINK_QUERY_CHAR: char = '?';
pub const PJLINK_QUERY: u8 = PJLINK_QUERY_CHAR as u8;

pub const PJLINK_NULLIFIED_SECURITY: &[u8; 9] = b"PJLINK 0\x0d";
pub const PJLINK_SECURITY: &[u8; 9] = b"PJLINK 1 ";

pub struct PjLinkRawPayload {
    header_and_class: [u8; 2],
    command_body: [u8; 4],
    separator: u8,
    transmission_parameter: Vec<u8>,
    terminator: u8
}

pub struct PjLinkRawNoBodyPayload {
    header_and_class: [u8; 2],
    command_body: [u8; 4],
    terminator: u8
}

pub enum PjLinkResponse {
    Ok,
    Undefined,
    OutOfParameter,
    UnavailableTime,
    ProjectorOrDisplayFailure,
    OkSingle(u8),
    OkMultiple(Vec<u8>)
}

pub enum PjLinkPowerCommandParameter {
    Off,
    On,
    Query,
    Unknown,
}

pub struct PjLinkPowerCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkPowerCommandStatus {
    pub const Off: u8 = b'0';
    pub const On: u8 = b'1';
    pub const Cooling: u8 = b'2';
    pub const WarmUp: u8 = b'3';
}

pub struct PjLinkClassCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkClassCommandStatus {
    pub const Class1: u8 = b'1';
    pub const Class2: u8 = b'2';
}

pub struct PjLinkErrorStatusCommandStatusItem;
#[allow(non_upper_case_globals)]
impl PjLinkErrorStatusCommandStatusItem {
    pub const Normal: u8 = b'0';
    pub const Warning: u8 = b'1';
    pub const Error: u8 = b'2';
}

pub struct PjLinkInputCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkInputCommandStatus {
    pub const RGB: u8 = b'1';
    pub const Video: u8 = b'2';
    pub const Digital: u8 = b'3';
    pub const Storage: u8 = b'4';
    pub const Network: u8 = b'5';
    pub const Internal: u8 = b'6';
}
pub enum PjLinkInputCommandParameter {
    RGB(u8),
    Video(u8),
    Digital(u8),
    Storage(u8),
    Network(u8),
    Internal(u8),
    Query,
    Unknown,
}

pub struct PjLinkMuteCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkMuteCommandStatus {
    pub const Audio: u8 = b'2';
    pub const Video: u8 = b'1';
    pub const AudioAndVideo: u8 = b'3';
    pub const Mute: u8 = b'1';
    pub const NonMute: u8 = b'0';
}
pub enum PjLinkMuteCommandParameter {
    Audio(bool),
    Video(bool),
    AudioAndVideo(bool),
    Query,
    Unknown,
}

pub enum PjLinkCommand {
    Search2,
    Power1(PjLinkPowerCommandParameter),
    Input1(PjLinkInputCommandParameter),
    Input2(PjLinkInputCommandParameter),
    AvMute1(PjLinkMuteCommandParameter),
    ErrorStatus1,
    Lamp1,
    InputTogglingList1(u8),
    InputTogglingList2(u8),
    Name1,
    InfoManufacturer1,
    InfoProductName1,
    InfoOther1,
    Class1,
    SerialNumber2(u8),
    SoftwareVersion2(u8),
    InputTerminalName2(u8),
    InputResolution2(u8),
    RecommendResolution2(u8),
    FilterUsageTime2(u8),
    LampReplacementModelNumber2(u8),
    FilterReplacementModelNumber2(u8),
    SpeakerVolumeAdjustment2(bool),
    MicrophoneVolumeAdjustment2(bool),
    Freeze2(u8),
    Unknown,
}

pub enum PjLinkStatusCommand {
    Acknowledge2([[u8; 2]; 6]),
    Lookup2([[u8; 2]; 6]),
    ErrorStatus2([u8; 6]),
    Power2(u8),
    Input2(u8, u8),
}


pub trait PjLinkHandler: Sync + Send  {
    fn get_password(&mut self) -> Option<&String>;
    fn handle_command(&mut self, command: PjLinkCommand, raw_command: &PjLinkRawPayload) -> PjLinkResponse;
}

pub struct PjLinkListener<'a> {
    _nil: &'a bool,
    listener: TcpListener,
    handler: Arc<RwLock<dyn PjLinkHandler>>,
}

impl<'a> PjLinkListener<'a> {
    pub fn new(
        handler: impl PjLinkHandler + 'static,
        listener: TcpListener,
    ) -> PjLinkListener<'a> {
        return PjLinkListener {
            _nil: &false,
            handler: Arc::new(RwLock::new(handler)),
            listener: listener
        };
    }

    pub fn listen(&mut self) {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let handler = self.handler.clone();
                    thread::spawn(|| {
                        let mut connection_handler = PjLinkConnectionHandler {handler};
                        connection_handler.handle_connection(stream);
                    });
                },
                Err(e) => println!("Error! {}", e)
            }
        }
    }
}

struct PjLinkConnectionHandler {
    handler: Arc<RwLock<dyn PjLinkHandler>>
}

impl PjLinkConnectionHandler{
    fn handle_connection(&mut self, mut stream: TcpStream) {
        let lock_handler = &self.handler;

        if let Ok(mut handler) = lock_handler.write() {
            let mut auth_buffer = Vec::<u8>::new();
            let password: Option<&String> = handler.get_password();

            if password.is_none() {
                Self::generate_nullified_security(&mut auth_buffer);
            } else {
                let number = Self::generate_random_number();
                Self::generate_password_security(&mut auth_buffer, number);
            }

            stream.write(&auth_buffer).unwrap();
            stream.flush().unwrap();
        }

        'message: loop {
            let mut input_command_buffer = Vec::<u8>::new();
            'input: loop {
                let mut char_buffer = [0u8; 1];
                match stream.read_exact(&mut char_buffer) {
                    Ok(_) => {
                        if char_buffer[0] == PJLINK_TERMINATOR {
                            break 'input;
                        } else {
                            input_command_buffer.extend(char_buffer);
                        }
                    }
                    Err(_) => {
                        break 'message;
                    }
                }
            }

            let raw_command = self.to_raw_command(input_command_buffer);
            print!("Command:");

            for byte in raw_command.command_body {
                print!("{}", byte as char)
            }
            println!("");
            
            let command = self.get_command(&raw_command);

            if let Ok(mut handler) = lock_handler.write() {
                let response = handler.handle_command(command, &raw_command);
                let raw_response = self.to_raw_response(raw_command, response);
                let output_buffer = self.write_to_buffer(raw_response);
                match stream.write(&output_buffer) {
                    Ok(_) => {
                        match stream.flush() {
                            Ok(_) => continue 'message,
                            Err(_) => break 'message
                        }
                    }
                    Err(_) => break 'message
                }
            }
        }
    }

    fn get_command(&self, raw_command: &PjLinkRawPayload) -> PjLinkCommand {
        let transmission_parameter = &raw_command.transmission_parameter;
        let class = raw_command.header_and_class[1];
        let mut command_body_string = std::str::from_utf8(&[class]).unwrap().to_owned();
        command_body_string.push_str(
            std::str::from_utf8(&raw_command.command_body).unwrap()
        );
        let command_body_str = command_body_string.as_str();
        let is_class_2 = class == b'2';
        let transmission_parameter_len = transmission_parameter.len();

        return match command_body_str {
            "1POWR" => {
                let raw_parameter = transmission_parameter[0];
                let parameter = match raw_parameter as char {
                    '1' => PjLinkPowerCommandParameter::On,
                    '0' => PjLinkPowerCommandParameter::Off,
                    PJLINK_QUERY_CHAR => PjLinkPowerCommandParameter::Query,
                    _ => PjLinkPowerCommandParameter::Unknown, 
                };

                return PjLinkCommand::Power1(parameter);
            },
            "1CLSS" => PjLinkCommand::Class1,
            "1ERST" => PjLinkCommand::ErrorStatus1,
            "1LAMP" => PjLinkCommand::Lamp1,
            "1INFO" => PjLinkCommand::InfoOther1,
            "1INF1" => PjLinkCommand::InfoManufacturer1,
            "1INF2" => PjLinkCommand::InfoProductName1,
            "1NAME" => PjLinkCommand::Name1,
            "1AVMT" => {
                let parameter = if transmission_parameter_len == 1 && transmission_parameter[0] == PJLINK_QUERY {
                    PjLinkMuteCommandParameter::Query
                } else if transmission_parameter_len == 2 {
                    let raw_parameter = (transmission_parameter[0], transmission_parameter[1]);
                    match raw_parameter {
                        (b'1', b'1') => PjLinkMuteCommandParameter::Video(true),
                        (b'1', b'0') => PjLinkMuteCommandParameter::Video(false),
                        (b'2', b'1') => PjLinkMuteCommandParameter::Audio(true),
                        (b'2', b'0') => PjLinkMuteCommandParameter::Audio(false),
                        (b'3', b'1') => PjLinkMuteCommandParameter::AudioAndVideo(true),
                        (b'3', b'0') => PjLinkMuteCommandParameter::AudioAndVideo(false),
                        _ => PjLinkMuteCommandParameter::Unknown
                    }
                } else {
                    PjLinkMuteCommandParameter::Unknown
                };

                return PjLinkCommand::AvMute1(parameter);
            }
            "1INPT" | "2INPT" => {
                let parameter = if transmission_parameter_len == 1 && transmission_parameter[0] == PJLINK_QUERY {
                    PjLinkInputCommandParameter::Query
                } else if transmission_parameter_len == 2 {
                    let (input_char, input_value) = (transmission_parameter[0], transmission_parameter[1]);
                    match input_char {
                        b'1' => PjLinkInputCommandParameter::RGB(input_value),
                        b'2' => PjLinkInputCommandParameter::Video(input_value),
                        b'3' => PjLinkInputCommandParameter::Digital(input_value),
                        b'4' => PjLinkInputCommandParameter::Network(input_value),
                        b'5' => if is_class_2 {
                            PjLinkInputCommandParameter::Internal(input_value)
                        } else {
                            PjLinkInputCommandParameter::Unknown
                        }
                        _ => PjLinkInputCommandParameter::Unknown
                    }
                } else {
                    PjLinkInputCommandParameter::Unknown
                };

                return if is_class_2 {
                    PjLinkCommand::Input2(parameter)
                } else {
                    PjLinkCommand::Input1(parameter)
                }
            }
            _ => PjLinkCommand::Unknown
        }
    }

    fn to_raw_response(&self, raw_command: PjLinkRawPayload, response: PjLinkResponse) -> PjLinkRawPayload {
        let header_and_class: [u8; 2] = raw_command.header_and_class;
        let command_body: [u8; 4] = raw_command.command_body;
        let separator: u8 = PJLINK_RESPONSE_SEPARATOR;
        let transmission_parameter: Vec<u8> = match response {
            PjLinkResponse::Ok => Vec::from("OK"),
            PjLinkResponse::OutOfParameter => Vec::from("ERR2"),
            PjLinkResponse::UnavailableTime => Vec::from("ERR3"),
            PjLinkResponse::ProjectorOrDisplayFailure => Vec::from("ERR4"),
            PjLinkResponse::Undefined => Vec::from("ERR1"),
            PjLinkResponse::OkSingle(response_value) => Vec::from([response_value]),
            PjLinkResponse::OkMultiple(response_value) => Vec::from(response_value),
        };

        return PjLinkRawPayload {
            header_and_class,
            command_body,
            separator,
            transmission_parameter,
            terminator: PJLINK_TERMINATOR
        };
    }

    fn to_raw_command(&self, command: Vec<u8>) -> PjLinkRawPayload {
        let mut header_and_class: [u8; 2] = Default::default();
        let mut command_body: [u8; 4] = Default::default();
        let transmission_parameter: Vec<u8> = command[7..command.len()].to_vec();

        header_and_class.copy_from_slice(&command[0..2]);
        command_body.copy_from_slice(&command[2..6]);

        let command = PjLinkRawPayload {
            header_and_class,
            command_body,
            separator: command[6],
            transmission_parameter,
            terminator: PJLINK_TERMINATOR
        };

        return command;
    }

    fn write_to_buffer(&self, mut raw_response: PjLinkRawPayload) -> Vec<u8> {
        let mut buffer = Vec::<u8>::new();
        buffer.extend(&raw_response.header_and_class);
        buffer.extend(&raw_response.command_body);
        buffer.push(raw_response.separator);

        buffer.append(&mut raw_response.transmission_parameter);
        let buffer_last = buffer.len() - 1;

        if buffer[buffer_last] == b'\x00' {
            buffer[buffer_last] = PJLINK_TERMINATOR;
        } else {
            buffer.push(PJLINK_TERMINATOR);
        }

        return buffer;
    }

    fn generate_random_number() -> u32 {
        let mut rng = rand::thread_rng();
        return rng.next_u32()
    }

    fn generate_nullified_security(buffer: &mut Vec<u8>) {
        buffer.extend(PJLINK_NULLIFIED_SECURITY);
    }

    fn generate_password_security(buffer: &mut Vec<u8>, number: u32) {
        buffer.extend(PJLINK_SECURITY);
        buffer.extend(format!("{:08X}", number).as_bytes());
    }
}
