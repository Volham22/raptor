use raptor_core::response;

use super::errors::FrameResult;

pub struct Data {
    flags: u8,
    data: Vec<u8>,
}

impl Data {
    pub fn set_end_stream(&mut self, end_stream: bool) {
        if end_stream {
            self.flags |= 0x08;
        } else {
            self.flags &= !0x08;
        }
    }

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
