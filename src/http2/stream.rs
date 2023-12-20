use std::{cmp::min, collections::HashMap, sync::Arc};

use bytes::{Buf, Bytes, BytesMut};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tracing::{debug, error, trace};

use crate::{
    config::Config,
    connection::{send_all, ConnectionError, ConnectionResult},
    method_handlers::handle_request,
    request::{HttpRequest, RequestType},
};

use super::{
    frames::{self, FrameError, Headers, FRAME_HEADER_LENGTH},
    response::build_frame_header,
};

pub(crate) const INITIAL_WINDOW_SIZE: i64 = 65535;
pub(crate) const MAX_WINDOW_SIZE: i64 = (1 << 31) - 1; // 2**31 - 1

#[derive(Debug, PartialEq)]
enum StreamState {
    Idle,
    // TODO: Support Push promise
    // ReservedLocal,
    // ReservedRemote,
    // HalfClosedLocal,
    // HalfClosedRemote,
    Closed,
}

impl Default for StreamState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Error, Debug, Clone)]
pub enum StreamError {}

#[derive(Debug, Default)]
pub struct Stream {
    pub identifier: u32,
    window_space: i64,
    state: StreamState,
    request_headers: Option<Headers>,
    data_to_send: Option<Bytes>,
}

impl Stream {
    pub fn new(identifier: u32, initial_window_size: i64) -> Self {
        Self {
            identifier,
            window_space: initial_window_size,
            state: StreamState::default(),
            request_headers: None,
            data_to_send: None,
        }
    }

    pub fn set_window_space(&mut self, size: i64) {
        self.window_space = size;
    }

    pub fn update_window(&mut self, space: i64) -> Result<(), FrameError> {
        debug!(
            "stream {}: value after update {} (valid: {})",
            self.identifier,
            self.window_space + space,
            self.window_space + space >= MAX_WINDOW_SIZE
        );
        if self.window_space + space >= MAX_WINDOW_SIZE {
            error!(
                "stream {}: Window increase is too big {}",
                self.identifier,
                self.window_space + space
            );
            return Err(FrameError::WindowUpdateTooBig);
        }

        self.window_space += space;
        Ok(())
    }

    pub fn has_data_to_send(&self) -> bool {
        self.data_to_send.is_some()
    }

    pub fn set_headers(&mut self, header: Headers) {
        trace!("Set headers: to stream {}", self.identifier);
        self.request_headers = Some(header);
    }

    fn mark_as_closed(&mut self) {
        if self.request_headers.as_ref().unwrap().should_stream_close() {
            trace!("Mark stream {} as closed", self.identifier);
            self.state = StreamState::Closed;
        }
    }

    pub async fn respond_to(
        &mut self,
        stream: &mut TlsStream<TcpStream>,
        max_frame_size: usize,
        global_window_size: &mut i64,
        config: &Arc<Config>,
        encoder: &mut hpack::Encoder<'_>,
    ) -> ConnectionResult<()> {
        let headers = self.request_headers.as_ref().unwrap();

        match headers.get_type().map_err(|e| {
            debug!("Bad request received: {:?}", e);
            ConnectionError::BadRequest
        })? {
            RequestType::Get => {
                let mut response = handle_request(headers, config).await;
                let mut serialize_buffer = BytesMut::with_capacity(8192);
                let mut headers_vec = vec![(
                    Bytes::from_static(b":status"),
                    Bytes::copy_from_slice(response.code.to_string().as_bytes()),
                )];
                headers_vec.append(&mut response.headers); // Cheap because of Bytes
                let header_frame = frames::Headers::new(&headers_vec, response.body.is_none());
                build_frame_header(
                    &mut serialize_buffer,
                    frames::FrameType::Headers,
                    self.identifier,
                    &header_frame,
                    Some(encoder),
                );

                trace!("Send response header frame: {:?}", header_frame);
                send_all(stream, serialize_buffer.as_ref()).await?;

                if let Some(body) = response.body {
                    self.data_to_send = Some(body);
                    self.try_send_data_payload(stream, global_window_size, max_frame_size)
                        .await?;
                }
            }
            _ => todo!("Method not implemented"),
        };

        Ok(())
    }

