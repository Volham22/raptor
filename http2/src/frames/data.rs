use raptor_core::response;

use super::SerializeFrame;

pub(crate) struct Data {
    pub flags: u8,
    pub data: Vec<u8>,
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
