use std::cmp::Ordering;
use std::collections::VecDeque;

use hidapi::{HidDevice, HidResult};


/// Bytestring sent from computer to oximeter to establish communication.
pub const INIT_BYTESTRING: &[u8] = &[
    0x7d, 0x81, 0xa7, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
    0x7d, 0x81, 0xa2, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
];

/// Calculates the one-byte checksum which appears at the end of each command or response.
pub fn calculate_checksum(bytes: &[u8]) -> u8 {
    // sum of all preceding bytes (including command/response byte)
    // wrapping modulo 128 (equivalent to bitand 127)
    let mut sum_byte: u8 = 0x00;
    for b in bytes {
        // bytes wrap around at 256, but because 128 is a divisor of 256,
        // we don't lose anything by only wrapping at the end
        sum_byte = sum_byte.wrapping_add(*b);
    }
    sum_byte % 128
}

/// Verifies whether the checksum at the end of the given command or response matches the checksum
/// calculated on the fly.
pub fn is_checksum_ok(bytes: &[u8]) -> bool {
    let provided_checksum = if let Some(c) = bytes.last() {
        *c
    } else {
        // vacuous truth, I guess
        return true;
    };
    let calculated_checksum = calculate_checksum(&bytes[0..bytes.len()-1]);
    provided_checksum == calculated_checksum
}


/// The code of a command being issued.
///
/// The corresponding response code to a command code is mostly `command_code ^ 0x70` and vice
/// versa. However, there are exceptions to this rule.
#[derive(Clone, Copy, Debug, Hash)]
pub enum CommandCode {
    ReadyCommand,
    GetDeviceNameCommand,
    GetVersionInfoCommand,
    SetDateTimeCommand,
    ReadPropertyCommand,
    SetPropertyCommand,
    GetAuxiliaryDataCommand,
    KeepAliveCommand,
    LiveDataCommand,
    AdvanceAndShowAutoRecordedFileHeaderCommand,
    ReadAutoRecordedFileCommand,
    FileStoreInfoCommand,
    ManuallyRecordedFileMetadataCommand,
    ReadPulseFromManuallyRecordedFileCommand,
    ReadOxygenFromManuallyRecordedFileCommand,

    ReadyResponse,
    GetDeviceNameResponse,
    GetVersionInfoResponse,
    ManuallyRecordedFileMetadataResponse,
    ReadPulseFromManuallyRecordedFileResponse,
    ReadOxygenFromManuallyRecordedFileResponse,
    GetAuxiliaryDataResponse,
    // no response to KeepAliveCommand
    LiveDataResponse,
    AdvanceAndShowAutoRecordedFileHeaderResponse,
    ReadAutoRecordedFileResponse,
    FileStoreInfoResponse,
    SetDateTimeResponse,
    ReadPropertyResponse,
    SetPropertyResponse,

