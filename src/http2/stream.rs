use std::collections::HashMap;

use thiserror::Error;
use tracing::{debug, error, trace};

use super::frames::Headers;

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
pub enum StreamError {
    #[error("No space left in the current stream window")]
    NoSpaceLeftInCurrentWindow,
}

#[derive(Debug, Default)]
pub struct Stream {
    pub identifier: u32,
    window_space: u32,
    state: StreamState,
    request_headers: Option<Headers>,
}

impl Stream {
    pub fn new(identifier: u32) -> Self {
        Self {
            identifier,
            window_space: u16::MAX as u32,
            state: StreamState::default(),
            request_headers: None,
        }
    }

    pub fn consume_space(&mut self, space: u32) -> Result<(), StreamError> {
        if (self.window_space as i64 - space as i64) < 0 {
            Err(StreamError::NoSpaceLeftInCurrentWindow)
        } else {
            debug!(
                "Consume window space: (before: {}, after: {})",
                self.window_space,
                self.window_space - space
            );
            self.window_space -= space;
            Ok(())
        }
    }

    pub fn has_room_in_window(&self, size: u32) -> bool {
        debug!(
            "Has room in window: (window_space: {}, size: {})",
            self.window_space, size
        );
        self.window_space >= size
    }

    pub fn update_window(&mut self, space: u32) {
        self.window_space = space;
    }

    pub fn get_headers(&self) -> Option<&Headers> {
        self.request_headers.as_ref()
    }

    pub fn set_headers(&mut self, header: Headers) {
        trace!("Set headers: to stream {}", self.identifier);
        self.request_headers = Some(header);
    }

    pub fn mark_as_closed(&mut self) {
        if self.request_headers.as_ref().unwrap().should_stream_close() {
            trace!("Mark stream {} as closed", self.identifier);
            self.state = StreamState::Closed;
        }
    }
}

#[derive(Debug)]
pub struct StreamManager {
    streams: HashMap<u32, Stream>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
        }
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

    pub fn get_at(&self, index: u32) -> Option<&Stream> {
        match self.streams.get(&index) {
            Some(s) => Some(s),
            None => {
                tracing::error!("Tried to get an unregistered stream {}", index);
                None
            }
        }
    }

    pub fn get_at_mut(&mut self, index: u32) -> Option<&mut Stream> {
        match self.streams.get_mut(&index) {
            Some(s) => Some(s),
            None => {
                tracing::error!("Tried to get an unregistered stream {}", index);
                None
            }
        }
    }

    pub fn register_new_stream(&mut self, index: u32) {
        trace!("Register new stream at index: {index}");
        self.streams.insert(index, Stream::new(index));
    }
}
