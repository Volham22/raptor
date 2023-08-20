use std::io;

use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};
use tracing::{debug, info, trace, warn};

use crate::http2::{
    check_connection_preface,
    frames::{self, Frame, FrameType, Settings, FRAME_HEADER_LENGTH},
    response::{build_frame_header, respond_request, ResponseSerialize},
};

pub async fn send_all(stream: &mut TlsStream<TcpStream>, data: &[u8]) -> io::Result<()> {
    let mut sent_data = 0usize;

    while sent_data < data.len() {
        sent_data += stream.write(data).await?;
    }

    Ok(())
}

async fn send_server_setting(stream: &mut TlsStream<TcpStream>) -> io::Result<()> {
    let default_settings = Settings::default(); // TODO: This could be constant
    debug!("Send server settings: {default_settings:?}");
    let mut buffer = BytesMut::with_capacity(
        FRAME_HEADER_LENGTH + default_settings.compute_frame_length(None) as usize,
    );

    build_frame_header(&mut buffer, FrameType::Settings, 0, &default_settings, None);
    send_all(stream, &buffer[..]).await
}

pub async fn do_connection(ssl_socket: TlsAcceptor, client_socket: TcpStream) -> io::Result<()> {
    let mut buffer = BytesMut::new();
    let mut stream = ssl_socket.accept(client_socket).await?;
    let _ = stream.read_buf(&mut buffer).await?; // read connection preface
    debug!("Check connection preface");
    let preface_offset = check_connection_preface(&buffer).expect("Bad connection preface");
    debug!("Connection preface good");
    let mut decoder = hpack::Decoder::new();
    let mut encoder = hpack::Encoder::new();
    buffer.advance(preface_offset); // consume connection preface in buffer
    send_server_setting(&mut stream).await?;

    loop {
        debug!("buffer content: {:X?}", buffer);
        let frame = match Frame::try_from(&buffer) {
            Ok(fr) => fr,
            Err(msg) => {
                warn!("Bad frame: {msg}");
                trace!("Continue to read, the frame might be not fully received");
                let _ = stream.read_buf(&mut buffer).await?;
                continue;
            }
        };

        info!("received frame: {:?}", frame);

        match frame.frame_type {
            FrameType::Settings => {
                // Setting ACK, ignore the frame.
                if frame.flags & 0x01 > 0 {
                    debug!("Client ack settings");
                    buffer.advance(FRAME_HEADER_LENGTH); // consume current frame
                    continue;
                }

                let settings = frames::Settings::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                )
                .expect("Failed to parse settings");
                info!("settings: {:?}", settings);

                let mut buffer = BytesMut::with_capacity(
                    settings.compute_frame_length(None) as usize + FRAME_HEADER_LENGTH,
                );
                build_frame_header(
                    &mut buffer,
                    frame.frame_type,
                    frame.stream_identifier,
                    &Settings::new_ack(),
                    None,
                );
                send_all(&mut stream, &buffer[..]).await?;
            }
            FrameType::WindowUpdate => {
                let window_update = frames::WindowUpdate::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                )
                .expect("Failed to parse window update");
                info!("Window update: {:?}", window_update);
            }
            FrameType::Headers => {
                let headers = if let Ok(h) = frames::Headers::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    &mut decoder,
                    frame.flags,
                    frame.length as usize,
                ) {
                    h
                } else {
                    let _ = stream.read_buf(&mut buffer).await?;
                    continue;
                };
                info!("Headers: {:?}", headers);
                respond_request(&mut stream, frame.stream_identifier, &headers, &mut encoder)
                    .await?;
            }
            FrameType::GoAway => {
                info!("Go away received: {:?}", frame);

                if frame.stream_identifier == 0 && frame.flags == 0 {
                    info!("Gracefully closing the connection");
                    break;
                }
            }
            FrameType::Priority => {
                info!("Priority received: {:?}", frame);
            }
            FrameType::ResetStream => {
                info!("Reset stream received: {frame:?}");
            }
            _ => continue,
        }

        // Parse next frame
        buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize); // consume current frame
        if buffer.is_empty() {
            let _ = stream.read_buf(&mut buffer).await?;
        }
    }

    Ok(())
}
