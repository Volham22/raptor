use bytes::{Buf, BufMut, BytesMut};
use std::{io, sync::Arc};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, info, trace, warn};

use crate::{
    config::Config,
    http2::{
        frames::{self, FrameType, FRAME_HEADER_LENGTH, PING_LENGTH},
        response::{build_frame_header, ResponseSerialize},
        stream,
    },
};

const CONNECTION_PRFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("Missing continuation frame for headers")]
    MissingContinuationFrame,
    #[error("IO error: {0:?}")]
    IOError(io::Error),
    #[error("Invalid header frame: {0:?}")]
    InvalidHeaderFrame(frames::FrameError),
    #[error("Invalid continuation frame: {0:?}")]
    InvalidContinuationFrame(frames::FrameError),
    #[error("Invalid frame")]
    InvalidFrame,
    #[error("Go away received on a stream id other than 0")]
    GoAwayOnNonDefaultStream,
    #[error("Received continuation frame without header frame")]
    ContinuationFrameWithoutHeaderFrame,
    #[error("Settings ACK length is not 0")]
    NonZeroSettingsAckLength,
    #[error("Settings on non default stream")]
    SettingsNonDefaultSream,
    #[error("Settings frame length is not a multiple of 6")]
    SettingsLengthNotMultipleOf6,
    #[error("Ping on non 0 stream")]
    PingOnNon0Stream,
    #[error("Ping frame length is not 8 bytes")]
    BadPingFrameSize,
    #[error("Window update of 0")]
    ZeroWindowUpdate,
    #[error("Window update frame length is incorrect: ({0} != 4)")]
    BadLengthWindowUpdate(u32),
    #[error("Prority frame length is incorrect: ({0} != 5)")]
    BadLengthPriorityFrame(u32),
    #[error("Bad request received")]
    BadRequest,
    #[error("Bad connection preface")]
    BadConnectionPreface,
    #[error("Window update too big")]
    WindowUpdateTooBig,
    #[error("Received frame is too big")]
    FrameTooBig { actual: u32, max_frame_size: u32 },
    #[error("Data frame on stream `0`")]
    DataOnStreamZero,
    #[error("Header frame on stream `0`")]
    HeaderOnStreamZero,
    #[error("Priority frame on stream `0`")]
    PriorityFrameOnStreamZero,
    #[error("Invalid setting value")]
    InvalidSettingValue(u32),
    #[error("Invalid Initial window size: {0:}")]
    BadInitialWindowSize(u32),
}

pub(crate) type ConnectionResult<T> = Result<T, ConnectionError>;

macro_rules! connection_error_to_io_error {
    ($err:expr, $ty:ty) => {
        match $err {
            Ok(_) => Ok::<$ty, ::std::io::Error>(()),
            Err(ConnectionError::IOError(err)) => Err::<$ty, ::std::io::Error>(err),
            Err(_) => unreachable!("should never be other than IOError!"),
        }
    };
}

pub fn check_connection_preface(bytes: &[u8]) -> Option<usize> {
    if bytes.starts_with(CONNECTION_PRFACE) {
        Some(CONNECTION_PRFACE.len())
    } else {
        None
    }
}

pub async fn send_all(stream: &mut TlsStream<TcpStream>, data: &[u8]) -> ConnectionResult<()> {
    stream
        .write_all(data)
        .await
        .map_err(ConnectionError::IOError)?;

    stream.flush().await.map_err(ConnectionError::IOError)
}

async fn send_server_setting(stream: &mut TlsStream<TcpStream>) -> ConnectionResult<()> {
    let default_settings = frames::Settings::default(); // TODO: This could be constant
    debug!("Send server settings: {default_settings:?}");
    let mut buffer = BytesMut::with_capacity(
        FRAME_HEADER_LENGTH + default_settings.compute_frame_length(None) as usize,
    );

    build_frame_header(&mut buffer, FrameType::Settings, 0, &default_settings, None);
    send_all(stream, &buffer[..]).await
}

