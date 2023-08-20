use std::io;

use bytes::{BufMut, BytesMut};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::{debug, info};

use crate::{
    connection::send_all,
    method_handlers::handle_get,
    request::{HttpRequest, RequestType},
};

use super::{
    frames::{Data, FrameType, Headers},
    stream::StreamManager,
};

pub fn build_frame_header<T: ResponseSerialize>(
    buffer: &mut BytesMut,
    frame_type: FrameType,
    stream_identifer: u32,
    frame: &T,
    encoder: Option<&mut hpack::Encoder>,
) {
    // Slice to 1.. because of network endianness
    let payload = frame.serialize_response(encoder);
    buffer.put(&(payload.len() as u32).to_be_bytes()[1..]); // length

    buffer.put_u8((frame_type as u8).to_be()); // type
    buffer.put_u8(frame.get_flags().to_be()); // flags

    buffer.put_u32(stream_identifer); // stream identifier
    buffer.put(payload.as_slice());
}

pub async fn respond_request(
    stream: &mut TlsStream<TcpStream>,
    stream_identifer: u32,
    stream_manager: &mut StreamManager,
    encoder: &mut hpack::Encoder<'_>,
) -> io::Result<()> {
    let headers = stream_manager
        .get_at(stream_identifer)
        .unwrap()
        .get_headers()
        .unwrap();

    match headers.get_type() {
        Ok(kind) => match kind {
            RequestType::Get => {
                let get_payload = handle_get(headers).await?;
                let http_stream = stream_manager.get_at_mut(stream_identifer).unwrap();

                if !http_stream.has_room_in_window(get_payload.len() as u32) {
                    info!(
                        "Stream {} has not enough room to send response payload",
                        http_stream.identifier
                    );
                    todo!("Handle error");
                }

                http_stream
                    .consume_space(get_payload.len() as u32)
                    .expect("Tried to consume space on a too small window");

                let payload_size = get_payload.len().to_string();
                let response_headers = Headers::new(&[
                    (b":status", b"200"),
                    (b"content-type", b"text/plain"),
                    (b"content-length", payload_size.as_bytes()),
                ]);
                debug!(
                    "Response headers: {:?} stream: {}",
                    response_headers, http_stream.identifier
                );

                // Send buffer header
                let mut buffer = BytesMut::new();
                build_frame_header(
                    &mut buffer,
                    FrameType::Headers,
                    http_stream.identifier,
                    &response_headers,
                    Some(encoder),
                );
                send_all(stream, &buffer[..]).await?;

                // Send buffer data
                let mut data_frame = Data::new(get_payload);
                data_frame.set_flags(0x01);
                let mut data_buffer = BytesMut::new();
                build_frame_header(
                    &mut data_buffer,
                    FrameType::Data,
                    http_stream.identifier,
                    &data_frame,
                    None,
                );
                debug!("Response data: {:?}", data_frame);

                http_stream.mark_as_closed();
                send_all(stream, &data_buffer[..]).await
            }
            RequestType::Delete => todo!(),
            RequestType::Put => todo!(),
            RequestType::Head => todo!(),
        },
        Err(_) => todo!(),
    }
}

pub trait ResponseSerialize {
    fn serialize_response(&self, encoder: Option<&mut hpack::Encoder>) -> Vec<u8>;
    fn compute_frame_length(&self, encoder: Option<&mut hpack::Encoder>) -> u32;
    fn get_flags(&self) -> u8 {
        0
    }
}
