//!`pjlink-bridge` provides a base for implementing a full-featured PJLink server.
//! 
//! It's implemented based on [PJLink Specifications, Version 2.00](https://pjlink.jbmia.or.jp/english/data_cl2/PJLink_5-1.pdf).
//! 
//! Provides the following functionalities:
//! * [PjLinkServer](self::PjLinkServer): Spawns necessary TCP and UDP connections and listens to requests using [PjLinkListener](self::PjLinkListener).
//! * [PjLinkHandler](self::PjLinkHandler): Base trait for handling PJLink messages. This is implemented by who is using `pjlink-bridge`.
//! * [PjLinkListener](self::PjLinkListener): Listens to PJLink TCP (and UDP, if used) requests using provided connections.
//! 
//! # External Dependencies
//! * [rand](rand): to generate random numbers (used in PJLink Authentication procedure).
//! * [md5](md5): to calculate md5 hashes (used in PJLink Authentication procedure).
//! * [mac_address](mac_address): to get MAC address of network interface (used in PJLink Class 2 Search/Lookup procedures).
//! * [log](log)
//! 
//! # Useful Links
//! * [JBMIA's PJLink Class2 specification, manual and test software](https://pjlink.jbmia.or.jp/english/dl_class2.html): Contains related documents and tools,
//! including the PJLinkTEST4PJ software, that can be used as a test client.

