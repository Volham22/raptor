use raptor_core::response;

use super::{errors::FrameResult, SerializeFrame};

pub(crate) struct Data {
    pub flags: u8,
    pub data: Vec<u8>,
}

impl Data {
    // pub fn set_end_stream(&mut self, end_stream: bool) {
    //     if end_stream {
    //         self.flags |= 0x01;
    //     } else {
    //         self.flags &= !0x01;
    //     }
    // }

    pub fn from_bytes(_bytes: &[u8]) -> FrameResult<Self> {
        todo!("Parse data frame");
    }
}

impl From<response::Response> for Data {
    fn from(value: response::Response) -> Self {
        assert!(value.body.is_some());

        Self {
            flags: 0,
            data: value.body.unwrap().to_vec(),
        }
    }
}

impl SerializeFrame for Data {
    async fn serialize_frame(
        self,
        frame: &mut super::Frame,
        _encoder: Option<std::sync::Arc<tokio::sync::Mutex<fluke_hpack::Encoder<'_>>>>,
    ) -> Vec<u8> {
        frame.length = self.data.len() as u32;
        frame.flags = self.flags;
        self.data
    }
}