    /// This function tries to send response data the client according to
    /// the window size. It will send as much data frame as the current window
    /// space allow.
    pub async fn try_send_data_payload(
        &mut self,
        stream: &mut TlsStream<TcpStream>,
        global_window_size: &mut i64,
        max_frame_size: usize,
    ) -> ConnectionResult<()> {
        let data_frame_count = self
            .data_to_send
            .as_ref()
            .expect("call Stream::try_send_data_payload without data to send")
            .chunks(max_frame_size)
            .count();
        let mut data_sent = 0usize;
        let mut window_space = self.window_space;

        if window_space == 0 || *global_window_size == 0 {
            trace!(
                "stream {}: Can't receive data yet window is empty (local_window: {}, global_window: {})",
                self.identifier,
                window_space,
                global_window_size
            );
            return Ok(());
        }

        trace!(
            "stream: {} Try to send payload: max_frame_size: {max_frame_size} local_window_size: {:?} global_window_size: {} remaining_bytes: {}",
            self.identifier,
            self.window_space,
            global_window_size,
            self.data_to_send.as_ref().unwrap().len(),
        );

        for (i, chunk) in self
            .data_to_send
            .as_ref()
            .unwrap()
            .chunks(min(max_frame_size, self.window_space as usize))
            .enumerate()
        {
            trace!(
                "stream {}: window_space: {window_space:?} chunk_len: {} data_frame_count: {data_frame_count} i: {i}",
                self.identifier,
                chunk.len()
            );

            if (window_space as usize) < chunk.len() || (*global_window_size as usize) < chunk.len()
            {
                trace!(
                    "stream {}: Not enough space in windows yet (needed: {}, local_current_size: {} global_current_size: {})",
                    self.identifier,
                    chunk.len(),
                    window_space,
                    *global_window_size
                );
                self.data_to_send.as_mut().unwrap().advance(data_sent);
                self.window_space = window_space;
                return Ok(());
            } else {
                window_space -= chunk.len() as i64;
                *global_window_size -= chunk.len() as i64;
            }

            let mut data_frame = frames::Data::new(Bytes::copy_from_slice(chunk));
            let mut data_frame_buffer = BytesMut::with_capacity(FRAME_HEADER_LENGTH + chunk.len());

            data_frame.set_flags(
                if self.data_to_send.as_ref().unwrap().len() - (data_sent + chunk.len()) == 0 {
                    0x01 // close stream it's the last data frame we'll send
                } else {
                    0x00
                },
            );

            debug!(
                "stream {}: Data frame flags {}",
                self.identifier, data_frame.flags
            );
            build_frame_header(
                &mut data_frame_buffer,
                frames::FrameType::Data,
                self.identifier,
                &data_frame,
                None,
            );

            send_all(stream, data_frame_buffer.as_ref()).await?;
            data_sent += chunk.len();
            trace!(
                "stream {}: Send data frame {}/{} remaining bytes: {}",
                self.identifier,
                i + 1,
                data_frame_count,
                self.data_to_send.as_ref().unwrap().len() - data_sent
            );
        }

        self.data_to_send.as_mut().unwrap().advance(data_sent);
        // At this point all the payload has been sent to the client.
        // We can now close the stream and set the remaining payload to send as
        // None
        assert!(!self.data_to_send.as_ref().unwrap().has_remaining());
        self.data_to_send = None;
        debug!("Stream {}: sent {}", self.identifier, data_sent);
        self.mark_as_closed();

        Ok(())
    }

    pub fn reset(&mut self, initial_window_size: i64) {
        self.window_space = initial_window_size;
        self.mark_as_closed();
        self.request_headers = None;

        if self.data_to_send.is_some() {
            self.data_to_send = None;
        }
    }
}

#[derive(Debug)]
pub struct StreamManager {
    streams: HashMap<u32, Stream>,
    initial_window_size: i64,
}

impl StreamManager {
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
            initial_window_size: INITIAL_WINDOW_SIZE,
        }
    }

    pub fn set_initial_window_size(&mut self, size: i64) {

        for stream in self.streams.values_mut() {
            stream.set_window_space(size - self.initial_window_size);
        }

        self.initial_window_size = size;
    }

    pub async fn try_send_data_all_stream(
        &mut self,
        tls_stream: &mut TlsStream<TcpStream>,
        global_window_size: &mut i64,
        max_frame_size: usize,
    ) -> ConnectionResult<()> {
        for stream in self.streams.iter_mut() {
            if stream.1.has_data_to_send() {
                stream
                    .1
                    .try_send_data_payload(tls_stream, global_window_size, max_frame_size)
                    .await?;
            }
        }

        Ok(())
    }

    // pub fn create_new_stream(&mut self) {
    //     let max_id = self.streams.keys().max().unwrap_or(&2);
    //     let new_entry_id = if max_id % 2 == 1 {
    //         max_id + 1
    //     } else {
    //         max_id + 2
    //     };
    //
    //     debug!("Registered a new stream with id {}", new_entry_id);
    //     self.streams.insert(new_entry_id, Stream::new(new_entry_id));
    // }

    pub fn has_stream(&self, index: u32) -> bool {
        self.streams.contains_key(&index)
    }

    pub fn get_at_mut(&mut self, index: u32) -> Option<&mut Stream> {
        match self.streams.get_mut(&index) {
            Some(s) => Some(s),
            None => {
                tracing::warn!("Tried to get an unregistered stream {}", index);
                None
            }
        }
    }

    pub fn register_new_stream(&mut self, index: u32) {
        trace!("Register new stream at index: {index}");
        self.streams
            .insert(index, Stream::new(index, self.initial_window_size));
    }

    pub fn get_initial_window_size(&self) -> i64 {
        self.initial_window_size
    }
}