//#![deny(missing_docs)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::thread::{self, JoinHandle};
use std::sync::{
    Mutex,
    Arc,
    atomic,
    atomic::AtomicU64
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::io;
use std::io::{Read, Write};
use lazy_static::lazy_static;
use rand::prelude::*;
use mac_address::get_mac_address;
use log::{info, warn, debug, trace};

/// PJLink header character (%).
/// 
/// Every PJLink message (except authentication hello) starts with this
/// character.
pub const PJLINK_HEADER: u8 = b'%';
/// PJLink command separator (0x20, space)
/// 
/// Messages coming from controller to projector use this character to
/// separate command body from transmission parameter.
/// 
/// ### Command example
/// ```"%1INPT 32\x0d"```
pub const PJLINK_COMMAND_SEPARATOR: u8 = 0x20; // space
/// PJLink response separator (=)
/// 
/// Messages coming from projector to controller (responses) use this
/// character to separate command body from transmission parameter.
/// 
/// ### Response example
/// ```"%2FREZ=OK\x0d"```
pub const PJLINK_RESPONSE_SEPARATOR: u8 = 0x3d; // =
/// PJLink terminator/end of sequence (0x0d, carriage return)
/// 
/// All PJLink messages must end with this character.
pub const PJLINK_TERMINATOR: u8 = b'\x0d'; // carriage return
/// PJLink query character (?), as char
/// 
/// All query requests use this chararcter to indicate a query request.
pub const PJLINK_QUERY_CHAR: char = '?';
/// PJLink query character (?), as u8
/// 
/// All query requests use this chararcter to indicate a query request.
pub const PJLINK_QUERY: u8 = PJLINK_QUERY_CHAR as u8;

/// PJLink nullified security header (PJLINK 0\x0d)
/// 
/// If the projector does not have authentication, this header is returned
/// to controller. Afterwards, controller can send requests without
/// password.
const PJLINK_NULLIFIED_SECURITY: &[u8; 9] = b"PJLINK 0\x0d";
/// PJLink authentication header (PJLINK 1 )
/// 
/// If the projector does have authentication, this header is returned
/// to controller with a hash (see PJLink specification). Afterwards,
/// controller sends first request with a hashed MD5 salt+password.
const PJLINK_SECURITY: &[u8; 9] = b"PJLINK 1 ";
/// PJLink authentication error (PJLINK ERRA\x0d)
/// 
/// Controller returned with an invalid or wrong password hash.
const PJLINK_SECURITY_ERRA: &[u8; 12] = b"PJLINK ERRA\x0d";

/// PJLink Class 2 broadcast search start (%2SRCH\x0d)
/// 
/// This is the message sent from controller to the projector over
/// UDP on broadcast address for querying all Class 2 projectors on local
/// network. This command doesn't use a command separator.
const PJLINK_BROADCAST_SEARCH_START: &[u8; 7] = b"%2SRCH\x0d";
/// PJLink Class 2 Acknoledge broadcast command body (ACKN)
/// 
/// This is the command body used for response message to broadcast
/// search request.
/// 
/// ### Usage in response string
/// ```"%2ACKN=00:00:00:00:00:00\x0d"```
pub const PJLINK_BROADCAST_MESSAGE_ACKN: &[u8; 5] = b"2ACKN";
/// PJLink Class 2 Lookup Notify command body (LKUP)
/// 
/// This is the command body used for spontaneous lookup message from projector
/// to controller.
/// 
/// ### Usage in response string
/// ```"%2LKUP=00:00:00:00:00:00\x0d"```
pub const PJLINK_BROADCAST_MESSAGE_LKUP: &[u8; 5] = b"2LKUP";
/// PJLink Class 2 Error Status Notify command body (ERST)
/// 
/// This is the command body used for spontaneous error status change message
/// from projector to controller.
/// 
/// ### Usage in response string
/// ```"%2ERST=001000\x0d"```
pub const PJLINK_BROADCAST_MESSAGE_ERST: &[u8; 5] = b"2ERST";
/// PJLink Class 2 Power Status Notify command body (POWR)
/// 
/// This is the command body used for spontaneous power status change message
/// from projector to controller.
/// 
/// ### Usage in response string
/// ```"%2POWR=1\x0d"```
pub const PJLINK_BROADCAST_MESSAGE_POWR: &[u8; 5] = b"2POWR";
/// PJLink Class 2 Input Notiy command body (POWER)
/// 
/// This is the command body used for spontaneous input change message
/// from projector to controller.
/// 
/// ### Usage in response string
/// ```"%2INPT=32\x0d"```
pub const PJLINK_BROADCAST_MESSAGE_INPT: &[u8; 5] = b"2INPT";

/// The maximum size of UDP datagrams sent to the server.
/// 
/// Rust's UDPSocket implementation needs a fixed buffer size due to
/// UDP nature, this is the maximum broadcast message size present
/// on PJLink specification.
const PJLINK_MAX_BROADCAST_BUFFER_SIZE: usize = 25;

/// PJLink Response Transmission Parameter: Sucessful Execution (OK)
/// 
/// This is the command response when the command is executed successfully,
/// without any response.
const PJLINK_RESPONSE_TRANSMISSION_PARAMETER_OK: &[u8; 2] = b"OK";

/// PJLink Response Transmission Parameter: Undefined Command (ERR1)
/// 
/// This is the command response when the command is unknown to the projector.
const PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR1: &[u8; 4] = b"ERR1";

/// PJLink Response Transmission Parameter: Out of Parameter (ERR2)
/// 
/// This is the command response when the command parameter is unknown or invalid.
const PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR2: &[u8; 4] = b"ERR2";

/// PJLink Response Transmission Parameter: Unavailable Time (ERR3)
/// 
/// This is the command response when the command cannot be received while projector is in
/// standby.
const PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR3: &[u8; 4] = b"ERR3";

/// PJLink Response Transmission Parameter: Projector/Display failure (ERR4)
/// 
/// This is the command response when the projector cannot be operated properly anymore,
/// due to an internal failure.
const PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR4: &[u8; 4] = b"ERR3";

lazy_static! {
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_OK_VEC: Vec<u8> = PJLINK_RESPONSE_TRANSMISSION_PARAMETER_OK.to_vec();
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_OK_VEC_HASH: u64 = {
        let mut hasher = DefaultHasher::new();
        PJLINK_RESPONSE_TRANSMISSION_PARAMETER_OK_VEC.hash(&mut hasher);
        hasher.finish()
    };
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR1_VEC: Vec<u8> = PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR1.to_vec();
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR1_VEC_HASH: u64 = {
        let mut hasher = DefaultHasher::new();
        PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR1_VEC.hash(&mut hasher);
        hasher.finish()
    };
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR2_VEC: Vec<u8> = PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR2.to_vec();
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR2_VEC_HASH: u64 = {
        let mut hasher = DefaultHasher::new();
        PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR2_VEC.hash(&mut hasher);
        hasher.finish()
    };
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR3_VEC: Vec<u8> = PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR3.to_vec();
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR3_VEC_HASH: u64 = {
        let mut hasher = DefaultHasher::new();
        PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR3_VEC.hash(&mut hasher);
        hasher.finish()
    };
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR4_VEC: Vec<u8> = PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR4.to_vec();
    static ref PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR4_VEC_HASH: u64 = {
        let mut hasher = DefaultHasher::new();
        PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR4_VEC.hash(&mut hasher);
        hasher.finish()
    };
}
/// PJLink Command/Response Line
/// 
/// This struct aims to match the PJLink's Command Line and Response Line,
/// without the [terminator](self::PJLINK_TERMINATOR).
/// 
/// ## Examples
/// ### Using [```new_command()```](PjLinkRawPayload::new_command)
/// ```
/// use pjlink_bridge::*;
/// 
/// let payload = PjLinkRawPayload::new_command(b'1', *b"POWR", vec![PJLINK_QUERY]);
/// ```
/// ### Using [```new_response()```](PjLinkRawPayload::new_response)
/// ```
/// use pjlink_bridge::*;
/// 
/// let payload = PjLinkRawPayload::new_response(b'1', *b"POWR", vec![b'0']);
/// ```
/// ### Struct instantiation 
/// ```
/// use pjlink_bridge::*;
/// 
/// let payload = PjLinkRawPayload {
///     header_and_class: [PJLINK_HEADER, b'1'],
///     command_body: *b"POWR",
///     separator: PJLINK_COMMAND_SEPARATOR,
///     transmission_parameter: vec![PJLINK_QUERY]
/// }
/// ```
pub struct PjLinkRawPayload {
    /// Contains PJLink's command body, with the class
    pub command_body_with_class: [u8; 5],
    /// Message separator.
    /// [PJLINK_COMMAND_SEPARATOR](self::PJLINK_COMMAND_SEPARATOR) for a command,
    /// [PJLINK_RESPONSE_SEPARATOR](self::PJLINK_RESPONSE_SEPARATOR) for a response,
    pub separator: u8,
    pub transmission_parameter: Vec<u8>,
}

impl PjLinkRawPayload {
    /// Utility method for generating a PJLink Command line (uses 
    /// [PJLINK_COMMAND_SEPARATOR](self::PJLINK_COMMAND_SEPARATOR) as separator)
    /// 
    /// **Arguments**:
    /// * `command_body_with_class`: PJLink command body with class. Value example: `*b"1POWR"`
    /// * `transmission_parameter`: PJLink transmission parameter.`
    pub fn new_command(
        command_body_with_class: [u8; 5],
        transmission_parameter: Vec<u8>
    ) -> PjLinkRawPayload {
        PjLinkRawPayload {
            command_body_with_class,
            separator: PJLINK_COMMAND_SEPARATOR,
            transmission_parameter
        }
    }

    /// Utility method for generating a PJLink Response line (uses 
    /// [PJLINK_RESPONSE_SEPARATOR](self::PJLINK_RESPONSE_SEPARATOR) as
    /// separator)
    /// 
    /// **Arguments**:
    /// * `command_body_with_class`: PJLink command body with class. Value example: `*b"1POWR"`
    /// * `transmission_parameter`: PJLink transmission parameter.`
    pub fn new_response(
        command_body_with_class: [u8; 5],
        transmission_parameter: Vec<u8>
    ) -> PjLinkRawPayload {
        PjLinkRawPayload {
            command_body_with_class,
            separator: PJLINK_RESPONSE_SEPARATOR,
            transmission_parameter
        }
    }

    /// Utility method for generating a PJLink Command/Response line from
    /// a buffer.
    ///
    /// **Arguments**:
    /// * `buffer`: Raw PJLink instruction buffer
    /// * `connection_id`: Connection ID
    pub fn from_buffer(buffer: &mut Vec<u8>, connection_id: &u64) -> PjLinkRawPayload {
        let mut command_body_with_class: [u8; 5] = Default::default();
        let transmission_parameter: Vec<u8> = buffer[7..buffer.len()].to_vec();

        command_body_with_class.copy_from_slice(&buffer[1..6]);

        let command = PjLinkRawPayload {
            command_body_with_class,
            separator: buffer[6],
            transmission_parameter,
        };

        debug!(
            "Parsed command. ConnectionId: {}; CmdBodyWithClass: {}; Sep: {}, TxParam: {}",
            *connection_id,
            String::from_utf8(command.command_body_with_class.to_vec()).unwrap_or_default(),
            command.separator as char,
            String::from_utf8(command.transmission_parameter.to_vec()).unwrap_or_default()
        );

        command
    }

    /// Updates a [PjLinkRawPayload](self::PjLinkRawPayload) instance with the provided
    /// [PjLinkResponse](self::PjLinkResponse).
    ///
    /// **Arguments**:
    /// * `response`: [PjLinkResponse](self::PjLinkResponse) enum item
    /// * `connection_id`: Connection ID
    pub fn update_with_response(self, response: PjLinkResponse, connection_id: &u64) -> PjLinkRawPayload {
        let transmission_parameter: Vec<u8> = match response {
            PjLinkResponse::Ok => PJLINK_RESPONSE_TRANSMISSION_PARAMETER_OK_VEC.clone(),
            PjLinkResponse::OutOfParameter => PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR2_VEC.clone(),
            PjLinkResponse::UnavailableTime => PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR3_VEC.clone(),
            PjLinkResponse::ProjectorOrDisplayFailure => PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR4_VEC.clone(),
            PjLinkResponse::Undefined => PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR1_VEC.clone(),
            PjLinkResponse::Single(response_value) => Vec::from([response_value]),
            PjLinkResponse::Multiple(response_value) => response_value,
            PjLinkResponse::Empty => Vec::new(),
        };
        let command_body_with_class: [u8; 5] = self.command_body_with_class;
        let separator: u8 = PJLINK_RESPONSE_SEPARATOR;
        
        debug!(
            "Parsed Response: ConnectionId: {}, CmdBodyWithClass: {}, Sep: {}, TxParam: {}",
            *connection_id,
            String::from_utf8(command_body_with_class.to_vec()).unwrap_or_default(),
            separator as char,
            String::from_utf8(transmission_parameter.clone()).unwrap_or_default()
        );

        PjLinkRawPayload {
            command_body_with_class,
            separator,
            transmission_parameter,
        }
    }


}

/// PJLink Response Transmission parameter
/// 
/// It's used as a response to [PjLinkCommand](self::PjLinkCommand) commands.
pub enum PjLinkResponse {
    /// Matches a PJLink Successful execution (```OK```) response parameter
    /// 
    /// ### As used in:
    /// ```%1POWR=OK```
    Ok,
    /// Matches a PJLink Undefined command (```ERR1```) response parameter.
    /// 
    /// ### As used in:
    /// ```%1NONE=ERR1```
    Undefined,
    /// Matches a PJLink Out of parameter (```ERR2```) response parameter.
    /// 
    /// ### As used in:
    /// ```%1INPT=ERR2```
    OutOfParameter,
    /// Matches a PJLink Unavailable time (```ERR3```) response parameter.
    /// 
    /// ### As used in:
    /// ```%1INPT=ERR3```
    UnavailableTime,
    /// Matches a PJLink Projector/Display failure (```ERR4```) response
    /// parameter.
    /// 
    /// ### As used in:
    /// ```%1INPT=ERR4```
    ProjectorOrDisplayFailure,
    /// A single character response parameter.
    /// 
    /// ### As used in:
    /// ```%1POWR=1```
    Single(u8),
    /// A multiple character response parameter.
    /// 
    /// ### As used in:
    /// ```%2INPT=2B```
    Multiple(Vec<u8>),
    /// An empty response parameter.
    /// 
    /// ### As used in:
    /// ```%2SVER=```
    Empty
}

impl From<String> for PjLinkResponse {
    fn from(from: String) -> Self {
        Vec::from(from.as_bytes()).into()
    }
}

impl From<Vec<u8>> for PjLinkResponse {
    fn from(from: Vec<u8>) -> Self {
        let mut hasher = DefaultHasher::new();
        from.hash(&mut hasher);
        let hash = hasher.finish();

        if hash == *PJLINK_RESPONSE_TRANSMISSION_PARAMETER_OK_VEC_HASH {Self::Ok}
        else if hash == *PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR1_VEC_HASH {Self::Undefined}
        else if hash == *PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR2_VEC_HASH {Self::OutOfParameter}
        else if hash == *PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR3_VEC_HASH {Self::UnavailableTime}
        else if hash == *PJLINK_RESPONSE_TRANSMISSION_PARAMETER_ERR4_VEC_HASH {Self::ProjectorOrDisplayFailure}
        else {
            let size = from.len();

            if size >= 1 {
                Self::Multiple(from)
            } else if size == 1 {
                Self::Single(*from.get(0).unwrap_or(&0))
            } else {
                Self::Empty
            }
        }
    }
}

/// Parameters for [1POWR](self::PjLinkCommand::Power1) command
pub enum PjLinkPowerCommandParameter {
    /// Power off action: `%1POWR 0`
    Off,
    /// Power on action: `%1POWR 1`
    On,
    /// Query action:`%1POWR ?`
    ///
    /// See: [PJLINK_QUERY](self::PJLINK_QUERY)
    Query,
    /// Used if an unknown parameter is received
    Unknown,
}

/// Response status for [1POWR](self::PjLinkCommand::Power1) command
pub struct PjLinkPowerCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkPowerCommandStatus {
    /// Projector is off: `%1POWR=0`
    pub const Off: u8 = b'0';
    /// Projector is on: `%1POWR=1`
    pub const On: u8 = b'1';
    /// Projector is in cooling state: `%1POWR=2`
    pub const Cooling: u8 = b'2';
    /// Projector is in warmup state: `%1POWR=3`
    pub const WarmUp: u8 = b'3';
}

/// Response status for [1CLSS](self::PjLinkCommand::Class1) command
pub struct PjLinkClassCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkClassCommandStatus {
    /// Projector supports Class 1 commands: `%1CLSS=1`
    pub const Class1: u8 = b'1';
    /// Projector supports Class 1 and 2 commands: `%1CLSS=2`
    pub const Class2: u8 = b'2';
}

/// Response status for each item of [1ERST](self::PjLinkCommand::ErrorStatus1) command.
///
/// See: [PjLinkCommand::ErrorStatus1](self::PjLinkCommand::ErrorStatus1)
pub struct PjLinkErrorStatusCommandStatusItem;
#[allow(non_upper_case_globals)]
impl PjLinkErrorStatusCommandStatusItem {
    /// Item is normal state / is not checked
    pub const Normal: u8 = b'0';
    /// Item is in warning state
    pub const Warning: u8 = b'1';
    /// Item is in error state
    pub const Error: u8 = b'2';
}

/// Parameter for [1INPT](self::PjLinkCommand::Input1) command 
pub enum PjLinkInputCommandParameter {
    /// 
    RGB(u8),
    Video(u8),
    Digital(u8),
    Storage(u8),
    Network(u8),
    Internal(u8),
    Query,
    Unknown,
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
pub enum PjLinkVolumeCommandParameter {
    Increase,
    Decrase,
    Unknown,
}

pub struct PjLinkInputResolutionCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkInputResolutionCommandStatus {
    pub const NoSignal: u8 = b'-';
    pub const Unknown: u8 = b'*';
}

pub enum PjLinkFreezeCommandParameter {
    Freeze,
    Unfreeze,
    Query,
    Unknown,
}
pub struct PjLinkFreezeCommandStatus;
#[allow(non_upper_case_globals)]
impl PjLinkFreezeCommandStatus {
    pub const Freezed: u8 = b'1';
    pub const Unfreezed: u8 = b'0';
}

pub enum PjLinkCommand {
    Search2,
    Power1(PjLinkPowerCommandParameter),
    Input1(PjLinkInputCommandParameter),
    Input2(PjLinkInputCommandParameter),
    AvMute1(PjLinkMuteCommandParameter),
    ErrorStatus1,
    Lamp1,
    InputTogglingList1,
    InputTogglingList2,
    Name1,
    InfoManufacturer1,
    InfoProductName1,
    InfoOther1,
    Class1,
    SerialNumber2,
    SoftwareVersion2,
    InputTerminalName2(PjLinkInputCommandParameter),
    InputResolution2,
    RecommendResolution2,
    FilterUsageTime2,
    LampReplacementModelNumber2,
    FilterReplacementModelNumber2,
    SpeakerVolumeAdjustment2(PjLinkVolumeCommandParameter),
    MicrophoneVolumeAdjustment2(PjLinkVolumeCommandParameter),
    Freeze2(PjLinkFreezeCommandParameter),
    Unknown,
}

impl PjLinkCommand {
    pub fn from_raw_payload(raw_command: &PjLinkRawPayload) -> PjLinkCommand {
        let transmission_parameter = &raw_command.transmission_parameter;
        let class = raw_command.command_body_with_class[0];
        let command_body_str = match std::str::from_utf8(&raw_command.command_body_with_class) {
            Ok(string) => string,
            Err(_) => return PjLinkCommand::Unknown
        };
        let is_class_2 = class == b'2';
        let transmission_parameter_len = transmission_parameter.len();

        match command_body_str {
            "1POWR" => {
                let raw_parameter = transmission_parameter[0];
                let parameter = match raw_parameter as char {
                    '1' => PjLinkPowerCommandParameter::On,
                    '0' => PjLinkPowerCommandParameter::Off,
                    PJLINK_QUERY_CHAR => PjLinkPowerCommandParameter::Query,
                    _ => PjLinkPowerCommandParameter::Unknown, 
                };

                PjLinkCommand::Power1(parameter)
            },
            "1INPT" | "2INPT" => {
                let parameter: PjLinkInputCommandParameter;
                if transmission_parameter_len == 1 && transmission_parameter[0] == PJLINK_QUERY {
                    parameter = PjLinkInputCommandParameter::Query
                } else if transmission_parameter_len == 2 {
                    let (input_char, input_value) = (transmission_parameter[0], transmission_parameter[1]);
                    parameter = Self::input_param_parse(is_class_2, input_char, input_value);
                } else {
                    parameter = PjLinkInputCommandParameter::Unknown
                };

                if is_class_2 {
                    PjLinkCommand::Input2(parameter)
                } else {
                    PjLinkCommand::Input1(parameter)
                }
            }
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

                PjLinkCommand::AvMute1(parameter)
            }
            "1ERST" => PjLinkCommand::ErrorStatus1,
            "1LAMP" => PjLinkCommand::Lamp1,
            "1INST" | "2INST" => if is_class_2 {
                PjLinkCommand::InputTogglingList2
            } else {
                PjLinkCommand::InputTogglingList1
            }
            "1NAME" => PjLinkCommand::Name1,
            "1INF1" => PjLinkCommand::InfoManufacturer1,
            "1INF2" => PjLinkCommand::InfoProductName1,
            "1INFO" => PjLinkCommand::InfoOther1,
            "1CLSS" => PjLinkCommand::Class1,
            "2SNUM" => PjLinkCommand::SerialNumber2,
            "2SVER" => PjLinkCommand::SoftwareVersion2,
            "2INNM" => {
                let parameter: PjLinkInputCommandParameter;
                if transmission_parameter_len == 3 {
                    if transmission_parameter[0] == PJLINK_QUERY {
                        let (input_char, input_value) = (transmission_parameter[1], transmission_parameter[2]);
                        parameter = Self::input_param_parse(true, input_char, input_value);
                    } else {
                        parameter = PjLinkInputCommandParameter::Unknown
                    }
                } else {
                    parameter = PjLinkInputCommandParameter::Unknown
                };

                PjLinkCommand::InputTerminalName2(parameter)
            },
            "2IRES" => PjLinkCommand::InputResolution2,
            "2RRES" => PjLinkCommand::RecommendResolution2,
            "2FILT" => PjLinkCommand::FilterUsageTime2,
            "2RLMP" => PjLinkCommand::LampReplacementModelNumber2,
            "2RFIL" => PjLinkCommand::FilterReplacementModelNumber2,
            "2SVOL" => {
                if transmission_parameter_len == 1 {
                    let is_increase = transmission_parameter[0] == b'1';
                    let is_decrease = transmission_parameter[0] == b'0';
                    return PjLinkCommand::SpeakerVolumeAdjustment2(if is_increase {
                        PjLinkVolumeCommandParameter::Increase
                    } else if is_decrease {
                        PjLinkVolumeCommandParameter::Decrase
                    } else {
                        PjLinkVolumeCommandParameter::Unknown
                    })
                }

                PjLinkCommand::Unknown
            },
            "2MVOL" => {
                if transmission_parameter_len == 1 {
                    let is_increase = transmission_parameter[0] == b'1';
                    let is_decrease = transmission_parameter[0] == b'0';
                    return PjLinkCommand::MicrophoneVolumeAdjustment2(if is_increase {
                        PjLinkVolumeCommandParameter::Increase
                    } else if is_decrease {
                        PjLinkVolumeCommandParameter::Decrase
                    } else {
                        PjLinkVolumeCommandParameter::Unknown
                    })
                }

                PjLinkCommand::Unknown
            },
            "2FREZ" => {
                if transmission_parameter_len == 1 {
                    if transmission_parameter[0] == PJLINK_QUERY {
                        return PjLinkCommand::Freeze2(PjLinkFreezeCommandParameter::Query);
                    } else {
                        let is_freeze = transmission_parameter[0] == b'1';
                        let is_unfreeze = transmission_parameter[0] == b'0';
                        return PjLinkCommand::Freeze2(if is_freeze {
                            PjLinkFreezeCommandParameter::Freeze
                        } else if is_unfreeze {
                            PjLinkFreezeCommandParameter::Unfreeze
                        } else {
                            PjLinkFreezeCommandParameter::Unknown
                        })
                    }
                }

                PjLinkCommand::Unknown
            },
            _ => PjLinkCommand::Unknown
        }
    }

    fn input_param_parse(
        is_class_2: bool,
        input_char: u8,
        input_value: u8,
    ) -> PjLinkInputCommandParameter {
        let is_invalid_below = input_value < b'1';
        let is_class_1_invalid_higher = !is_class_2 && (input_value > b'9');
        let is_class_2_invalid_higher = is_class_2
                                        && ((input_value > b'9' && input_value < b'A')
                                            || input_value > b'Z');

        if  is_invalid_below || is_class_1_invalid_higher || is_class_2_invalid_higher {
            PjLinkInputCommandParameter::Unknown                        
        } else {
            match input_char {
                b'1' => PjLinkInputCommandParameter::RGB(input_value),
                b'2' => PjLinkInputCommandParameter::Video(input_value),
                b'3' => PjLinkInputCommandParameter::Digital(input_value),
                b'4' => PjLinkInputCommandParameter::Storage(input_value),
                b'5' => PjLinkInputCommandParameter::Network(input_value),
                b'6' => if is_class_2 {
                    PjLinkInputCommandParameter::Internal(input_value)
                } else {
                    PjLinkInputCommandParameter::Unknown
                }
                _ => PjLinkInputCommandParameter::Unknown
            }
        } 
    }
}

pub enum PjLinkStatusCommand {
    Acknowledge2([[u8; 2]; 6]),
    Lookup2([[u8; 2]; 6]),
    ErrorStatus2([u8; 6]),
    Power2(u8),
    Input2(u8, u8),
}

pub trait PjLinkHandler: Send {
    fn get_password(&mut self, connection_id: &u64) -> Option<String>;
    fn handle_command(&mut self, command: PjLinkCommand, raw_command: &PjLinkRawPayload, connection_id: &u64) -> PjLinkResponse;
}

pub type PjLinkHandlerShared = Arc<Mutex<dyn PjLinkHandler>>;

pub type PjLinkServerTcpOnlyResult<'a> = (Arc<PjLinkListener<'a>>, JoinHandle<()>);
pub type PjLinkServerTcpUdpResult<'a> = (Arc<PjLinkListener<'a>>, JoinHandle<()>, JoinHandle<()>);

