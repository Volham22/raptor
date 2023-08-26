use std::{fmt::Display, mem};

use tracing::warn;

use crate::{connection::ConnectionError, http2::response::ResponseSerialize};

use super::{FrameError, FRAME_HEADER_LENGTH};

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum ErrorType {
    NoError = 0,
    ProtocolError = 1,
    InternalError = 2,
    FlowControlError = 3,
    SettingsTimeout = 4,
    StreamClosed = 5,
    FrameSizeError = 6,
    RefusedStream = 7,
    Cancel = 8,
    CompressionError = 9,
    ConnectError = 10,
    EnhanceYourCalm = 11,
    IndequateSecurity = 12,
    Http11Required = 13,
}

impl Display for ErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoError => write!(f, "NoError"),
            Self::ProtocolError => write!(f, "ProtocolError"),
            Self::InternalError => write!(f, "InternalError"),
            Self::FlowControlError => write!(f, "FlowControlError"),
            Self::SettingsTimeout => write!(f, "SettingsTimeout"),
            Self::StreamClosed => write!(f, "StreamClosed"),
            Self::FrameSizeError => write!(f, "FrameSizeError"),
            Self::RefusedStream => write!(f, "RefusedStream"),
            Self::Cancel => write!(f, "Cancel"),
            Self::CompressionError => write!(f, "CompressionError"),
            Self::ConnectError => write!(f, "ConnectError"),
            Self::EnhanceYourCalm => write!(f, "EnhanceYourCalm"),
            Self::IndequateSecurity => write!(f, "IndequateSecurity"),
            Self::Http11Required => write!(f, "Http11Required"),
        }
    }
}

impl From<ConnectionError> for ErrorType {
    fn from(value: ConnectionError) -> ErrorType {
        match value {
            ConnectionError::WindowUpdateTooBig => ErrorType::FlowControlError,
            ConnectionError::IOError(_) => unreachable!(),
            ConnectionError::NonZeroSettingsAckLength
            | ConnectionError::BadLengthWindowUpdate(_)
            | ConnectionError::BadPingFrameSize
            | ConnectionError::FrameTooBig{..}
            | ConnectionError::SettingsLengthNotMultipleOf6 => ErrorType::FrameSizeError,
            _ => ErrorType::ProtocolError,
        }
    }
}

impl From<ErrorType> for u32 {
    fn from(value: ErrorType) -> Self {
        value as u32
    }
}

impl From<u32> for ErrorType {
    fn from(value: u32) -> Self {
        match value {
            0 => ErrorType::NoError,
            1 => ErrorType::ProtocolError,
            2 => ErrorType::InternalError,
            3 => ErrorType::FlowControlError,
            4 => ErrorType::SettingsTimeout,
            5 => ErrorType::StreamClosed,
            6 => ErrorType::FrameSizeError,
            7 => ErrorType::RefusedStream,
            8 => ErrorType::Cancel,
            9 => ErrorType::CompressionError,
            10 => ErrorType::ConnectError,
            11 => ErrorType::EnhanceYourCalm,
            12 => ErrorType::IndequateSecurity,
            13 => ErrorType::Http11Required,
            _ => {
                warn!(
                    "Unknown error code {}, treating it as INTERNAL_ERROR.",
                    value
                );
                ErrorType::InternalError
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GoAway {
    kind: ErrorType,
    last_stream_id: u32,
    pub additionnal_data: Vec<u8>,
}

impl GoAway {
    pub fn new(kind: ErrorType, last_stream_id: u32, additionnal_data: Vec<u8>) -> Self {
        Self {
            kind,
            last_stream_id,
            additionnal_data,
        }
    }

    pub fn from_bytes(data: &[u8], length: usize) -> Result<Self, FrameError> {
        if data.len() < length {
            return Err(FrameError::BadFrameSize(data.len()));
        }

        let last_stream_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let kind = ErrorType::from(u32::from_be_bytes([data[4], data[5], data[6], data[7]]));
        let additionnal_data = data[8..length].to_vec();

        Ok(Self {
            kind,
            last_stream_id,
            additionnal_data,
        })
    }

    pub fn is_error(&self) -> bool {
        !matches!(self.kind, ErrorType::NoError)
    }
}

impl ResponseSerialize for GoAway {
    fn serialize_response(&self, _: Option<&mut hpack::Encoder>) -> Vec<u8> {
        let mut result = Vec::with_capacity(
            FRAME_HEADER_LENGTH + 2 * mem::size_of::<u32>() + self.additionnal_data.len(),
        );

        result.extend(self.last_stream_id.to_be_bytes());
        result.extend((self.kind as u32).to_be_bytes());
        result.extend_from_slice(&self.additionnal_data);

        result
    }

    fn compute_frame_length(&self, _: Option<&mut hpack::Encoder>) -> u32 {
        (FRAME_HEADER_LENGTH + 2 * mem::size_of::<u32>() + self.additionnal_data.len()) as u32
    }
}
