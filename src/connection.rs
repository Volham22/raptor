use std::io;

use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::TlsAcceptor;

use crate::http2::{
    check_connection_preface,
    frames::{self, Frame, FrameType, Settings, FRAME_HEADER_LENGTH},
    response::{build_frame_header, ResponseSerialize},
};

pub async fn do_connection(ssl_socket: TlsAcceptor, client_socket: TcpStream) -> io::Result<()> {
    let mut buffer = BytesMut::new();

    let mut stream = ssl_socket.accept(client_socket).await?;
    let _ = stream.read_buf(&mut buffer).await?;
    let preface_offset = check_connection_preface(&buffer).expect("Bad connection preface");
    println!("Connection preface good");
    buffer.advance(preface_offset);

    loop {
        println!("buffer content: {:X?}", buffer);
        let frame = Frame::try_from(&buffer).expect("Failed to parse frame");
        println!("received frame: {:?}", frame);

        match frame.frame_type {
            FrameType::Settings => {
                let settings = frames::Settings::from_bytes(&buffer[9..], frame.length as usize)
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
                stream.write_buf(&mut buffer).await?;
            }
            FrameType::WindowUpdate => {
                let window_update =
                    frames::WindowUpdate::from_bytes(&buffer[9..], frame.length as usize)
                        .expect("Failed to parse window update");
                println!("Window update: {:?}", window_update);
            }
            _ => continue,
        }

        // Parse next frame
        buffer.advance((9 + frame.length) as usize);
        let _ = stream.read_buf(&mut buffer).await?;
    }
}
