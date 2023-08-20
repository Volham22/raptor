use bytes::BytesMut;
use thiserror::Error;

pub const FRAME_HEADER_LENGTH: usize = 9;

#[derive(Error, Debug)]
pub enum FrameError {
    #[error("Not enought bytes to parse a frame (len < 9)")]
    BadFrameSize,
    #[error("Unknown frame number `{0}`")]
    UnknownFrameNumber(u8),
    #[error("HPACK decoder error: '{0:?}'")]
    HpackDecoderError(hpack::decoder::DecoderError),
    #[error("Continuation frame without header frame")]
    ContinuationWithoutHeader,
    #[error("Continuation frame but END_HEADERS set")]
    ContinuationWithEndHeadersSet,
}

#[repr(u8)]
#[derive(Debug)]
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

impl TryFrom<&BytesMut> for Frame {
    type Error = FrameError;

    fn try_from(bytes: &BytesMut) -> Result<Self, Self::Error> {
        if bytes.len() < FRAME_HEADER_LENGTH {
            return Err(FrameError::BadFrameSize);
        }

        let length = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        let frame_type = u8::from_be(bytes[3]);
        let flags = u8::from_be(bytes[4]);
        let stream_identifier =
            u32::from_be_bytes(<[u8; 4]>::try_from(&bytes[5..=8]).expect("unreachable"));

        Ok(Self {
            length,
            frame_type: FrameType::try_from(frame_type)?,
            flags,
            stream_identifier,
        })
    }
}
