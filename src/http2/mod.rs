const CONNECTION_PRFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub mod frames;
pub mod response;
// pub mod stream;

pub fn check_connection_preface(bytes: &[u8]) -> Option<usize> {
    if bytes.starts_with(CONNECTION_PRFACE) {
        Some(CONNECTION_PRFACE.len())
    } else {
        None
    }
}