async fn receive_headers(
    stream: &mut TlsStream<TcpStream>,
    buffer: &mut BytesMut,
    decoder: &mut hpack::Decoder<'_>,
    frame: &frames::Frame,
    max_frame_size: u32,
) -> ConnectionResult<frames::Headers> {
    trace!("Receiving headers");
    match frames::Headers::from_bytes(
        &buffer[FRAME_HEADER_LENGTH..],
        decoder,
        frame.flags,
        frame.length as usize,
    ) {
        Ok(h) => {
            buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
            Ok(h)
        }
        Err(err) => match err {
            frames::FrameError::BadFrameSize(s) => loop {
                trace!("Bad frame size for headers (frame_length: {}, actual: {s}), keep reading again...", frame.length);
                let _ = stream
                    .read_buf(buffer)
                    .await
                    .map_err(ConnectionError::IOError)?;
                let headers = frames::Headers::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    decoder,
                    frame.flags,
                    frame.length as usize,
                );

                match headers {
                    Ok(h) => {
                        buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
                        return Ok(h);
                    }
                    Err(frames::FrameError::BadFrameSize(_)) => continue,
                    Err(err) => {
                        debug!("Fully received the header frame, but failed to parse its headers.");
                        if buffer[FRAME_HEADER_LENGTH..].len() >= frame.length as usize
                            && frames::Headers::is_end_header(frame.flags)
                        {
                            return Err(ConnectionError::InvalidHeaderFrame(err));
                        }
                        debug!("END_HEADER flag not set");
                        trace!("Try to receive continuation frames");

                        let mut headers_bytes = frames::Headers::extract_headers_data(
                            &buffer[FRAME_HEADER_LENGTH..],
                            frame.flags,
                            frame.length as usize,
                        )
                        .map_err(ConnectionError::InvalidHeaderFrame)?;
                        buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
                        debug!(
                            "Extracted {} bytes from first header frame",
                            headers_bytes.len()
                        );
                        return receive_continuation_frames(
                            stream,
                            buffer,
                            &mut headers_bytes,
                            decoder,
                            frame,
                            max_frame_size,
                        )
                        .await;
                    }
                }
            },
            _ => Err(ConnectionError::InvalidHeaderFrame(err)),
        },
    }
}

async fn receive_continuation_frames(
    stream: &mut TlsStream<TcpStream>,
    buffer: &mut BytesMut,
    headers_bytes: &mut BytesMut,
    decoder: &mut hpack::Decoder<'_>,
    frame: &frames::Frame,
    max_frame_size: u32,
) -> ConnectionResult<frames::Headers> {
    trace!("Begin continuation frame handling");
    let mut total_size: usize = frame.length as usize;

    loop {
        let frame = frames::Frame::try_from_bytes(buffer.as_ref(), max_frame_size)
            .map_err(ConnectionError::InvalidHeaderFrame)?;
        if let FrameType::Continuation = frame.frame_type {
            debug!("Got continuation frame header: {frame:?}");
            let continuation = match frames::Continuation::from_bytes(
                &buffer[FRAME_HEADER_LENGTH..],
                frame.flags,
                frame.length as usize,
            ) {
                Ok(c) => c,
                Err(frames::FrameError::BadFrameSize(len)) => {
                    trace!(
                        "Continuation frame not full. (length: {}, got: {})",
                        frame.length,
                        len,
                    );
                    continue;
                }
                Err(err) => return Err(ConnectionError::InvalidContinuationFrame(err)),
            };

            info!("Received continuation frame: {continuation:?}");
            total_size += continuation.headers.len();
            debug!("Total header size is now: {total_size}");
            headers_bytes.put_slice(continuation.headers.as_ref());
            buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);

            if continuation.is_end_header() {
                trace!("Continuation has END_HEADER set. stop receiving");
                break;
            } else {
                trace!("Continuation has not END_HEADER set. keep receiving...");
                let _ = stream
                    .read_buf(buffer)
                    .await
                    .map_err(ConnectionError::IOError)?;
            }
        } else {
            return Err(ConnectionError::MissingContinuationFrame);
        }
    }

    frames::Headers::from_bytes(headers_bytes.as_ref(), decoder, frame.flags, total_size)
        .map_err(ConnectionError::InvalidHeaderFrame)
}

