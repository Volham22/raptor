use tokio::io::AsyncReadExt;

use crate::server::ConnectionStream;

use super::{
    errors::{FrameError, FrameResult},
    Frame,
};

pub(crate) const DEFAULT_WINDOW_SIZE: u32 = (1 << 16) - 1; // 2**16 - 1
const WINDOW_UPDATE_LENGTH: u32 = 4;
const MAX_WINDOW_FRAME_ADVERTISEMENT: u32 = (1 << 31) - 1; // 2**31 - 1

pub(crate) struct WindowUpdate {
    pub stream: u32,
    pub size_increment: u32,
}

impl WindowUpdate {
    pub async fn receive_window_update(
        stream: &mut ConnectionStream,
        frame: &Frame,
    ) -> FrameResult<Self> {
        let mut buffer: [u8; 4] = [0; 4];
        stream
            .read(&mut buffer)
            .await
            .map_err(FrameError::IOError)?;

        Self::parse_frame(frame, &buffer)
    }

    pub(self) fn parse_frame(frame: &Frame, buffer: &[u8]) -> FrameResult<Self> {
        assert_eq!(buffer.len(), 4);

        if frame.length != WINDOW_UPDATE_LENGTH {
            return Err(FrameError::BadLengthWindowUpdate(frame.length));
        }

        if frame.length > MAX_WINDOW_FRAME_ADVERTISEMENT {
            return Err(FrameError::WindowUpdateTooBig(frame.length));
        }

        Ok(Self {
            stream: frame.stream_id,
            size_increment: u32::from_be_bytes(
                TryFrom::<&[u8]>::try_from(buffer).expect("unreachable"),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::frames::{Frame, FrameType, FRAME_HEADER_SIZE};

    use super::WindowUpdate;

    #[test]
    fn parse_good_frame() {
        const WINDOW_UPDATE_FRAME: &[u8; 13] =
            include_bytes!("../../tests/data/window_update_frame1.raw");

        let result = WindowUpdate::parse_frame(
            &Frame {
                length: 4,
                frame_type: FrameType::WindowUpdate,
                ..Default::default()
            },
            &WINDOW_UPDATE_FRAME[FRAME_HEADER_SIZE..],
        );

        assert!(result.is_ok());
        let window_update = result.unwrap();
        assert_eq!(window_update.stream, 0);
        assert_eq!(window_update.size_increment, 1073676289);
    }
}
