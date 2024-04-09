use std::{collections::HashMap, future::Future, sync::Arc};

use raptor_core::{config, method_handlers, response};
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinSet,
};
use tracing::{debug, trace};

use crate::frames::{errors::FrameResult, headers::Headers};

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
    Header(Headers),
}

pub type StreamReceiver = Receiver<Arc<StreamFrame>>;
pub type StreamSender = Sender<Arc<response::Response>>;

pub struct Stream {
    pub id: u32,
    state: StreamState,
    conf: Arc<config::Config>,
    rx: Option<StreamReceiver>,
    tx: Option<StreamSender>,
}

impl Stream {
    pub fn new(id: u32, conf: Arc<config::Config>) -> Self {
        Self {
            id,
            state: StreamState::Idle,
            conf,
            rx: None,
            tx: None,
        }
    }

    pub fn setup_channels(&mut self, rx: StreamReceiver) -> Receiver<Arc<response::Response>> {
        self.rx = Some(rx);
        let (tx, rx) = mpsc::channel(100);
        self.tx = Some(tx);

        rx
    }

    pub async fn do_stream_job(&mut self) -> FrameResult<()> {
        loop {
            let Some(frame) = self.rx.as_mut().unwrap().recv().await else {
                trace!("Failed to receive data from connection. Is the connection closed?");
                break;
            };

            match frame.as_ref() {
                StreamFrame::PushPromise => {
                    self.state = StreamState::ReservedRemote;
                }
                StreamFrame::Header(headers) if headers.has_end_headers() => {
                    debug!("Got frame headers: {headers:?}");
                    let response =
                        Arc::new(method_handlers::handle_request(headers, &self.conf).await);

                    if let Err(_) = self.tx.as_mut().unwrap().send(response).await {
                        trace!("Failed to send data to main thread. Is the connection closed?");
                    }

                    break;
                }
                StreamFrame::Header(_) => todo!("Continuation frame"),
            }
        }

        Ok(())
    }
}

pub struct StreamManager {
    streams: HashMap<u32, Sender<Arc<StreamFrame>>>,
    data_receiver: JoinSet<Option<Arc<response::Response>>>,
}

impl Default for StreamManager {
    fn default() -> Self {
        Self {
            streams: HashMap::with_capacity(MAX_CONCURRENT_STREAM),
            data_receiver: JoinSet::new(),
        }
    }
}

impl StreamManager {
    pub fn register_new_stream_if_needed(&mut self, id: u32, conf: Arc<config::Config>) {
        if self.streams.contains_key(&id) {
            trace!("Stream {id} already registered. Skipping.");
            return;
        }

        let (tx, rx) = mpsc::channel::<Arc<StreamFrame>>(MAX_CONCURRENT_STREAM);
        let mut stream = Stream::new(id, conf);
        let mut receiver = stream.setup_channels(rx);

        // Spawn a future to poll if any data has been sent by the stream
        self.data_receiver
            .spawn(async move { receiver.recv().await });

        self.streams.insert(id, tx);

        tokio::spawn(async move { stream.do_stream_job().await });
    }

    pub fn is_empty(&self) -> bool {
        self.data_receiver.is_empty()
    }

    pub async fn poll_data(&mut self) -> Option<Arc<response::Response>> {
        self.data_receiver
            .join_next()
            .await
            .expect("Set is empty")
            .expect("Failed to join future")
    }

    pub async fn send_frame_to_stream(&mut self, stream_frame: Arc<StreamFrame>, id: u32) {
        let sender = self.streams.get_mut(&id).expect("Should be present");
        sender
            .send(stream_frame)
            .await
            .expect("Failed to send to stream");
    }
}