pub struct PjLinkServer {}

impl PjLinkServer{
    pub fn listen_tcp_udp<'a>(
        handler: PjLinkHandlerShared,
        tcp_bind_address: String,
        udp_bind_address: String,
        port: String,
    ) -> PjLinkServerTcpUdpResult<'a> {
        let tcp_listener = TcpListener::bind(format!("{}:{}", tcp_bind_address, port)).unwrap();

        let udp_socket = UdpSocket::bind(format!("{}:{}", udp_bind_address, port)).unwrap();
        let listener = PjLinkListener::new(handler, tcp_listener, udp_socket);
        let udp_address_clone = udp_bind_address;
        let listener_clone = listener.clone();
        let listener_result_clone = listener.clone();

        let port_clone = port.clone();
        
        let handle = thread::spawn(move || {
            Self::listen_tcp_internal(tcp_bind_address.clone(), port, listener.clone());
        });

        let udp_handle = thread::spawn(move || {
            info!("Running UDP Listener on {}:{}", udp_address_clone, port_clone);
            listener_clone.listen_multicast();
        });

        (listener_result_clone.clone(), handle, udp_handle)
    }

    pub fn listen_tcp_only<'a>(
        handler: PjLinkHandlerShared,
        tcp_bind_address: String,
        port: String
    ) -> PjLinkServerTcpOnlyResult<'a> {
        let tcp_listener = TcpListener::bind(format!("{}:{}", tcp_bind_address, port)).unwrap();
        let listener = PjLinkListener::new_without_broadcast(handler, tcp_listener);
        let listener_clone = listener.clone();
        
        let handle = thread::spawn(move || {
            Self::listen_tcp_internal(tcp_bind_address, port, listener);
        });

        (listener_clone, handle)
    }

    fn listen_tcp_internal(address: String, port: String, listener: PjLinkListenerShared<'static>) {
        info!("Running TCP Listener on {}:{}", address, port);
        listener.listen();
    }
}

