use std::io;

use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{server::TlsStream, TlsAcceptor};

use crate::http2::{
    check_connection_preface,
    frames::{self, Frame, FrameType, Settings, FRAME_HEADER_LENGTH},
    response::{build_frame_header, ResponseSerialize},
};

async fn send_all(stream: &mut TlsStream<TcpStream>, data: &[u8]) -> io::Result<()> {
    let mut sent_data = 0usize;

    while sent_data < data.len() {
        sent_data += stream.write(data).await?;
    }

    Ok(())
}

async fn send_server_setting(stream: &mut TlsStream<TcpStream>) -> io::Result<()> {
    let default_settings = Settings::default(); // TODO: This could be constant
    let mut buffer = BytesMut::with_capacity(
        FRAME_HEADER_LENGTH + default_settings.compute_frame_length() as usize,
    );

    build_frame_header(&mut buffer, FrameType::Settings, 0, 0, &default_settings);
    send_all(stream, &buffer[..]).await
}

pub async fn do_connection(ssl_socket: TlsAcceptor, client_socket: TcpStream) -> io::Result<()> {
    let mut buffer = BytesMut::new();

    let mut stream = ssl_socket.accept(client_socket).await?;
    let _ = stream.read_buf(&mut buffer).await?;
    let preface_offset = check_connection_preface(&buffer).expect("Bad connection preface");
    let mut decoder = hpack::Decoder::new();
    println!("Connection preface good");
    buffer.advance(preface_offset);
    send_server_setting(&mut stream).await?;

    loop {
        println!("buffer content: {:X?}", buffer);
        let frame = Frame::try_from(&buffer).expect("Failed to parse frame");
        println!("received frame: {:?}", frame);

        match frame.frame_type {
            FrameType::Settings => {
                // Setting ACK, ignore the frame.
                if frame.flags & 0x01 > 0 {
                    println!("Client ack settings");
                    continue;
                }

                let settings = frames::Settings::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                )
                .expect("Failed to parse settings");
                println!("settings: {:?}", settings);

                let mut buffer = BytesMut::with_capacity(
                    settings.compute_frame_length() as usize + FRAME_HEADER_LENGTH,
                );
                build_frame_header(
                    &mut buffer,
                    frame.frame_type,
                    frame.flags,
                    frame.stream_identifier,
                    &Settings::new_ack(),
                );
                send_all(&mut stream, &buffer[..]).await?;
            }
            FrameType::WindowUpdate => {
                let window_update = frames::WindowUpdate::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    frame.length as usize,
                )
                .expect("Failed to parse window update");
                println!("Window update: {:?}", window_update);
            }
            FrameType::Headers => {
                let headers = frames::Headers::from_bytes(
                    &buffer[FRAME_HEADER_LENGTH..],
                    &mut decoder,
                    frame.flags,
                    frame.length as usize,
                )
                .expect("Failed to parse headers");
                println!("Headers: {:?}", headers);
            }
            _ => continue,
        }

        // Parse next frame
        buffer.advance(FRAME_HEADER_LENGTH + frame.length as usize);
        let _ = stream.read_buf(&mut buffer).await?;
    }
}