    Other(u8),
}
impl From<u8> for CommandCode {
    fn from(b: u8) -> Self {
        match b {
            0x80 => Self::ReadyCommand,
            0x81 => Self::GetDeviceNameCommand,
            0x82 => Self::GetVersionInfoCommand,
            0x83 => Self::SetDateTimeCommand,
            0x8E => Self::ReadPropertyCommand,
            0x8F => Self::SetPropertyCommand,
            0x90 => Self::GetAuxiliaryDataCommand,
            0x9A => Self::KeepAliveCommand,
            0x9B => Self::LiveDataCommand,
            0x9C => Self::AdvanceAndShowAutoRecordedFileHeaderCommand,
            0x9D => Self::ReadAutoRecordedFileCommand,
            0x9F => Self::FileStoreInfoCommand,
            0xA0 => Self::ManuallyRecordedFileMetadataCommand,
            0xA2 => Self::ReadPulseFromManuallyRecordedFileCommand,
            0xA3 => Self::ReadOxygenFromManuallyRecordedFileCommand,

            0xF0 => Self::ReadyResponse,
            0xF1 => Self::GetDeviceNameResponse,
            0xF2 => Self::GetVersionInfoResponse,
            0xD0 => Self::ManuallyRecordedFileMetadataResponse,
            0xD2 => Self::ReadPulseFromManuallyRecordedFileResponse,
            0xD3 => Self::ReadOxygenFromManuallyRecordedFileResponse,
            0xE0 => Self::GetAuxiliaryDataResponse,
            0xEB => Self::LiveDataResponse,
            0xEC => Self::AdvanceAndShowAutoRecordedFileHeaderResponse,
            0xED => Self::ReadAutoRecordedFileResponse,
            0xEF => Self::FileStoreInfoResponse,
            0xF3 => Self::SetDateTimeResponse,
            0xFE => Self::ReadPropertyResponse,
            0xFF => Self::SetPropertyResponse,

            other => Self::Other(other),
        }
    }
}
impl From<&CommandCode> for u8 {
    fn from(code: &CommandCode) -> Self {
        match code {
            CommandCode::ReadyCommand => 0x80,
            CommandCode::GetDeviceNameCommand => 0x81,
            CommandCode::GetVersionInfoCommand => 0x82,
            CommandCode::SetDateTimeCommand => 0x83,
            CommandCode::ReadPropertyCommand => 0x8E,
            CommandCode::SetPropertyCommand => 0x8F,
            CommandCode::GetAuxiliaryDataCommand => 0x90,
            CommandCode::KeepAliveCommand => 0x9A,
            CommandCode::LiveDataCommand => 0x9B,
            CommandCode::AdvanceAndShowAutoRecordedFileHeaderCommand => 0x9C,
            CommandCode::ReadAutoRecordedFileCommand => 0x9D,
            CommandCode::FileStoreInfoCommand => 0x9F,
            CommandCode::ManuallyRecordedFileMetadataCommand => 0xA0,
            CommandCode::ReadPulseFromManuallyRecordedFileCommand => 0xA2,
            CommandCode::ReadOxygenFromManuallyRecordedFileCommand => 0xA3,

            CommandCode::ReadyResponse => 0xF0,
            CommandCode::GetDeviceNameResponse => 0xF1,
            CommandCode::GetVersionInfoResponse => 0xF2,
            CommandCode::ManuallyRecordedFileMetadataResponse => 0xD0,
            CommandCode::ReadPulseFromManuallyRecordedFileResponse => 0xD2,
            CommandCode::ReadOxygenFromManuallyRecordedFileResponse => 0xD3,
            CommandCode::GetAuxiliaryDataResponse => 0xE0,
            CommandCode::LiveDataResponse => 0xEB,
            CommandCode::AdvanceAndShowAutoRecordedFileHeaderResponse => 0xEC,
            CommandCode::ReadAutoRecordedFileResponse => 0xED,
            CommandCode::FileStoreInfoResponse => 0xEF,
            CommandCode::SetDateTimeResponse => 0xF3,
            CommandCode::ReadPropertyResponse => 0xFE,
            CommandCode::SetPropertyResponse => 0xFF,

            CommandCode::Other(b) => *b,
        }
    }
}
impl From<CommandCode> for u8 {
    fn from(code: CommandCode) -> Self {
        (&code).into()
    }
}
impl PartialEq for CommandCode {
    fn eq(&self, other: &Self) -> bool {
        let self_u8: u8 = self.into();
        let other_u8: u8 = other.into();
        self_u8 == other_u8
    }
}
impl Eq for CommandCode {
}
impl PartialOrd for CommandCode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_u8: u8 = self.into();
        let other_u8: u8 = other.into();
        Some(self_u8.cmp(&other_u8))
    }
}
impl Ord for CommandCode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}


/// The code of a property. Properties can be read using `CommandCode::ReadPropertyCommand` and
/// written using `CommandCode::SetPropertyCommand`.
#[derive(Clone, Copy, Debug, Hash)]
pub enum PropertyCode {
    DeviceId,
    UnknownProperty04,
    AutoRecordedFiles,
    RecordingMode,

    Other(u8),
}
impl From<u8> for PropertyCode {
    fn from(b: u8) -> Self {
        match b {
            0x03 => Self::DeviceId,
            0x04 => Self::UnknownProperty04,
            0x06 => Self::AutoRecordedFiles,
            0x07 => Self::RecordingMode,

            other => Self::Other(other),
        }
    }
}
impl From<&PropertyCode> for u8 {
    fn from(code: &PropertyCode) -> Self {
        match code {
            PropertyCode::DeviceId => 0x03,
            PropertyCode::UnknownProperty04 => 0x04,
            PropertyCode::AutoRecordedFiles => 0x06,
            PropertyCode::RecordingMode => 0x07,

            PropertyCode::Other(b) => *b,
        }
    }
}
impl From<PropertyCode> for u8 {
    fn from(code: PropertyCode) -> Self {
        (&code).into()
    }
}
impl PartialEq for PropertyCode {
    fn eq(&self, other: &Self) -> bool {
        let self_u8: u8 = self.into();
        let other_u8: u8 = other.into();
        self_u8 == other_u8
    }
}
impl Eq for PropertyCode {
}
impl PartialOrd for PropertyCode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_u8: u8 = self.into();
        let other_u8: u8 = other.into();
        Some(self_u8.cmp(&other_u8))
    }
}
impl Ord for PropertyCode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}


/// The recording mode for which the oximeter is currently configured.
#[derive(Clone, Copy, Debug, Hash)]
pub enum RecordingMode {
    Automatic,
    Manual,