pub struct PjLinkListener<'a> {
    _nil: &'a bool,
    shared_handler: PjLinkHandlerShared,
    shared_connection_counter: Arc<AtomicU64>,
    tcp_listener: TcpListener,
    udp_socket: Option<UdpSocket>
}

pub type PjLinkListenerShared<'a> = Arc<PjLinkListener<'a>>;

impl<'a> PjLinkListener<'a> {
    pub fn new(
        shared_handler: PjLinkHandlerShared,
        tcp_listener: TcpListener,
        udp_socket: UdpSocket
    ) -> PjLinkListenerShared<'a> {
        Arc::new(PjLinkListener {
            _nil: &false,
            shared_handler,
            shared_connection_counter: Arc::new(AtomicU64::new(0)),
            tcp_listener,
            udp_socket: Option::Some(udp_socket),
        })
    }

    pub fn new_without_broadcast(
        shared_handler: Arc<Mutex<dyn PjLinkHandler>>,
        tcp_listener: TcpListener
    ) -> PjLinkListenerShared<'a> {
        Arc::new(PjLinkListener {
            _nil: &false,
            shared_handler,
            shared_connection_counter: Arc::new(AtomicU64::new(0)),
            tcp_listener,
            udp_socket: Option::None,
        })
    }

    pub fn listen(&self) {
        let shared_handler = &self.shared_handler;
        let listener = &self.tcp_listener;

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let handler = shared_handler.clone();
                    let shared_connection_counter = self.shared_connection_counter.clone();

                    thread::spawn(move || {
                        let mut connection_handler = PjLinkConnectionHandler {
                            handler,
                            shared_connection_counter,
                        };
                        connection_handler.handle_connection(stream);
                    });
                },
                Err(e) => debug!("Error on received connection! {}", e)
            }
        }
    }

    pub fn listen_multicast(&self) {
        let shared_handler = &self.shared_handler;
        if let Some(socket) = &self.udp_socket {
            socket.set_broadcast(true).unwrap();
            let port = socket.local_addr().unwrap().port();
            let shared_connection_counter = self.shared_connection_counter.clone();

            let handler = shared_handler.clone();
            let mut connection_handler = PjLinkConnectionHandler {
                handler,
                shared_connection_counter,
            };
            connection_handler.handle_connection_multicast(socket, port);
        }
    }
}

