extern crate rand;

use std::any::Any;
use std::thread;
use std::sync::{RwLock, Arc};
use std::net::{TcpListener, TcpStream};
use std::convert::TryFrom;
use std::io::{Read, Write};
use rand::prelude::*;

pub fn teste() {
    println!("Teste")
}

pub const PJLINK_HEADER_CHAR: u8 = '%' as u8;
pub const PJLINK_COMMAND_SEPARATOR: u8 = 0x20; // space
pub const PJLINK_RESPONSE_SEPARATOR: u8 = 0x3d; // =
pub const PJLINK_TERMINATOR: u8 = 0x0d; // carriage return
pub const PJLINK_QUERY: u8 = '?' as u8;

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
    OK,
    Undefined,
    OutOfParameter,
    UnavailableTime,
    ProjectorOrDisplayFailure,
    Response(Box<dyn Any>)
}

pub enum PjLinkCommand {
    Search2,
    Power1(PjLinkPowerCommandParameter),
    Input1(PjLinkInputType, u8),
    Input2(PjLinkInputType, u8),
    AvMute1(PjLinkMuteType, u8),
    ErrorStatus1(u8),
    Lamp1(u8),
    InputTogglingList1(u8),
    InputTogglingList2(u8),
    Name1(u8),
    InfoManufacturer1(u8),
    InfoProductName1(u8),
    InfoOther1(u8),
    Class1(u8),
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

#[derive(Clone, Copy)]
pub enum PjLinkPowerCommandParameter {
    Off,
    On,
    Query,
    Unknown,
}

pub enum PjLinkPowerCommandStatus {
    Off,
    On,
    Cooling,
    WarmUp,
}

pub enum PjLinkInputType {
    RGB(u8),
    Video(u8),
    Digital(u8),
    Storage(u8),
    Network(u8),
    Internal(u8),
    Query,
}

pub enum PjLinkMuteType {
    Audio(bool),
    Video(bool),
    AudioAndVideo(bool),
    Query,
}

pub trait PjLinkHandler: Sync + Send  {
    fn get_password(&mut self) -> Option<&String>;
    fn handle_command(&mut self, command: PjLinkCommand, raw_command: PjLinkRawPayload) -> PjLinkResponse;
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
        let mut buffer = [0u8; 256];
        let lock_handler = &self.handler;

        if let Ok(mut handler) = lock_handler.write() {
            let password: Option<&String> = handler.get_password();

            if password.is_none() {
                Self::generate_nullified_security(&mut buffer);
            } else {
                let number = Self::generate_random_number();
                Self::generate_password_security(&mut buffer, number);
            }

            stream.write(&buffer).unwrap();
            stream.flush().unwrap();

            let mut input_command_buffer = [0u8; 256];
            stream.read(&mut input_command_buffer).unwrap();

            let raw_command = self.to_raw_command(input_command_buffer);
            let command = self.get_command(&raw_command);

            handler.handle_command(command, raw_command);
        }
    }

    fn get_command(&self, raw_command: &PjLinkRawPayload) -> PjLinkCommand {
        let transmission_parameter = &raw_command.transmission_parameter;
        let class = [raw_command.header_and_class[1]];
        let mut command_body_string = std::str::from_utf8(&class).unwrap().to_owned();
        command_body_string.push_str(
            std::str::from_utf8(&raw_command.command_body).unwrap()
        );
        let command_body_str = command_body_string.as_str();

        return match command_body_str {
            "1POWR" => {
                let raw_parameter = transmission_parameter[0];
                let parameter = match raw_parameter as char {
                    '1' => PjLinkPowerCommandParameter::On,
                    '0' => PjLinkPowerCommandParameter::Off,
                    '?' => PjLinkPowerCommandParameter::Query,
                    _ => PjLinkPowerCommandParameter::Unknown, 
                };

                return PjLinkCommand::Power1(parameter);
            }
            _ => PjLinkCommand::Unknown
        }
    }

    fn to_raw_command(&self, command: [u8; 256]) -> PjLinkRawPayload {
        let mut header_and_class: [u8; 2] = Default::default();
        let mut command_body: [u8; 4] = Default::default();
        let transmission_parameter: Vec<u8> = command[7..256].to_vec();

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

    fn generate_random_number() -> u32 {
        let mut rng = rand::thread_rng();
        return rng.next_u32()
    }

    fn generate_nullified_security(buffer: &mut [u8; 256]) {
        buffer[0..9].copy_from_slice(PJLINK_NULLIFIED_SECURITY);
    }

    fn generate_password_security(buffer: &mut [u8; 256], number: u32) {
        buffer[0..9].copy_from_slice(PJLINK_SECURITY);
        buffer[10..17].copy_from_slice(format!("{:08X}", number).as_bytes());
    }
}
