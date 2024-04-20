use crate::{server::ConnectionStream, utils};

use super::{
    errors::{FrameError, FrameResult},
    Frame,
};

pub struct Continuation {
    flags: u8,
    pub(crate) data: Vec<u8>,
}

impl Continuation {
    pub async fn receive_continuation(
        stream: &mut ConnectionStream,
        frame: &Frame,
    ) -> FrameResult<Self> {
        if frame.stream_id == 0 {
            return Err(FrameError::ContinuationStreamZero);
        }

        let data = utils::receive_n_bytes(stream, frame.length as usize)
            .await
            .map_err(FrameError::IOError)?;

        Ok(Self {
            flags: frame.flags,
            data,
        })
    }

    #[inline]
    pub fn has_end_headers(&self) -> bool {
        self.flags & 0x04 > 0
    }
}