struct PjLinkConnectionHandler {
    handler: Arc<Mutex<dyn PjLinkHandler>>,
    shared_connection_counter: Arc<AtomicU64>,
}

#[inline(always)]
fn get_empty_socket_addr<E>(_e: E) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0,0,0,0)), 0)
}

impl PjLinkConnectionHandler {
    fn handle_connection(&mut self, mut stream: TcpStream) {
        let lock_handler = &self.handler; 
        let mut use_auth = false;
        let mut password_salt: Option<String> = Option::None;
        let mut password: Option<String> = Option::None;
        let mut has_authenticated = false;
        let connection_id = (*self.shared_connection_counter).fetch_add(1, atomic::Ordering::SeqCst);

        if let Ok(mut handler) = lock_handler.lock() {
            password = handler.get_password(&connection_id);
            match Self::handle_password_input(&mut stream, &password, &connection_id) {
                Ok((use_auth_result, password_salt_result)) => {
                    use_auth = use_auth_result;
                    password_salt = password_salt_result;
                }
                Err(e) => {
                    debug!("Failed to read password! ConnectionId: {}, {}", connection_id, e);
                    return;
                }
            }
        }

        'message: loop {
            let mut input_command_buffer = Vec::<u8>::new();
            debug!("Waiting for command! ConnectionId: {}, Host: {}", connection_id, stream.peer_addr().unwrap_or_else(get_empty_socket_addr));

            if let Err(e) = Self::read_command(&mut input_command_buffer, &mut stream, &connection_id) {
                debug!("Failed to read command! ConnectionId: {}, {}", connection_id, e);
                break 'message;
            }

            if use_auth && (!has_authenticated || (input_command_buffer[0] != PJLINK_HEADER)) {
                match Self::handle_password_hash_response(
                    has_authenticated,
                    &mut input_command_buffer,
                    &password,
                    &password_salt,
                    &mut stream,
                    &connection_id
                ) {
                    Ok(has_authenticated_response) => {
                        if !has_authenticated_response {
                            break 'message;
                        } else {
                            has_authenticated = true;
                        }
                    },
                    Err(e) => {
                        debug!("Error while checking authentication! ConnectionId: {}, {}", connection_id, e);
                        break 'message
                    }
                }
            }

            let raw_command = PjLinkRawPayload::from_buffer(&mut input_command_buffer, &connection_id);
            let command = PjLinkCommand::from_raw_payload(&raw_command);

            if let Ok(mut handler) = lock_handler.lock() {
                let response = handler.handle_command(command, &raw_command, &connection_id);
                let raw_response = raw_command.update_with_response(response, &connection_id);
                let output_buffer = Self::write_to_buffer(raw_response);
                match stream.write(&output_buffer) {
                    Ok(_) => {
                        match stream.flush() {
                            Ok(_) => continue 'message,
                            Err(e) => {
                                debug!("Error when flushing socket: ConnectionId: {}, {}", connection_id, e);
                                break 'message;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to lock PjLinkHandler: ConnectionId: {}, {}", connection_id, e);
                        break 'message;
                    }
                }
            }
        }
    }

    fn handle_connection_multicast(&mut self, stream: &UdpSocket, port: u16) {
        'message: loop{
            let mut input_command_buffer: Vec<u8> = Vec::new();
            let mut input_command: Vec<u8> = Vec::new();
            let mut message_origin: SocketAddr;
            input_command_buffer.resize(PJLINK_MAX_BROADCAST_BUFFER_SIZE, 0);

            match stream.recv_from(&mut input_command_buffer) {
                Ok((_, origin)) => {
                    let mut is_valid_command = false;

                    trace!("UDP message received! RawMessage: {:?}", input_command_buffer);
                    message_origin = origin;

                    for char in input_command_buffer.iter() {
                        input_command.push(*char);

                        if *char == PJLINK_TERMINATOR {
                            is_valid_command = true;
                            break;
                        }
                    }

                    if is_valid_command {
                        debug!(
                            "UDP message received! ParsedMessage: {:?}",
                            String::from_utf8(input_command.clone()).unwrap_or_default()
                        );
                    } else {
                        debug!("UDP message doesn't end with Carriage Return. Origin: {}", origin);
                    }
                }
                Err(e) => {
                    debug!("UDP message handling failed: {}", e);
                    continue 'message;
                }
            }

            if input_command == PJLINK_BROADCAST_SEARCH_START {
                // TODO a way to get mac address by broadcast address' associated
                // interface
                let mac_address = match get_mac_address() {
                    Ok(Some(mac)) => format!("{}", mac),
                    Ok(None) | Err(_) => {
                        debug!("UDP: 2SRCH: Cannot infer MAC Address, sending null");
                        "00:00:00:00:00:00".to_string()
                    }
                };

                let response = PjLinkRawPayload {
                    command_body_with_class: *PJLINK_BROADCAST_MESSAGE_ACKN,
                    separator: PJLINK_RESPONSE_SEPARATOR,
                    transmission_parameter: Vec::from(mac_address)
                };

                let output_buffer = Self::write_to_buffer(response);
                Self::send_multicast_message(&mut message_origin, port, output_buffer);
            }
        }
    }