async fn send_reset_stream(
    stream: &mut TlsStream<TcpStream>,
    stream_id: u32,
    err: ConnectionError,
) -> io::Result<()> {
    warn!("Send reset stream (id={}): {:?}", stream_id, err);
    let error_type: frames::ErrorType = err.into();
    let frame = frames::ResetStream::new(error_type.into());
    let mut buffer =
        BytesMut::with_capacity(FRAME_HEADER_LENGTH + frame.compute_frame_length(None) as usize);
    build_frame_header(
        &mut buffer,
        frames::FrameType::ResetStream,
        stream_id,
        &frame,
        None,
    );

    connection_error_to_io_error!(send_all(stream, buffer.as_ref()).await, ())
}

async fn send_go_away(stream: &mut TlsStream<TcpStream>, err: ConnectionError) -> io::Result<()> {
    error!("Send GoAway frame {:?}", err);

    let error_type: frames::ErrorType = err.into();
    let frame = frames::GoAway::new(error_type, 0, error_type.to_string().as_bytes().to_vec());
    let mut buffer =
        BytesMut::with_capacity(FRAME_HEADER_LENGTH + frame.compute_frame_length(None) as usize);

    build_frame_header(&mut buffer, frames::FrameType::GoAway, 0, &frame, None);

    connection_error_to_io_error!(send_all(stream, &buffer).await, ())
}

