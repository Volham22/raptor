use crate::{server::ConnectionStream, utils};

use super::{
    errors::{FrameError, FrameResult},
    Frame,
};

const PRIORITY_FRAME_SIZE: usize = 5;

fn check_check_priority_frame(frame: &Frame) -> FrameResult<()> {
    if frame.length != PRIORITY_FRAME_SIZE as u32 {
        return Err(FrameError::BadPriorityFrameSize(frame.length));
    }

    if frame.stream_id == 0 {
        return Err(FrameError::PriorityFrameStreamZero);
    }

    Ok(())
}

/// Server does not support priority frame yet.
/// This function is only here to throw an error if an invalid frame has been received.
pub(crate) async fn receive_priority_frame(
    stream: &mut ConnectionStream,
    frame: &Frame,
) -> FrameResult<()> {
    check_check_priority_frame(frame)?;

    let _ = utils::receive_n_bytes(stream, PRIORITY_FRAME_SIZE)
        .await
        .map_err(FrameError::IOError)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::frames::{errors::FrameError, Frame, FrameType};

    use super::{check_check_priority_frame, PRIORITY_FRAME_SIZE};

    #[test]
    fn priority_non_zero_stream() {
        let frame = Frame {
            length: PRIORITY_FRAME_SIZE as u32,
            flags: 0,
            stream_id: 0,
            frame_type: FrameType::Priority,
        };

        let result = check_check_priority_frame(&frame);
        assert!(result.is_err());
        match result.unwrap_err() {
            FrameError::PriorityFrameStreamZero => (),
            _ => panic!("Error is not correct"),
        }
    }

    #[test]
    fn priority_bad_length() {
        let frame = Frame {
            length: 51,
            flags: 0,
            stream_id: 0,
            frame_type: FrameType::Priority,
        };

        let result = check_check_priority_frame(&frame);
        assert!(result.is_err());
        match result.unwrap_err() {
            FrameError::BadPriorityFrameSize(51) => (),
            _ => panic!("Error is not correct"),
        }
    }

    #[test]
    fn priority_correct_frame() {
        let frame = Frame {
            length: PRIORITY_FRAME_SIZE as u32,
            flags: 0,
            stream_id: 2,
            frame_type: FrameType::Priority,
        };

        let result = check_check_priority_frame(&frame);
        assert!(result.is_ok());
    }
}