    fn write_to_buffer(mut raw_response: PjLinkRawPayload) -> Vec<u8> {
        let mut buffer = vec![PJLINK_HEADER];
        buffer.extend(&raw_response.command_body_with_class);
        buffer.push(raw_response.separator);

        buffer.append(&mut raw_response.transmission_parameter);
        let buffer_last = buffer.len() - 1;

        if buffer[buffer_last] == b'\x00' {
            buffer[buffer_last] = PJLINK_TERMINATOR;
        } else {
            buffer.push(PJLINK_TERMINATOR);
        }

        buffer
    }

    fn read_command(input_command_buffer: &mut Vec<u8>, stream: &mut TcpStream, connection_id: &u64) -> Result<(), io::Error> {
        loop {
            let mut char_buffer = [0u8; 1];
            match stream.read_exact(&mut char_buffer) {
                Ok(_) => {
                    trace!("Read command char. ConnectionId: {}, Char: {}", *connection_id, char_buffer[0]);
                    if char_buffer[0] == PJLINK_TERMINATOR {
                        return Result::Ok(());
                    } else {
                        input_command_buffer.extend(char_buffer);
                    }
                }
                Err(e) => {
                    return Result::Err(e);
                }
            }
        }
    }

    fn send_multicast_message(message_origin: &mut SocketAddr, port: u16, output_buffer: Vec<u8>) {
        match UdpSocket::bind("0.0.0.0:0") {
            Ok(socket) => {
                message_origin.set_port(port);

                debug!("UDP: Will send response to: {}", message_origin);
                if let Err(e) = socket.connect(*message_origin) {
                    debug!("UDP: Error on connecting to remote host. {}", e);
                };

                if let Err(e) = socket.send(&output_buffer) {
                    debug!("UDP: Error on sending datagram message to remote host. {}", e);
                }

                trace!(
                    "UDP message sent! RawParsedMessage: {:?}",
                    output_buffer
                );

                debug!(
                    "UDP message sent! ParsedMessage: {:?}",
                    String::from_utf8(output_buffer).unwrap_or_default()
                );
            },
            Err(e) => {
                debug!("UDP: Error on opening local port to send response. {}", e);
            }
        }
 
    }

