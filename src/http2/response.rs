use bytes::{BufMut, BytesMut};

use super::frames;

pub fn build_frame_header<T: ResponseSerialize>(
    buffer: &mut BytesMut,
    frame_type: frames::FrameType,
    stream_identifer: u32,
    frame: &T,
    encoder: Option<&mut fluke_hpack::Encoder>,
) {
    // Slice to 1.. because of network endianness
    let payload = frame.serialize_response(encoder);
    buffer.put(&(payload.len() as u32).to_be_bytes()[1..]); // length

    buffer.put_u8((frame_type as u8).to_be()); // type
    buffer.put_u8(frame.get_flags().to_be()); // flags

    buffer.put_u32(stream_identifer); // stream identifier
    buffer.put(payload.as_slice());
}

pub trait ResponseSerialize {
    fn serialize_response(&self, encoder: Option<&mut fluke_hpack::Encoder>) -> Vec<u8>;
    fn compute_frame_length(&self, encoder: Option<&mut fluke_hpack::Encoder>) -> u32;
    fn get_flags(&self) -> u8 {
        0
    }
}
