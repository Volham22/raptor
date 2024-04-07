use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

use crate::{
    frames::{Frame, SerializeFrame},
    server::ConnectionStream,
};

pub(crate) async fn receive_n_bytes(
    stream: &mut ConnectionStream,
    count: usize,
) -> io::Result<Vec<u8>> {
    let mut received_bytes_count = 0usize;
    let mut buffer = vec![0u8; count];

    while received_bytes_count < count {
        received_bytes_count += stream.read(&mut buffer).await?;
    }

    Ok(buffer)
}

pub(crate) async fn write_all_buffer(
    stream: &mut ConnectionStream,
    buffer: &[u8],
) -> io::Result<()> {
    let mut sent_size = 0usize;

    while sent_size < buffer.len() {
        sent_size += stream.write(buffer).await?;
    }

    Ok(())
}

pub(crate) async fn send_frame<T: SerializeFrame>(
    stream: &mut ConnectionStream,
    frame: &mut Frame,
    payload: T,
) -> io::Result<()> {
    let mut bytes = Vec::new();
    let mut payload_bytes = payload.serialize_frame(frame);
    frame.length = payload_bytes.len() as u32;
    debug!("Send frame header: {frame:?}");

    bytes.extend_from_slice(&frame.length.to_be_bytes()[1..]);
    bytes.extend_from_slice(&(frame.frame_type as u8).to_be_bytes());
    bytes.extend_from_slice(&frame.flags.to_be_bytes());
    bytes.extend_from_slice(&frame.stream_id.to_be_bytes());
    bytes.append(&mut payload_bytes);
    debug!("Frame: {bytes:#01x?}");

    write_all_buffer(stream, &bytes).await
    // Ok(())
}