    fn handle_password_input(
        stream: &mut TcpStream,
        password: &Option<String>,
        connection_id: &u64,
    ) -> Result<(bool, Option<String>), io::Error> {
        let mut auth_buffer = Vec::<u8>::new();
        let mut password_salt = Option::None;
        let mut use_auth = false;

        if password.is_none() {
            debug!("PJLink Security: nullified; ConnectionId: {}", connection_id);
            Self::generate_nullified_security(&mut auth_buffer);
        } else {
            let string_salt = format!("{:08X}", Self::generate_random_number());
            Self::generate_password_security(&mut auth_buffer, &string_salt);
            debug!(
                "PJLink Security: password; ConnectionId: {}, Response: {}",
                *connection_id,
                String::from_utf8(auth_buffer.clone()).unwrap_or_default()
            );
            password_salt = Option::Some(string_salt);
            use_auth = true;
        }

        if let Err(err) = stream.write_all(&auth_buffer) {
            return Err(err);
        }
        if let Err(err) = stream.flush() {
            return Err(err);
        };

        Ok((use_auth, password_salt))
    }

    fn handle_password_hash_response(
        has_authenticated: bool,
        input_command_buffer: &mut Vec<u8>,
        password: &Option<String>,
        password_salt: &Option<String>,
        stream: &mut TcpStream,
        connection_id: &u64
    ) -> Result<bool, io::Error> {
        let mut auth_error = false;
        let mut has_authenticated_response = has_authenticated;

        if !has_authenticated {
            if input_command_buffer.len() > 32 {
                let mut input_password_hash: [u8; 32] = [0u8; 32];
                input_password_hash.copy_from_slice(&input_command_buffer[0..32]);

                let mut internal_password_string = password_salt.clone()
                    .unwrap();
                internal_password_string.push_str(&(password.clone().unwrap()));

                let internal_password = internal_password_string.as_bytes();
                let internal_password_hash = md5::compute(internal_password);

                debug!(
                    "Received password hash! ConnectionId: {}, Hash: {}",
                    *connection_id,
                    String::from_utf8(input_password_hash.to_vec()).unwrap_or_default()
                );

                if format!("{:x}", internal_password_hash).as_bytes() == input_password_hash {
                    debug!("Password accepted! ConnectionId: {}", *connection_id);
                    has_authenticated_response = true;
                } else {
                    debug!("Password denied! ConnectionId: {}", *connection_id);
                    auth_error = true;
                }
            } else {
                debug!("Password denied (command is too short)! ConnectionId: {}", *connection_id);
                auth_error = true;
            }

            if auth_error {
                match stream.write(PJLINK_SECURITY_ERRA) {
                    Ok(_) => return Result::Ok(false),
                    Err(e) => return Result::Err(e)
                }
            }
        }
        
        if has_authenticated_response {
            input_command_buffer.drain(0..32);
        }

        Result::Ok(has_authenticated_response)
    }

