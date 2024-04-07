use self::errors::FrameError;

/// 4.1 Frame format
/// All frames begin with a fixed 9-octet header followed by a variable-length frame payload.
pub(crate) const FRAME_HEADER_SIZE: usize = 9;

pub(crate) mod errors;
pub(crate) mod headers;
pub(crate) mod priority;
pub(crate) mod settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum FrameType {
    Data = 0,
    Header = 1,
    Priority = 2,
    ResetStream = 3,
    Settings = 4,
    PushPromise = 5,
    Ping = 6,
    GoAway = 7,
    WindowUpdate = 8,
    Continuation = 9,
}

impl Default for FrameType {
    fn default() -> Self {
        Self::Data
    }
}

impl TryFrom<u8> for FrameType {
    type Error = FrameError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Data),
            1 => Ok(Self::Header),
            2 => Ok(Self::Priority),
            3 => Ok(Self::ResetStream),
            4 => Ok(Self::Settings),
            5 => Ok(Self::PushPromise),
            6 => Ok(Self::Ping),
            7 => Ok(Self::GoAway),
            8 => Ok(Self::WindowUpdate),
            9 => Ok(Self::Continuation),
            _ => Err(FrameError::UnknownFrame(value)),
        }
    }
}

/// Represent a frame header
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Frame {
    pub length: u32,
    pub frame_type: FrameType,
    pub flags: u8,
    pub stream_id: u32,
}

impl TryFrom<&[u8]> for Frame {
    type Error = FrameError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < FRAME_HEADER_SIZE {
            return Err(FrameError::NotEnoughData(bytes.len()));
        }

        let length = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
        let frame_type = FrameType::try_from(u8::from_be(bytes[3]))?;
        let flags = u8::from_be(bytes[4]);
        let stream_identifier_bytes = &bytes[5..9];

        // Check if reserved bit is set
        if stream_identifier_bytes[0] & 0x08 > 0 {
            return Err(FrameError::ReservedBitSet);
        }

        let stream_id = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);

        Ok(Self {
            length,
            frame_type,
            flags,
            stream_id,
        })
    }
}

pub trait SerializeFrame {
    fn serialize_frame(&self, frame: &mut Frame) -> Vec<u8>;
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::frames::FrameType;

    use super::Frame;

    #[rstest]
    #[case::parse_settings(
        include_bytes!("../../tests/data/settings_frame1.raw"),
        Frame {
            length: 18,
            frame_type: FrameType::Settings,
            flags: 0,
            stream_id: 0,
        }
    )]
    #[case::parse_header(
        include_bytes!("../../tests/data/header_frame1.raw"),
        Frame {
            length: 196,
            frame_type: FrameType::Header,
            flags: 0x04,
            stream_id: 1,
        }
    )]
    #[case::parse_window_update(
        include_bytes!("../../tests/data/window_update_frame1.raw"),
        Frame {
            length: 4,
            frame_type: FrameType::WindowUpdate,
            flags: 0,
            stream_id: 0,
        }
    )]
    fn test_frame_header_parsing(#[case] bytes: &[u8], #[case] expected_frame: Frame) {
        let frame = Frame::try_from(bytes).expect("Failed to parse message");
        assert_eq!(frame, expected_frame);
    }
}
