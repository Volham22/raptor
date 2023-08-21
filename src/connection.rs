use std::{io, path::Path};

use bytes::{Buf, BufMut, BytesMut};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use tracing::{debug, error, info, trace, warn};

use crate::http2::{
    check_connection_preface,
    frames::{self, Frame, FrameType, FRAME_HEADER_LENGTH},
    response::{build_frame_header, respond_request, ResponseSerialize},
    stream::StreamManager,
};

macro_rules! connection_error_to_io_error {
    ($err:expr, $ty:ty) => {
        match $err {
            Ok(_) => Ok::<$ty, ::std::io::Error>(()),
            Err(crate::connection::ConnectionError::IOError(err)) => {
                Err::<$ty, ::std::io::Error>(err)
            }
            Err(_) => unreachable!("should never be other than IOError!"),
        }
    };
}

pub(crate) use connection_error_to_io_error;

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
    #[error("Window update frame lenght is incorrect: ({0} != 4)")]
    BadLengthWindowUpdate(u32),
}

type ConnectionResult<T> = Result<T, ConnectionError>;

pub async fn send_all(stream: &mut TlsStream<TcpStream>, data: &[u8]) -> ConnectionResult<()> {
    let mut sent_data = 0usize;

    while sent_data < data.len() {
        sent_data += stream.write(data).await.map_err(ConnectionError::IOError)?;
    }

    Ok(())
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
    frame: &Frame,
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
    frame: &Frame,
) -> ConnectionResult<frames::Headers> {
    trace!("Begin continuation frame handling");
    let mut total_size: usize = frame.length as usize;

    loop {
        let frame =
            Frame::try_from(buffer.as_ref()).map_err(ConnectionError::InvalidHeaderFrame)?;
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

async fn send_go_away(stream: &mut TlsStream<TcpStream>, err: ConnectionError) -> io::Result<()> {
    error!("Send GoAway frame {:?}", err);

    let error_type = match err {
        ConnectionError::IOError(_) => unreachable!(),
        ConnectionError::NonZeroSettingsAckLength
        | ConnectionError::BadLengthWindowUpdate(_)
        | ConnectionError::SettingsLengthNotMultipleOf6 => frames::ErrorType::FrameSizeError,
        _ => frames::ErrorType::ProtocolError,
    };

    let frame = frames::GoAway::new(error_type, 0, error_type.to_string().as_bytes().to_vec());
    let mut buffer =
        BytesMut::with_capacity(FRAME_HEADER_LENGTH + frame.compute_frame_length(None) as usize);

    build_frame_header(&mut buffer, frames::FrameType::GoAway, 0, &frame, None);
    match send_all(stream, &buffer).await {
        Ok(()) => Ok(()),
        Err(ConnectionError::IOError(e)) => Err(e),
        Err(_) => unreachable!(),
    }
}

pub async fn do_connection_loop(
    stream: &mut TlsStream<TcpStream>,
    root_dir: &Path,
    mut buffer: BytesMut,
) -> ConnectionResult<()> {
    let mut decoder = hpack::Decoder::new();
    let mut encoder = hpack::Encoder::new();
    let mut stream_manager = StreamManager::new();

    loop {
        let frame_result = Frame::try_from(buffer.as_ref());
        let frame = match frame_result {
            Ok(fr) => fr,
            Err(msg) => {
                warn!("Bad frame: {msg}");
                trace!("Continue to read, the frame might be not fully received");
                let _ = stream
                    .read_buf(&mut buffer)
                    .await
                    .map_err(ConnectionError::IOError)?;
                continue;
            }
        };

        info!("received frame: {:?}", frame);

        match frame.frame_type {
            // TODO: We may want to add a timeout for settings frames. As adviced by RFC 9113
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

                    if frame.length % 6 != 0 {
                        error!(
                            "Settings length is not a multiple of 6! (size: {})",
                            frame.length
                        );
                        return Err(ConnectionError::SettingsLengthNotMultipleOf6);
                    }

                    buffer.advance(FRAME_HEADER_LENGTH); // consume current frame
                    continue;
                }

                let settings = frames::Settings::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                )
                .expect("Failed to parse settings");
                info!("settings: {:?}", settings);

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
                let window_update = frames::WindowUpdate::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                )
                .map_err(|_| ConnectionError::InvalidFrame)?;
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
                    Some(st) => st.update_window(window_update.0),
                    None => {
                        trace!("Update window received on an unregistered stream. Registering it");
                        stream_manager.register_new_stream(frame.stream_identifier);
                        stream_manager
                            .get_at_mut(frame.stream_identifier)
                            .unwrap()
                            .update_window(window_update.0);
                    }
                }

                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize); // consume current frame
            }
            FrameType::Headers => {
                match receive_headers(stream, &mut buffer, &mut decoder, &frame).await {
                    Ok(headers) => {
                        if !stream_manager.has_stream(frame.stream_identifier) {
                            stream_manager.register_new_stream(frame.stream_identifier);
                        }

                        stream_manager
                            .get_at_mut(frame.stream_identifier)
                            .unwrap()
                            .set_headers(headers);

                        respond_request(
                            stream,
                            frame.stream_identifier,
                            &mut stream_manager,
                            root_dir,
                            &mut encoder,
                        )
                        .await
                        .map_err(ConnectionError::IOError)?;
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
                    error!("Received go away frame: {:?}", go_away);
                } else {
                    info!("Terminate connection without errors.");
                }

                return Ok(());
            }
            FrameType::Priority => {
                info!("Priority received: {:?}", frame);
                buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize); // consume current frame
            }
            FrameType::Continuation => {
                return Err(ConnectionError::ContinuationFrameWithoutHeaderFrame);
            }
            FrameType::ResetStream => {
                info!("Reset stream received: {frame:?}");
                todo!("Handle reset stream");
            }
            FrameType::Data => todo!(),
            FrameType::PushPromise => todo!(),
            FrameType::Ping => {
                let ping =
                    match frames::Ping::from_bytes(&buffer[FRAME_HEADER_LENGTH..], frame.flags) {
                        Ok(p) => p,
                        Err(frames::FrameError::BadFrameSize(0)) => {
                            trace!("ping frame not fully received. Reading... again");
                            let _ = stream
                                .read_buf(&mut buffer)
                                .await
                                .map_err(ConnectionError::IOError)?;
                            continue;
                        }
                        Err(_) => unreachable!(),
                    };

                // See RFC 9113
                if frame.length != 8 {
                    error!("Ping frame has a length other than 8");
                    return Err(ConnectionError::BadPingFrameSize);
                }

                // See RFC 9113
                if frame.stream_identifier != 0 {
                    error!("Ping on non zero stream received");
                    return Err(ConnectionError::PingOnNon0Stream);
                }

                if ping.is_ack {
                    info!("Received ping ack with value: {}", ping.opaque_data);
                } else {
                    let ping_ack = frames::Ping::new(true);
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
                    buffer.advance(FRAME_HEADER_LENGTH + frames::PING_LENGTH);
                }
            }
        }
    }
}
pub async fn do_connection(
    ssl_socket: TlsAcceptor,
    client_socket: TcpStream,
    root_dir: &Path,
) -> io::Result<()> {
    let mut buffer = BytesMut::new();
    let mut stream = ssl_socket.accept(client_socket).await?;
    let _ = stream.read_buf(&mut buffer).await?; // read connection preface

    debug!("Check connection preface");
    let preface_offset = check_connection_preface(&buffer).expect("Bad connection preface");
    debug!("Connection preface good");

    buffer.advance(preface_offset); // consume connection preface in buffer
    match send_server_setting(&mut stream).await {
        Ok(()) => (),
        Err(ConnectionError::IOError(e)) => return Err(e),
        Err(err) => send_go_away(&mut stream, err).await?,
    }

    connection_error_to_io_error!(do_connection_loop(&mut stream, root_dir, buffer).await, ())
}
