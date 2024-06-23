use std::{
    io,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use raptor_core::config;
use tokio::{io::AsyncReadExt, net::TcpStream, select, sync::Mutex};
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, instrument, trace};

use crate::{
    frames::{
        self,
        errors::{FrameError, FrameResult},
        settings::SettingType,
        window_update::DEFAULT_WINDOW_SIZE,
        Frame, FrameType, FRAME_HEADER_SIZE,
    },
    streams::{StreamFrame, StreamManager},
    utils,
};

const CONNECTION_PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub(crate) type ConnectionStream = TlsStream<TcpStream>;

struct ConnectionData {
    pub stream: ConnectionStream,
    pub global_control_flow: Arc<AtomicU32>,
    pub initial_window_size: i64,
    pub stream_manager: StreamManager,
    pub hpack_encoder: Arc<Mutex<fluke_hpack::Encoder<'static>>>,
    pub hpack_decoder: Arc<Mutex<fluke_hpack::Decoder<'static>>>,
}

impl ConnectionData {
    pub fn set_initial_window_size(&mut self, new_size: u32) {
        self.stream_manager.set_initial_window_size(new_size);
        self.initial_window_size = new_size as i64;
    }
}

async fn handle_connection_preface(stream: &mut ConnectionStream) -> io::Result<bool> {
    let preface_buffer = utils::receive_n_bytes(stream, CONNECTION_PREFACE.len()).await?;

    debug!("Receiver prefaced from client: {preface_buffer:?}");
    Ok(preface_buffer == CONNECTION_PREFACE)
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

async fn handle_frame(
    conf: &Arc<config::Config>,
    frame: &Frame,
    connection_data: &mut ConnectionData,
) -> FrameResult<()> {
    match frame.frame_type {
        FrameType::Settings => {
            let setting_frame =
                frames::settings::Settings::receive_from_frame(frame, &mut connection_data.stream)
                    .await?;
            debug!("Setting frame: {setting_frame:?}");

            if let Some((_, window_value)) = setting_frame
                .settings
                .into_iter()
                .find(|i| i.0 == SettingType::InitialWindowSize)
            {
                if window_value as i64 != connection_data.initial_window_size {
                    let new_stream_value =
                        window_value as i64 - connection_data.initial_window_size;
                    debug!("Set stream flow control to {new_stream_value}");
                    connection_data
                        .stream_manager
                        .broadcast_streams(Arc::new(StreamFrame::SetWindow(new_stream_value)))
                        .await;

                    connection_data.set_initial_window_size(window_value);
                }
            }

            if !setting_frame.is_ack {
                trace!("Sending ack");
                send_setting_ack(&mut connection_data.stream)
                    .await
                    .map_err(FrameError::IOError)
            } else {
                trace!("Setting acknowledged by the client");
                Ok(())
            }
        }
        FrameType::Priority => {
            trace!("Received priority frame. Server does not support this feature. Skipping.");
            frames::priority::receive_priority_frame(&mut connection_data.stream, frame).await
        }
        FrameType::PushPromise => {
            connection_data
                .stream_manager
                .register_new_stream_if_needed(
                    frame.stream_id,
                    conf.clone(),
                    connection_data.hpack_encoder.clone(),
                    connection_data.hpack_decoder.clone(),
                    connection_data.global_control_flow.clone(),
                );
            connection_data
                .stream_manager
                .send_frame_to_stream(Arc::new(StreamFrame::PushPromise), frame.stream_id)
                .await;

            Ok(())
        }
        FrameType::Header => {
            connection_data
                .stream_manager
                .register_new_stream_if_needed(
                    frame.stream_id,
                    conf.clone(),
                    connection_data.hpack_encoder.clone(),
                    connection_data.hpack_decoder.clone(),
                    connection_data.global_control_flow.clone(),
                );
            let headers_frame = frames::headers::Headers::receive_header_frame(
                &mut connection_data.stream,
                frame,
                &connection_data.hpack_decoder,
            )
            .await?;

            connection_data
                .stream_manager
                .send_frame_to_stream(
                    Arc::new(StreamFrame::Header(headers_frame)),
                    frame.stream_id,
                )
                .await;
            Ok(())
        }
        FrameType::WindowUpdate => {
            let window_update = frames::window_update::WindowUpdate::receive_window_update(
                &mut connection_data.stream,
                frame,
            )
            .await?;

            // This is stream specific. Let's notify the concerned stream
            if window_update.stream != 0 {
                connection_data
                    .stream_manager
                    .send_frame_to_stream(
                        Arc::new(StreamFrame::WindowUpdate(window_update)),
                        frame.stream_id,
                    )
                    .await;
            } else {
                connection_data
                    .global_control_flow
                    .fetch_add(window_update.size_increment, Ordering::Relaxed);
            }

            Ok(())
        }
        FrameType::Continuation => {
            let continuation = frames::continuation::Continuation::receive_continuation(
                &mut connection_data.stream,
                frame,
            )
            .await?;

            connection_data
                .stream_manager
                .send_frame_to_stream(
                    Arc::new(StreamFrame::Continuation(continuation)),
                    frame.stream_id,
                )
                .await;

            Ok(())
        }
        _ => todo!(),
    }
}

async fn do_connection_loop(
    stream: ConnectionStream,
    conf: &Arc<config::Config>,
) -> FrameResult<()> {
    let mut connection_data = ConnectionData {
        stream,
        global_control_flow: Arc::new(AtomicU32::new(DEFAULT_WINDOW_SIZE)),
        hpack_encoder: Arc::new(Mutex::new(fluke_hpack::Encoder::new())),
        hpack_decoder: Arc::new(Mutex::new(fluke_hpack::Decoder::new())),
        stream_manager: StreamManager::default(),
        initial_window_size: DEFAULT_WINDOW_SIZE as i64,
    };

    loop {
        // A connection thread can receive two kind of things:
        // 1. Some data to send from a stream
        // 2. A new frame to handle
        trace!("Waiting for data from streams or peer");
        select! {
            data = connection_data.stream_manager.poll_data() => {
                trace!("Stream data");
                let Some(payload_bytes) = data.as_ref() else {
                    error!("Failed to retrieve data to send from stream!");
                    break Ok(());
                };

                utils::write_all_buffer(&mut connection_data.stream, payload_bytes)
                    .await
                    .map_err(FrameError::IOError)?;
            },
            frame = receive_frame_header(&mut connection_data.stream) => {
                let Ok(frame) = frame else {
                    return Err(frame.unwrap_err());
                };
                debug!("Frame header received: {frame:?}");

                handle_frame(
                    conf,
                    &frame,
                    &mut connection_data,
                )
                .await?;

            },
            else => {
                error!("Select has been cancelled. Is the connection closed?");
                break Ok(());
            },
        };
    }
}

/// Run an HTTP/2 client connection. At this point the connection has already
/// been accepted and http/2 has been negociated with the TLS ALPN extension
#[instrument(name = "http2_connection")]
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
