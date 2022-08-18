use bytes::BytesMut;
use httparse::Request;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

use crate::{config::Vhost, handlers::handle_request};

pub struct Connection<T: AsyncRead + AsyncWrite + std::marker::Unpin> {
    stream: T,
    header_end_index: usize,
    buffer: BytesMut,
}

impl<T: AsyncRead + AsyncWrite + std::marker::Unpin> Connection<T> {
    pub fn new(stream: T) -> Self {
        Self {
            stream,
            header_end_index: 0,
            buffer: BytesMut::new(),
        }
    }

    pub async fn read_requests(&mut self, vhost_lists: &[Vhost]) {
        loop {
            let received_bytes_count = self.stream.read_buf(&mut self.buffer).await.unwrap();
            if received_bytes_count == 0 {
                break;
            }

            let mut header = [httparse::EMPTY_HEADER; 100];
            let mut req = Request::new(&mut header);

            match req.parse(&self.buffer) {
                Ok(res) => {
                    if res.is_complete() {
                        self.header_end_index = res.unwrap();
                        if self.is_payload_complete(&req) {
                            println!("Received: {:?}", req);
                            match handle_request(&req, &mut self.stream, vhost_lists).await {
                                Ok(keep) => {
                                    if keep {
                                        println!("Keep connection alive.");
                                        self.buffer.clear();
                                        continue;
                                    } else {
                                        println!("closed connection.");
                                        break;
                                    }
                                }
                                Err(msg) => {
                                    println!("ERROR: {}", msg);
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(msg) => {
                    eprintln!(
                        "Error while parsing request: {}\tTrying again with more bytes",
                        msg
                    );
                }
            }
        }
    }

    pub fn is_payload_complete<'header, 'buffer>(&self, req: &Request<'header, 'buffer>) -> bool {
        if let Some(res) = req.headers.iter().find(|h| h.name == "Content-Length") {
            self.buffer.len() - self.header_end_index
                == String::from_utf8_lossy(res.value).parse::<usize>().unwrap()
        } else {
            true // no payload so the request is complete
        }
    }
}
