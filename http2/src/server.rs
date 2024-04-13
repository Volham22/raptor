use std::{
    io,
    sync::{atomic::{AtomicU32, Ordering}, Arc},
};

use raptor_core::config;
use tokio::{io::AsyncReadExt, net::TcpStream, select, sync::Mutex};
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, instrument, trace};

use crate::{
    frames::{
        self,
        errors::{FrameError, FrameResult},
        window_update::DEFAULT_WINDOW_SIZE,
        Frame, FrameType, FRAME_HEADER_SIZE,
    },
    streams::{StreamFrame, StreamManager},
    utils,
};

const CONNECTION_PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub(crate) type ConnectionStream = TlsStream<TcpStream>;

async fn handle_connection_preface(stream: &mut ConnectionStream) -> io::Result<bool> {
    let mut preface_buffer: [u8; 24] = [0; 24];
    let mut received_size = 0usize;

    while received_size < CONNECTION_PREFACE.len() {
        received_size += stream.read(&mut preface_buffer).await?;
    }

    debug!("Receiver prefaced from client: {preface_buffer:?}");
    Ok(preface_buffer == *CONNECTION_PREFACE)
}

async fn send_server_settings(stream: &mut ConnectionStream) -> io::Result<()> {
    trace!("Send server settings");
    let mut frame = frames::Frame {
        length: 0,
        stream_id: 0,
        frame_type: frames::FrameType::Settings,
        flags: 0,
    };

    let settings = frames::settings::Settings::server_settings();
    utils::send_frame(stream, &mut frame, None, settings).await
}

async fn send_setting_ack(stream: &mut ConnectionStream) -> io::Result<()> {
    let mut frame = frames::Frame {
        frame_type: frames::FrameType::Settings,
        ..Default::default()
    };
    let ack = frames::settings::Settings::setting_ack();

    utils::send_frame(stream, &mut frame, None, ack).await
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

async fn handle_frame<'hpack>(
    conf: &Arc<config::Config>,
    frame: &Frame,
    global_control_flow: Arc<AtomicU32>,
    stream: &mut ConnectionStream,
    stream_manager: &mut StreamManager,
    hpack_decoder: &mut fluke_hpack::Decoder<'_>,
    hpack_encoder: Arc<Mutex<fluke_hpack::Encoder<'static>>>,
) -> FrameResult<()> {
    match frame.frame_type {
        FrameType::Settings => {
            let setting_frame =
                frames::settings::Settings::receive_from_frame(frame, stream).await?;
            debug!("Setting frame: {setting_frame:?}");

            if !setting_frame.is_ack {
                // TODO: Handle settings
                send_setting_ack(stream).await.map_err(FrameError::IOError)
            } else {
                trace!("Setting acknowledged by the client");
                Ok(())
            }
        }
        FrameType::Priority => {
            trace!("Received priority frame. Server does not support this feature. Skipping.");
            frames::priority::receive_priority_frame(stream, frame).await
        }
        FrameType::PushPromise => {
            stream_manager.register_new_stream_if_needed(
                frame.stream_id,
                conf.clone(),
                hpack_encoder.clone(),
                global_control_flow.clone(),
            );
            stream_manager
                .send_frame_to_stream(Arc::new(StreamFrame::PushPromise), frame.stream_id)
                .await;

            Ok(())
        }
        FrameType::Header => {
            stream_manager.register_new_stream_if_needed(
                frame.stream_id,
                conf.clone(),
                hpack_encoder.clone(),
                global_control_flow.clone(),
            );
            let headers_frame =
                frames::headers::Headers::receive_header_frame(stream, frame, hpack_decoder)
                    .await?;

            stream_manager
                .send_frame_to_stream(
                    Arc::new(StreamFrame::Header(headers_frame)),
                    frame.stream_id,
                )
                .await;
            Ok(())
        }
        FrameType::WindowUpdate => {
            let window_update =
                frames::window_update::WindowUpdate::receive_window_update(stream, frame).await?;

            // This is stream specific. Let's notify the concerned stream
            if window_update.stream != 0 {
                stream_manager
                    .send_frame_to_stream(
                        Arc::new(StreamFrame::WindowUpdate(window_update)),
                        frame.stream_id,
                    )
                    .await;
            } else {
                global_control_flow.fetch_add(window_update.size_increment, Ordering::Relaxed);
            }

            Ok(())
        }
        _ => todo!(),
    }
}

#[instrument]
async fn do_connection_loop(
    mut stream: ConnectionStream,
    conf: &Arc<config::Config>,
) -> FrameResult<()> {
    let mut stream_manager = StreamManager::default();
    let mut hpack_decoder = fluke_hpack::Decoder::new();
    let hpack_encoder = fluke_hpack::Encoder::new();
    let hpack_encoder = Arc::new(Mutex::new(hpack_encoder));
    let global_flow_control = Arc::new(AtomicU32::new(DEFAULT_WINDOW_SIZE));

    loop {
        // A connection thread can receive two kind of things:
        // 1. Some data to send from a stream
        // 2. A new frame to handle
        select! {
            data = stream_manager.poll_data() => {
                let Some(payload_bytes) = data.as_ref() else {
                    error!("Failed to retrieve data to send from stream!");
                    break Ok(());
                };

                trace!("Received data from stream. Sending");
                utils::write_all_buffer(&mut stream, payload_bytes)
                    .await
                    .map_err(FrameError::IOError)?;
            },
            frame = receive_frame_header(&mut stream) => {
                let Ok(frame) = frame else {
                    return Err(frame.unwrap_err());
                };

                handle_frame(
                    conf,
                    &frame,
                    global_flow_control.clone(),
                    &mut stream,
                    &mut stream_manager,
                    &mut hpack_decoder,
                    hpack_encoder.clone(),
                )
                .await?;

                debug!("Frame header received: {frame:?}");
            },
        };
    }
}

/// Run an HTTP/2 client connection. At this point the connection has already
/// been accepted and http/2 has been negociated with the TLS ALPN extension
#[instrument]
pub async fn run_connection(
    mut stream: ConnectionStream,
    config: Arc<config::Config>,
) -> io::Result<()> {
    if !handle_connection_preface(&mut stream).await? {
        error!("Invalid connection preface received");
        return Ok(());
    }

    trace!("Received connection preface. Starting connection handling");
    send_server_settings(&mut stream).await?;

    match do_connection_loop(stream, &config).await {
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
