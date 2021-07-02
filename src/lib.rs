use std::any::Any;

use std::net::{TcpListener, TcpStream};
use std::io::{Read};

pub fn teste() {
    println!("Teste")
}

pub const PJLINK_HEADER_CHAR: u8 = '%' as u8;
pub const PJLINK_COMMAND_SEPARATOR: u8 = 0x20; // space
pub const PJLINK_RESPONSE_SEPARATOR: u8 = 0x3d; // =
pub const PJLINK_TERMINATOR: u8 = 0x0d; // carriage return
pub const PJLINK_QUERY: u8 = '?' as u8;

pub const PJLINK_NULLIFIED_SECURITY: &[u8; 9] = b"PJLINK 0\x0d";

pub struct PjLinkRawPayload {
    header_and_class: [u8; 2],
    command_body: [u8; 4],
    separator: u8,
    transmission_parameter: Box<[u8]>,
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

pub trait PjLinkHandler {
    fn new() -> Self;
    fn handle_command(&mut self, command: PjLinkCommand, raw_command: PjLinkRawPayload) -> PjLinkResponse;
}

pub struct PjLinkListener{
    listener: TcpListener
}

impl PjLinkListener {
    pub fn new() -> Self {
        return PjLinkListener {listener: PjLinkListener::default_listener()};
    }

    pub fn custom(listener: TcpListener) -> Self {
        return PjLinkListener {listener};
    }

    fn default_listener() -> TcpListener {
        return TcpListener::bind("127.0.0.1:4352").unwrap();
    }

    pub fn listen(&self) {
        for stream in self.listener.incoming() {
            let stream = stream.unwrap();

            self.handle_connection(stream);
        }
    }

    fn handle_connection(&self, mut stream: TcpStream) {
        let mut buffer = [0u8; 256];
       
        
    }

    #[inline(always)]
    fn generate_nullified_security(mut buffer: [u8; 256]) {
        buffer[0..9].copy_from_slice(PJLINK_NULLIFIED_SECURITY);
    }
}
