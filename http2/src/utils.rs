use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::server::ConnectionStream;

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
