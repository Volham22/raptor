use std::io;

use thiserror::Error;

pub(crate) type FrameResult<T> = Result<T, FrameError>;

#[derive(Error, Debug)]
pub(crate) enum FrameError {
    #[error("Unknown frame number: {0}")]
    UnknownFrame(u8),
    #[error("Not enough data: {0}")]
    NotEnoughData(usize),
    #[error("Reserved header bit set")]
    ReservedBitSet,
    #[error("Unknown setting identifier: {0}")]
    UnknownSettingIdentifier(u16),
    #[error("Setting received on non-zero stream")]
    SettingNotStreamZero,
    #[error("Bad settings length: {0}")]
    BadSettingsLength(u32),
    #[error("Bad priority frame size: {0}")]
    BadPriorityFrameSize(u32),
    #[error("Priority frame received on stream zero")]
    PriorityFrameStreamZero,
    #[error("HPACK decode error")]
    HpackDecodeError(fluke_hpack::decoder::DecoderError),
    #[error("Bad length for window update: {0}")]
    BadLengthWindowUpdate(u32),
    #[error("Max window update value: {0}")]
    WindowUpdateTooBig(u32),
    #[error("Continuation on stream 0")]
    ContinuationStreamZero,
    #[error("Contination frame without header")]
    ContinuationWithoutHeader,
    #[error("IO error: {0:?}")]
    IOError(io::Error),
}
