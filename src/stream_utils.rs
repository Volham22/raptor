use bytes::BytesMut;
use chrono::{self, Datelike, Timelike};
use http::Response;
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
};

pub async fn write_bytes_to_stream<T: AsyncRead + AsyncWrite + std::marker::Unpin>(
    tcp_stream: &mut T,
    buffer: &Vec<u8>,
) -> Result<(), String> {
    match tcp_stream.write_all(&buffer).await {
        Ok(()) => {}
        Err(msg) => {
            return Err(msg.to_string());
        }
    }

    Ok(())
}

pub async fn write_buffered_read_to_stream<T: AsyncRead + AsyncWrite + std::marker::Unpin>(
    tcp_stream: &mut T,
    file: &mut File,
) -> Result<(), String> {
    loop {
        let mut buffer = BytesMut::with_capacity(1024usize);
        match file.read_buf(&mut buffer).await {
            Err(msg) => return Err(msg.to_string()),
            Ok(count) => {
                if count == 0 {
                    break;
                }
            }
        }

        if buffer.len() == 0 {
            break;
        }

        write_bytes_to_stream(tcp_stream, &buffer.to_vec()).await?;
    }

    Ok(())
}

pub fn generate_http_date_header() -> Vec<u8> {
    let now = chrono::offset::Utc::now();

    format!(
        "Date: {}, {} {} {} {}:{}:{}\r\n",
        now.weekday().to_string(),
        now.day(),
        now.month(),
        now.year(),
        now.hour(),
        now.minute(),
        now.second()
    )
    .as_bytes()
    .to_vec()
}

pub fn serialize_header<T>(resp: &Response<T>) -> Vec<u8> {
    let mut result: Vec<u8> = Vec::new();
    let carriage_return = b"\r\n";

    result.extend_from_slice(b"HTTP/1.1 ");
    result.extend_from_slice(resp.status().to_string().as_bytes());
    result.extend_from_slice(carriage_return);
    result.extend_from_slice(b"Server: feather\r\n");
    result.extend(generate_http_date_header());

    for (name, value) in resp.headers() {
        result.extend_from_slice(name.as_str().as_bytes());
        result.extend_from_slice(b": ");
        result.extend_from_slice(value.as_bytes());
        result.extend_from_slice(carriage_return);
    }

    result.extend_from_slice(carriage_return);

    result
}
