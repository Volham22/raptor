use crate::{server::ConnectionStream, utils};

use super::{
    errors::{FrameError, FrameResult},
    Frame,
};

const PRIORITY_FRAME_SIZE: usize = 5;

/// Server does not support priority frame yet.
/// This function is only here to throw an error if an invalid frame has been received.
pub(crate) async fn receive_priority_frame(
    stream: &mut ConnectionStream,
    frame: &Frame,
) -> FrameResult<()> {
    if frame.length != PRIORITY_FRAME_SIZE as u32 {
        return Err(FrameError::BadPriorityFrameSize(frame.length));
    }

    if frame.stream_id == 0 {
        return Err(FrameError::PriorityFrameStreamZero);
    }

    let _ = utils::receive_n_bytes(stream, PRIORITY_FRAME_SIZE)
        .await
        .map_err(FrameError::IOError)?;

    Ok(())
}
