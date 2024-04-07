use std::{collections::HashMap, sync::Arc};

use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::trace;

use crate::frames::errors::FrameResult;

pub(crate) const MAX_CONCURRENT_STREAM: usize = 100;

#[derive(Debug, PartialEq)]
enum StreamState {
    Idle,
    ReservedLocal,
    ReservedRemote,
    HalfClosedRemote,
    HalfClosedLocal,
    Closed,
}

pub(crate) enum StreamFrame {
    PushPromise,
}

pub type StreamReceiver = Receiver<Arc<StreamFrame>>;
pub type StreamSender = Sender<Arc<Vec<u8>>>;

pub struct Stream {
    pub id: u32,
    state: StreamState,
    rx: Option<StreamReceiver>,
    tx: Option<StreamSender>,
}

impl Stream {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            state: StreamState::Idle,
            rx: None,
            tx: None,
        }
    }

    pub fn setup_channels(&mut self, rx: StreamReceiver) -> Receiver<Arc<Vec<u8>>> {
        self.rx = Some(rx);
        let (tx, rx) = mpsc::channel(100);
        self.tx = Some(tx);

        rx
    }

    pub async fn do_stream_job(&mut self) -> FrameResult<()> {
        loop {
            let frame = self
                .rx
                .as_mut()
                .unwrap()
                .recv()
                .await
                .expect("Failed to receive stream frame");

            match frame.as_ref() {
                StreamFrame::PushPromise => {
                    self.state = StreamState::ReservedRemote;
                }
            }
        }
    }
}

pub struct StreamManager {
    streams: HashMap<u32, Sender<Arc<StreamFrame>>>,
    data_receiver: Vec<Receiver<Arc<Vec<u8>>>>,
}

impl Default for StreamManager {
    fn default() -> Self {
        Self {
            streams: HashMap::with_capacity(MAX_CONCURRENT_STREAM),
            data_receiver: Vec::with_capacity(MAX_CONCURRENT_STREAM),
        }
    }
}

impl StreamManager {
    pub fn register_new_stream_if_needed(&mut self, id: u32) {
        if self.streams.contains_key(&id) {
            trace!("Stream {id} already registered. Skipping.");
            return;
        }

        let (tx, rx) = mpsc::channel::<Arc<StreamFrame>>(MAX_CONCURRENT_STREAM);
        let mut stream = Stream::new(id);
        self.data_receiver.push(stream.setup_channels(rx));
        self.streams.insert(id, tx);

        tokio::spawn(async move { stream.do_stream_job().await });
    }

    pub async fn send_frame_to_stream(&mut self, stream_frame: Arc<StreamFrame>, id: u32) {
        let sender = self.streams.get_mut(&id).expect("Should be present");
        sender
            .send(stream_frame)
            .await
            .expect("Failed to send to stream");
    }
}
