use std::{io, sync::Arc};

use bytes::{BufMut, Bytes, BytesMut};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::{debug, info, trace};

use crate::{
    config::Config,
    connection::{connection_error_to_io_error, send_all},
    method_handlers::handle_request,
    request::{HttpRequest, RequestType},
};

use super::{
    frames::{self},
    stream::StreamManager,
};

pub fn build_frame_header<T: ResponseSerialize>(
    buffer: &mut BytesMut,
    frame_type: frames::FrameType,
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
    config: &Arc<Config>,
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
                let mut response = handle_request(headers, config).await;
                let http_stream = stream_manager.get_at_mut(stream_identifer).unwrap();

                let mut serialize_buffer = BytesMut::with_capacity(8192);
                let mut headers_vec = vec![(
                    Bytes::from_static(b":status"),
                    Bytes::copy_from_slice(response.code.to_string().as_bytes()),
                )];
                headers_vec.append(&mut response.headers); // Cheap because of Bytes
                let header_frame = frames::Headers::new(&headers_vec, response.body.is_none());
                build_frame_header(
                    &mut serialize_buffer,
                    frames::FrameType::Headers,
                    stream_identifer,
                    &header_frame,
                    Some(encoder),
                );

                if let Some(body) = response.body.as_ref() {
                    if !http_stream.has_room_in_window(body.len() as u32) {
                        info!(
                            "Stream {} has not enough room to send response payload",
                            http_stream.identifier
                        );
                        todo!("Handle error");
                    }

                    http_stream
                        .consume_space(body.len() as u32)
                        .expect("Tried to consume space on a too small window");
                }

                debug!("Send header frame: {:?}", header_frame);
                connection_error_to_io_error!(
                    send_all(stream, serialize_buffer.as_ref()).await,
                    ()
                )?;

                // // Send buffer data
                if let Some(body) = response.body {
                    let mut data_frame = frames::Data::new(body);
                    data_frame.set_flags(0x01); // END_STREAM

                    serialize_buffer.clear();
                    build_frame_header(
                        &mut serialize_buffer,
                        frames::FrameType::Data,
                        http_stream.identifier,
                        &data_frame,
                        None,
                    );

                    trace!("Send data frame (len: {})", serialize_buffer.len());
                    connection_error_to_io_error!(
                        send_all(stream, serialize_buffer.as_ref()).await,
                        ()
                    )?;
                }

                http_stream.mark_as_closed();
                Ok(())
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
