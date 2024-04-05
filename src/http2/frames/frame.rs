use thiserror::Error;

pub const FRAME_HEADER_LENGTH: usize = 9;
pub const MAX_FRAME_SIZE: u32 = (1 << 24) - 1;
pub const MIN_FRAME_SIZE: u32 = 1 << 14;

#[derive(Error, Debug)]
pub enum FrameError {
    #[error("Not enough bytes to parse a frame ({0} < 9)")]
    BadFrameSize(usize),
    #[error("Unknown frame number `{0}`")]
    UnknownFrameNumber(u8),
    #[error("HPACK decoder error: '{0:?}'")]
    HpackDecoderError(fluke_hpack::decoder::DecoderError),
    #[error("Settings frame size it not a multiple of 6")]
    SettingsFrameSize(usize),
    #[error("Window update that is greater than 2^31-1")]
    WindowUpdateTooBig,
    #[error("Frame is too big ({actual} > {max_frame_size})")]
    FrameTooBig { actual: u32, max_frame_size: u32 },
    // #[error("Continuation frame without header frame")]
    // ContinuationWithoutHeader,
    // #[error("Continuation frame but END_HEADERS set")]
    // ContinuationWithEndHeadersSet,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum FrameType {
    Data = 0,
    Headers = 1,
    Priority = 2,
    ResetStream = 3,
    Settings = 4,
    PushPromise = 5,
    Ping = 6,
    GoAway = 7,
    WindowUpdate = 8,
    Continuation = 9,
}

impl TryFrom<u8> for FrameType {
    type Error = FrameError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(FrameType::Data),
            1 => Ok(FrameType::Headers),
            2 => Ok(FrameType::Priority),
            3 => Ok(FrameType::ResetStream),
            4 => Ok(FrameType::Settings),
            5 => Ok(FrameType::PushPromise),
            6 => Ok(FrameType::Ping),
            7 => Ok(FrameType::GoAway),
            8 => Ok(FrameType::WindowUpdate),
            9 => Ok(FrameType::Continuation),
            _ => Err(FrameError::UnknownFrameNumber(value)),
        }
    }
}

#[derive(Debug)]
pub struct Frame {
    pub length: u32,
    pub frame_type: FrameType,
    pub flags: u8,
    pub stream_identifier: u32,
}

impl Frame {
    pub fn try_from_bytes(bytes: &[u8], max_frame_size: u32) -> Result<Self, FrameError> {
        if bytes.len() < FRAME_HEADER_LENGTH {
            return Err(FrameError::BadFrameSize(bytes.len()));
        }

        let length = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        if length >= max_frame_size {
            return Err(FrameError::FrameTooBig {
                actual: length,
                max_frame_size,
            });
        }

        let frame_type = u8::from_be(bytes[3]);
        let flags = u8::from_be(bytes[4]);
        // Ignore the reserved bit value
        let stream_identifier = u32::from_be_bytes([bytes[5] & 0x08, bytes[6], bytes[7], bytes[8]]);

        Ok(Self {
            length,
            frame_type: FrameType::try_from(frame_type)?,
            flags,
            stream_identifier,
        })
    }
}