    fn generate_random_number() -> u32 {
        let mut rng = rand::thread_rng();
        rng.next_u32()
    }

    fn generate_nullified_security(buffer: &mut Vec<u8>) {
        buffer.extend(PJLINK_NULLIFIED_SECURITY);
    }

    fn generate_password_security(buffer: &mut Vec<u8>, number: &str) {
        buffer.extend(PJLINK_SECURITY);
        buffer.extend(number.as_bytes());
        buffer.push(PJLINK_TERMINATOR);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    struct PjLinkMockHandler {
        handle_command_fn: fn(PjLinkCommand, &PjLinkRawPayload) -> PjLinkResponse,
        get_password_fn: fn() -> Option<String>
    }

    impl PjLinkHandler for PjLinkMockHandler {
        fn handle_command(&mut self, command: PjLinkCommand, raw_command: &PjLinkRawPayload, _connection_id: &u64) -> PjLinkResponse {
            (self.handle_command_fn)(command, raw_command)
        }

        fn get_password(&mut self, _connection_id: &u64) -> Option<String> {
            (self.get_password_fn)()
        }
    }

    fn _simple_mock_handler() -> PjLinkHandlerShared {
        Arc::new(Mutex::new(PjLinkMockHandler {
            handle_command_fn: |_command, _raw_command| PjLinkResponse::OutOfParameter,
            get_password_fn: || Option::None
        }))
    }

    #[test]
    fn it_converts_1powr_query_to_powr_query_enum() {
        let raw_command = PjLinkRawPayload::new_command(*b"1POWR", vec![PJLINK_QUERY]);
        let command = PjLinkCommand::from_raw_payload(&raw_command);
        assert!(matches!(command, PjLinkCommand::Power1(PjLinkPowerCommandParameter::Query)));
    }

    #[test]
    fn it_converts_1powr_on_to_powr_on_enum() {
        let raw_command = PjLinkRawPayload::new_command(*b"1POWR", vec![b'1']);
        let command = PjLinkCommand::from_raw_payload(&raw_command);
        assert!(matches!(command, PjLinkCommand::Power1(PjLinkPowerCommandParameter::On)));
    }

    #[test]
    fn it_converts_1powr_off_to_powr_off_enum() {
        let raw_command = PjLinkRawPayload::new_command(*b"1POWR", vec![b'0']);
        let command = PjLinkCommand::from_raw_payload(&raw_command);
        assert!(matches!(command, PjLinkCommand::Power1(PjLinkPowerCommandParameter::Off)));
    }

    #[test]
    fn it_converts_1powr_garbage_to_powr_unknown_enum() {
        let raw_command = PjLinkRawPayload::new_command(*b"1POWR", vec![b'b', b'2']);
        let command = PjLinkCommand::from_raw_payload(&raw_command);
        assert!(matches!(command, PjLinkCommand::Power1(PjLinkPowerCommandParameter::Unknown)));
    }
}