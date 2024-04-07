use std::{io, sync::Arc};

use tokio::{io::AsyncReadExt, net::TcpStream};
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, instrument, trace};

use crate::{
    frames::{
        self,
        errors::{FrameError, FrameResult},
        Frame, FrameType, FRAME_HEADER_SIZE,
    },
    streams::{StreamFrame, StreamManager},
    utils::write_all_buffer,
};

const CONNECTION_PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub(crate) type ConnectionStream = TlsStream<TcpStream>;

async fn handle_connection_preface(stream: &mut ConnectionStream) -> io::Result<bool> {
    write_all_buffer(stream, CONNECTION_PREFACE).await?;
    let mut preface_buffer: [u8; 24] = [0; 24];
    let mut received_size = 0usize;

    while received_size < CONNECTION_PREFACE.len() {
        received_size += stream.read(&mut preface_buffer).await?;
    }

    debug!("Receiver prefaced from client: {preface_buffer:?}");
    Ok(preface_buffer == *CONNECTION_PREFACE)
}

async fn receive_frame_header(stream: &mut ConnectionStream) -> FrameResult<Frame> {
    let mut frame_header_buffer: [u8; FRAME_HEADER_SIZE] = [0; FRAME_HEADER_SIZE];
    let mut received_bytes = 0usize;

    while received_bytes < FRAME_HEADER_SIZE {
        received_bytes += stream
            .read(&mut frame_header_buffer)
            .await
            .map_err(FrameError::IOError)?;
    }

    Frame::try_from(frame_header_buffer.as_slice())
}

async fn do_connection_loop(mut stream: ConnectionStream) -> FrameResult<()> {
    let mut stream_manager = StreamManager::default();
    let mut hpack_decoder = fluke_hpack::Decoder::new();

    loop {
        let frame = receive_frame_header(&mut stream).await?;
        debug!("Frame header received: {frame:?}");

        match frame.frame_type {
            FrameType::Settings => {
                let setting_frame =
                    frames::settings::Settings::receive_from_frame(&frame, &mut stream).await?;
                debug!("Setting frame: {setting_frame:?}");

                if setting_frame.is_ack {
                    trace!("Setting acknowledged by the server");
                }
            }
            FrameType::Priority => {
                trace!("Received priority frame. Server does not support this feature. Skipping.");
                frames::priority::receive_priority_frame(&mut stream, &frame).await?;
            }
            FrameType::PushPromise => {
                stream_manager.register_new_stream_if_needed(frame.stream_id);
                stream_manager
                    .send_frame_to_stream(Arc::new(StreamFrame::PushPromise), frame.stream_id)
                    .await;
            }
            FrameType::Header => {
                stream_manager.register_new_stream_if_needed(frame.stream_id);
                let headers_frame = frames::headers::Headers::receive_header_frame(
                    &mut stream,
                    &frame,
                    &mut hpack_decoder,
                )
                .await?;

                stream_manager
                    .send_frame_to_stream(
                        Arc::new(StreamFrame::Header(headers_frame)),
                        frame.stream_id,
                    )
                    .await;
            }
            _ => todo!(),
        }
    }
}

/// Run an HTTP/2 client connection. At this point the connection has already
/// been accepted and http/2 has been negociated with the TLS ALPN extension
#[instrument]
pub async fn run_connection(mut stream: ConnectionStream) -> io::Result<()> {
    if !handle_connection_preface(&mut stream).await? {
        error!("Invalid connection preface received");
        return Ok(());
    }

    trace!("Received connection preface. Starting connection handling");
    match do_connection_loop(stream).await {
        Ok(()) => Ok(()),
        Err(FrameError::IOError(e)) => Err(e),
        Err(e) => {
            error!("Frame error: {e}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::server::CONNECTION_PREFACE;

    #[test]
    fn test_connection_magic_correct() {
        const EXPECTED_MAGIC: &[u8; 24] = include_bytes!("../tests/data/connection_magic.raw");
        assert_eq!(CONNECTION_PREFACE, EXPECTED_MAGIC);
    }
}