pub async fn do_connection_loop(
    stream: &mut TlsStream<TcpStream>,
    conf: &Arc<Config>,
    mut buffer: BytesMut,
) -> ConnectionResult<()> {
    let mut decoder = hpack::Decoder::new();
    let mut encoder = hpack::Encoder::new();
    let mut stream_manager = stream::StreamManager::new();
    let mut max_frame_size: u32 = frames::MIN_FRAME_SIZE;
    let mut global_window_size: i64 = 65535;

    loop {
        let frame_result = frames::Frame::try_from_bytes(buffer.as_ref(), max_frame_size);
        let frame = match frame_result {
            Ok(fr) => fr,
            Err(frames::FrameError::UnknownFrameNumber(n)) => {
                warn!("Got unknown frame number: {}. Ignoring it.", n);
                buffer.advance(u32::from_be_bytes([0, buffer[0], buffer[1], buffer[2]]) as usize);
                let _ = stream
                    .read_buf(&mut buffer)
                    .await
                    .map_err(ConnectionError::IOError)?;
                continue;
            }
            Err(frames::FrameError::FrameTooBig {
                actual,
                max_frame_size,
            }) => {
                return Err(ConnectionError::FrameTooBig {
                    actual,
                    max_frame_size,
                });
            }
            Err(msg) => {
                warn!("Bad frame: {msg}");
                trace!("Continue to read, the frame might be not fully received");
                let count = stream
                    .read_buf(&mut buffer)
                    .await
                    .map_err(ConnectionError::IOError)?;

                if count == 0 {
                    error!("Remote closed connection.");
                    break;
                } else {
                    continue;
                }
            }
        };

        info!("received frame: {:?}", frame);

        match frame.frame_type {
            // TODO: We may want to add a timeout for settings frames. As advised by RFC 9113
            FrameType::Settings => {
                // Settings for a stream other than 0 are not allowed
                if frame.stream_identifier != 0 {
                    return Err(ConnectionError::SettingsNonDefaultSream);
                }

                // Setting ACK, ignore the frame.
                if frame.flags & 0x01 > 0 {
                    debug!("Client ack settings");

                    if frame.length > 0 {
                        error!(
                            "Settings ACK frame received with a length of {}",
                            frame.length
                        );
                        return Err(ConnectionError::NonZeroSettingsAckLength);
                    }

                    buffer.advance(FRAME_HEADER_LENGTH); // consume current frame
                    continue;
                }

                let settings = match frames::Settings::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                ) {
                    Ok(s) => s,
                    Err(frames::FrameError::BadFrameSize(s)) => {
                        trace!("Bad settings frame size: {s}");
                        let _ = stream
                            .read_buf(&mut buffer)
                            .await
                            .map_err(ConnectionError::IOError)?;
                        continue;
                    }
                    Err(frames::FrameError::SettingsFrameSize(_)) => {
                        return Err(ConnectionError::SettingsLengthNotMultipleOf6);
                    }
                    Err(_) => {
                        return Err(ConnectionError::InvalidFrame);
                    }
                };

                info!("settings: {:?}", settings);

                if let Some(value) = settings.is_push_enabled() {
                    if value != 0 && value != 1 {
                        return Err(ConnectionError::InvalidSettingValue(value));
                    }
                }

                let setting_frame_size = settings.get_max_frame_size();
                if !(frames::MIN_FRAME_SIZE..=frames::MAX_FRAME_SIZE).contains(&setting_frame_size)
                {
                    return Err(ConnectionError::InvalidSettingValue(setting_frame_size));
                }

                if setting_frame_size != max_frame_size {
                    trace!("Set new max frame size to: {}", setting_frame_size);
                    max_frame_size = setting_frame_size;
                }

                if let Some(size) = settings.get_initial_window_size() {
                    if size as i64 > stream::MAX_WINDOW_SIZE {
                        return Err(ConnectionError::BadInitialWindowSize(size));
                    }

                    trace!("Change initial window size to {size}");
                    stream_manager.set_initial_window_size(size as i64);
                }

                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize); // consume current frame
                let mut ack_buffer = BytesMut::with_capacity(
                    settings.compute_frame_length(None) as usize + FRAME_HEADER_LENGTH,
                );
                build_frame_header(
                    &mut ack_buffer,
                    frame.frame_type,
                    frame.stream_identifier,
                    &frames::Settings::new_ack(),
                    None,
                );
                send_all(stream, &ack_buffer[..]).await?;
            }
            FrameType::WindowUpdate => {
                let window_update = match frames::WindowUpdate::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                ) {
                    Ok(wu) => wu,
                    Err(frames::FrameError::BadFrameSize(_)) => {
                        let _ = stream
                            .read_buf(&mut buffer)
                            .await
                            .map_err(ConnectionError::IOError)?;
                        continue;
                    }
                    Err(_) => return Err(ConnectionError::InvalidFrame),
                };

                info!("Window update: {:?}", window_update);

                // See RFC 9113
                if window_update.0 == 0 {
                    error!("Received a window update of 0");
                    return Err(ConnectionError::ZeroWindowUpdate);
                }

                if frame.length != 4 {
                    error!("Window update length is not 4");
                    return Err(ConnectionError::BadLengthWindowUpdate(frame.length));
                }

                match stream_manager.get_at_mut(frame.stream_identifier) {
                    Some(st) if st.identifier == 0 => {
                        global_window_size += window_update.0 as i64;
                        trace!("Global window is now at {global_window_size}");
                        match st.update_window(window_update.0 as i64) {
                            Err(_) if frame.stream_identifier == 0 => {
                                return Err(ConnectionError::WindowUpdateTooBig);
                            }
                            Err(_) => {
                                send_reset_stream(
                                    stream,
                                    frame.stream_identifier,
                                    ConnectionError::WindowUpdateTooBig,
                                )
                                .await
                                .map_err(ConnectionError::IOError)?;
                                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
                                continue;
                            }
                            Ok(()) => (),
                        };
                    }
                    Some(st) => {
                        match st.update_window(window_update.0 as i64) {
                            Err(_) if frame.stream_identifier == 0 => {
                                return Err(ConnectionError::WindowUpdateTooBig);
                            }
                            Err(_) => {
                                send_reset_stream(
                                    stream,
                                    frame.stream_identifier,
                                    ConnectionError::WindowUpdateTooBig,
                                )
                                .await
                                .map_err(ConnectionError::IOError)?;
                                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
                                continue;
                            }
                            Ok(()) => (),
                        }

                        // Check if the stream has pending data to send because
                        // window size was too small to send data
                        if st.has_data_to_send() {
                            trace!("Stream has more data to send. Resuming it");
                            st.try_send_data_payload(
                                stream,
                                &mut global_window_size,
                                max_frame_size as usize,
                            )
                            .await?;
                        }
                    }
                    None => {
                        trace!("Update window received on an unregistered stream. Registering it");
                        stream_manager.register_new_stream(frame.stream_identifier);
                        match stream_manager
                            .get_at_mut(frame.stream_identifier)
                            .unwrap()
                            .update_window(window_update.0 as i64)
                        {
                            Err(_) if frame.stream_identifier == 0 => {
                                return Err(ConnectionError::WindowUpdateTooBig);
                            }
                            Err(_) => {
                                send_reset_stream(
                                    stream,
                                    frame.stream_identifier,
                                    ConnectionError::WindowUpdateTooBig,
                                )
                                .await
                                .map_err(ConnectionError::IOError)?;
                                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
                                continue;
                            }
                            Ok(()) => (),
                        }

                        if frame.stream_identifier == 0 {
                            global_window_size += window_update.0 as i64;
                            trace!("Global window size is now {global_window_size}");
                        }
                    }
                }

                if frame.stream_identifier == 0 {
                    trace!("Global window size updated. Try to resume stall streams");
                    stream_manager
                        .try_send_data_all_stream(
                            stream,
                            &mut global_window_size,
                            max_frame_size as usize,
                        )
                        .await?;
                }

                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize); // consume current frame
            }
            FrameType::Headers => {
                if frame.stream_identifier == 0 {
                    debug!("Header received on stream `0`");
                    return Err(ConnectionError::HeaderOnStreamZero);
                }

                match receive_headers(stream, &mut buffer, &mut decoder, &frame, max_frame_size)
                    .await
                {
                    Ok(headers) => {
                        if !stream_manager.has_stream(frame.stream_identifier) {
                            stream_manager.register_new_stream(frame.stream_identifier);
                        }

                        let response_stream =
                            stream_manager.get_at_mut(frame.stream_identifier).unwrap();
                        response_stream.set_headers(headers);
                        response_stream
                            .respond_to(
                                stream,
                                max_frame_size as usize,
                                &mut global_window_size,
                                conf,
                                &mut encoder,
                            )
                            .await?;
                    }
                    Err(err) => {
                        error!("Failed to parse headers: {err:?}");
                        return Err(ConnectionError::InvalidFrame);
                    }
                }
            }
            FrameType::GoAway => {
                info!("Go away received: {:?}", frame);

                let go_away = match frames::GoAway::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                ) {
                    Ok(go_away) => go_away,
                    Err(frames::FrameError::BadFrameSize(_)) => {
                        trace!("Go Away frame not fully received. Reading... again");
                        let _ = stream
                            .read_buf(&mut buffer)
                            .await
                            .map_err(ConnectionError::IOError)?;
                        continue;
                    }
                    Err(_) => {
                        return Err(ConnectionError::InvalidFrame);
                    }
                };

                if frame.stream_identifier != 0 {
                    return Err(ConnectionError::GoAwayOnNonDefaultStream);
                }

                if go_away.is_error() {
                    error!(
                        "Received go away frame: {:?} message: {}",
                        go_away,
                        String::from_utf8_lossy(&go_away.additionnal_data)
                    );
                } else {
                    info!("Terminate connection without errors.");
                }

                return Ok(());
            }
            FrameType::Priority => {
                info!("Priority received: {:?}", frame);
                if frame.stream_identifier == 0 {
                    debug!("Received priority frame on stream `0`");
                    return Err(ConnectionError::PriorityFrameOnStreamZero);
                }

                if frame.length != 5 {
                    return Err(ConnectionError::BadLengthPriorityFrame(frame.length));
                }

                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize); // consume current frame
            }
            FrameType::Continuation => {
                return Err(ConnectionError::ContinuationFrameWithoutHeaderFrame);
            }
            FrameType::ResetStream => {
                info!("Reset stream received: {frame:?}");
                let reset_frame = match frames::ResetStream::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                ) {
                    Ok(f) => f,
                    Err(frames::FrameError::BadFrameSize(_)) => {
                        let _ = stream
                            .read_buf(&mut buffer)
                            .await
                            .map_err(ConnectionError::IOError)?;
                        continue;
                    }
                    Err(_) => return Err(ConnectionError::InvalidFrame),
                };

                info!("Reset stream: {:?}", reset_frame);
                let initial_size = stream_manager.get_initial_window_size();
                stream_manager
                    .get_at_mut(frame.stream_identifier)
                    .ok_or(ConnectionError::InvalidFrame)?
                    .reset(initial_size);

                buffer.advance(FRAME_HEADER_LENGTH + 4);
            }
            FrameType::Data => {
                // RFC 9113 6.1: `If a DATA frame is received whose Stream
                // Identifier field is 0x00, the recipient MUST respond with a
                // connection error`
                if frame.stream_identifier == 0 {
                    return Err(ConnectionError::DataOnStreamZero);
                }

                warn!(
                    "Got data frame. Its content will be ignored the server does not support it."
                );
                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
            }
            FrameType::PushPromise => todo!(),
            FrameType::Ping => {
                // See RFC 9113
                if frame.stream_identifier != 0 {
                    error!("Ping on non zero stream received");
                    return Err(ConnectionError::PingOnNon0Stream);
                }

                if frame.length != PING_LENGTH as u32 {
                    error!(
                        "Ping frame length is not {} (got: {})",
                        PING_LENGTH, frame.length
                    );

                    return Err(ConnectionError::BadPingFrameSize);
                }

                let ping =
                    match frames::Ping::from_bytes(&buffer[FRAME_HEADER_LENGTH..], frame.flags) {
                        Ok(p) => p,
                        Err(frames::FrameError::BadFrameSize(_)) => {
                            trace!("ping frame not fully received. Reading... again");
                            let _ = stream
                                .read_buf(&mut buffer)
                                .await
                                .map_err(ConnectionError::IOError)?;

                            continue;
                        }
                        Err(_) => unreachable!(),
                    };

                if ping.is_ack {
                    info!("Received ping ack with value: {}", ping.opaque_data);
                } else {
                    let ping_ack = frames::Ping::new(true, ping.opaque_data);
                    let mut buf =
                        BytesMut::with_capacity(FRAME_HEADER_LENGTH + frames::PING_LENGTH);

                    build_frame_header(
                        &mut buf,
                        FrameType::Ping,
                        frame.stream_identifier,
                        &ping_ack,
                        None,
                    );

                    send_all(stream, buf.as_ref()).await?;
                }

                buffer.advance(FRAME_HEADER_LENGTH + frames::PING_LENGTH);
            }
        }
    }

    Ok(())
}

#[tracing::instrument(level = "info")]
pub async fn do_http2(mut stream: TlsStream<TcpStream>, config: Arc<Config>) -> io::Result<()> {
    let mut buffer = BytesMut::new();
    trace!("Do http2 connection");
    let _ = stream.read_buf(&mut buffer).await?; // read connection preface

    debug!("Check connection preface");
    let preface_offset = match check_connection_preface(&buffer) {
        Some(size) => size,
        None => {
            error!("Bad connection preface!");
            return send_go_away(&mut stream, ConnectionError::BadConnectionPreface).await;
        }
    };

    debug!("Connection preface good");

    buffer.advance(preface_offset); // consume connection preface in buffer
    match send_server_setting(&mut stream).await {
        Ok(()) => (),
        Err(ConnectionError::IOError(e)) => return Err(e),
        Err(err) => return send_go_away(&mut stream, err).await,
    }

    match do_connection_loop(&mut stream, &config, buffer).await {
        Ok(_) => Ok(()),
        Err(ConnectionError::IOError(e)) => Err(e),
        Err(err) => send_go_away(&mut stream, err).await,
    }
}
