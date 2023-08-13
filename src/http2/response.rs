use bytes::{BufMut, BytesMut};

use super::frames::FrameType;

pub fn build_frame_header<T: ResponseSerialize>(
    buffer: &mut BytesMut,
    frame_type: FrameType,
    flags: u8,
    stream_identifer: u32,
    frame: &T,
) {
    buffer.put(&frame.compute_frame_length().to_be_bytes()[..3]); // length

    buffer.put_u8((frame_type as u8).to_be()); // type
    buffer.put_u8(flags.to_be()); // flags

    buffer.put_u32(stream_identifer.to_be()); // stream identifier
    frame.serialize_response(buffer);
}

pub trait ResponseSerialize {
    fn serialize_response(&self, buffer: &mut BytesMut);
    fn compute_frame_length(&self) -> u32;
}
