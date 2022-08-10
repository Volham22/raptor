use bytes::BytesMut;
use httparse::Request;
use tokio::{io::AsyncReadExt, net::TcpStream};

pub struct Connection {
    stream: TcpStream,
    header_end_index: usize,
    buffer: BytesMut,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            header_end_index: 0,
            buffer: BytesMut::new(),
        }
    }

    pub async fn read_request(&mut self) {
        loop {
            let received_bytes_count = self.stream.read_buf(&mut self.buffer).await.unwrap();
            if received_bytes_count == 0 {
                break;
            }

            let mut header = [httparse::EMPTY_HEADER; 10];
            let mut req = Request::new(&mut header);

            match req.parse(&self.buffer) {
                Ok(res) => {
                    if res.is_complete() {
                        self.header_end_index = res.unwrap();
                        if self.is_payload_complete(&req) {
                            println!("Received: {:?}", req);
                            break;
                        }
                    }
                }
                Err(msg) => {
                    eprint!("Error while parsing request: {}", msg);
                    break;
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
