use thiserror::Error;

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
}

impl Stream {
    pub fn new(identifier: u32) -> Self {
        Self {
            identifier,
            window_space: 0,
            state: StreamState::default(),
        }
    }
    pub fn consume_space(&mut self, space: u32) -> Result<(), StreamError> {
        if (self.window_space as i64 - space as i64) < 0 {
            Err(StreamError::NoSpaceLeftInCurrentWindow)
        } else {
            self.window_space -= space;
            Ok(())
        }
    }

    pub fn update_window(&mut self, space: u32) {
        self.window_space = space;
    }
}

#[derive(Debug)]
pub struct StreamManager {
    streams: Vec<Stream>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self {
            streams: vec![Stream::new(0)],
        }
    }

    pub fn create_new_stream(&mut self) {
        let new_identifier = match self.streams.last() {
            Some(s) => {
                if s.identifier % 2 == 0 {
                    s.identifier + 2
                } else {
                    s.identifier + 1
                }
            }
            None => 2,
        };

        self.ensure_vec_has_capacity(new_identifier as usize);
        self.streams[new_identifier as usize] = Stream::new(new_identifier);
    }

    pub fn get_at(&self, index: usize) -> Option<&Stream> {
        match self.streams.get(index) {
            Some(s) => Some(s),
            None => {
                println!("Tried to get an unregistered stream {}", index);
                None
            }
        }
    }

    pub fn register_new_stream(&mut self, index: u32) {
        self.ensure_vec_has_capacity(index as usize);
        self.streams[index as usize] = Stream::new(index);
    }

    fn ensure_vec_has_capacity(&mut self, index: usize) {
        if self.streams.capacity() <= index {
            self.streams.reserve(5);
        }
    }
}