    Other(u16),
}
impl From<u16> for RecordingMode {
    fn from(b: u16) -> Self {
        match b {
            0x0000 => Self::Automatic,
            0x0001 => Self::Manual,

            other => Self::Other(other),
        }
    }
}
impl From<&RecordingMode> for u16 {
    fn from(mode: &RecordingMode) -> Self {
        match mode {
            RecordingMode::Automatic => 0x0000,
            RecordingMode::Manual => 0x0001,

            RecordingMode::Other(b) => *b,
        }
    }
}
impl From<RecordingMode> for u16 {
    fn from(mode: RecordingMode) -> Self {
        (&mode).into()
    }
}
impl PartialEq for RecordingMode {
    fn eq(&self, other: &Self) -> bool {
        let self_u16: u16 = self.into();
        let other_u16: u16 = other.into();
        self_u16 == other_u16
    }
}
impl Eq for RecordingMode {
}
impl PartialOrd for RecordingMode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_u16: u16 = self.into();
        let other_u16: u16 = other.into();
        Some(self_u16.cmp(&other_u16))
    }
}
impl Ord for RecordingMode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

pub fn send_to_oximeter(device: &HidDevice, data: &[u8]) -> HidResult<usize> {
    let mut outgoing_data = Vec::with_capacity(64);

    // prefix with 0x00 report ID
    outgoing_data.push(0x00);

    outgoing_data.extend_from_slice(data);

    // pad out with zeroes
    while outgoing_data.len() < 64 {
        outgoing_data.push(0x00);
    }

    device.write(&outgoing_data)
}

pub fn receive_from_oximeter(device: &HidDevice, queue: &mut CommandQueue) -> HidResult<()> {
    let mut incoming_data = vec![0; 64];
    let bytes_read = device.read(&mut incoming_data)?;
    incoming_data.truncate(bytes_read);

    queue.add_from_buffer(&incoming_data);

    Ok(())
}

/// Returns the index of the first byte in the slice where a new command starts. If no such byte is
/// found, `None` is returned.
pub fn index_of_command_start(bytes: &[u8]) -> Option<usize> {
    for (i, b) in bytes.iter().enumerate() {
        if *b & 0x80 != 0x00 {
            return Some(i);
        }
    }
    None
}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CommandQueue {
    queue: VecDeque<Vec<u8>>,
    holder: Vec<u8>,
}
impl CommandQueue {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            holder: Vec::new(),
        }
    }

    /// Enqueues commands from the received byte buffer.
    pub fn add_from_buffer(&mut self, bytes: &[u8]) {
        // the buffer may:
        // (1) start midway through a command
        // (2) end midway through a command
        // (3) contain trailing 0x00 bytes

        // try taking from the front first
        match index_of_command_start(bytes) {
            None => {
                // (1) is apparently true; (2) might be as well (check that later)
                // append the bytes to the holder
                self.holder.extend_from_slice(&bytes);
            },
            Some(csi) => {
                // append everything until this index to the holder
                self.holder.extend_from_slice(&bytes[0..csi]);

                if self.holder.len() > 0 {
                    // the holder now contains a full command; shunt it to the queue
                    // (the command might be invalid checksum-wise, but it's better to forward it to the
                    // user than to silently drop it)
                    self.queue.push_back(self.holder.clone());
                    self.holder.clear();
                }

                // is there another command?
                let mut current_csi = csi;
                while let Some(next_csi_offset) = index_of_command_start(&bytes[current_csi+1..]) {
                    // yes
                    let next_csi = current_csi + 1 + next_csi_offset;
                    self.queue.push_back(Vec::from(&bytes[current_csi..next_csi]));
                    current_csi = next_csi;
                }

                // place the rest into the holder
                self.holder.extend_from_slice(&bytes[current_csi..]);
            },
        }

        // try shaving off the zeroes in the holder
        let prev_holder_len = self.holder.len();
        while self.holder.last().map(|l| *l == 0x00).unwrap_or(false) {
            self.holder.truncate(self.holder.len() - 1);
        }

        if self.holder.len() == 0 {
            // the holder is empty; all commands are enqueued
            // everything is coming up daisies
            return;
        };
        if is_checksum_ok(&self.holder) {
            // that's a finished command; enqueue it as well!
            self.queue.push_back(self.holder.clone());
            self.holder.clear();
        } else {
            // oops, despite the trailing zeroes, that was not a complete command

            // maybe we were just unlucky and the checksum was a zero byte
            // try that
            self.holder.push(0x00);

            if is_checksum_ok(&self.holder) {
                // well that's a stone off my chest
                self.queue.push_back(self.holder.clone());
                self.holder.clear();
            } else {
                // put those zeroes back
                self.holder.resize(prev_holder_len, 0);
            }
        }
    }

    /// Attempts to dequeue and return a command.
    pub fn dequeue_command(&mut self) -> Option<Vec<u8>> {
        self.queue.pop_front()
    }
}
