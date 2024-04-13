use std::{
    cmp,
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use raptor_core::{config, method_handlers};
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};
use tracing::{debug, error, trace};

use crate::{
    frames::{
        self,
        errors::FrameResult,
        headers::Headers,
        window_update::{WindowUpdate, DEFAULT_WINDOW_SIZE},
        Frame, FrameType, SerializeFrame,
    },
    utils,
};

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
    WindowUpdate(WindowUpdate),
}

pub type StreamReceiver = Receiver<Arc<StreamFrame>>;
pub type StreamSender = Sender<Arc<Vec<u8>>>;

pub struct Stream {
    pub id: u32,
    state: StreamState,
    flow_control: u32,
    global_flow_control: Arc<AtomicU32>,
    encoder: Arc<Mutex<fluke_hpack::Encoder<'static>>>,
    conf: Arc<config::Config>,
    rx: Option<StreamReceiver>,
    tx: Option<StreamSender>,
    to_send: Option<(usize, Vec<u8>)>,
}

impl Stream {
    pub fn new(
        id: u32,
        conf: Arc<config::Config>,
        flow_control: u32,
        global_flow_control: Arc<AtomicU32>,
        encoder: Arc<Mutex<fluke_hpack::Encoder<'static>>>,
    ) -> Self {
        Self {
            id,
            state: StreamState::Idle,
            encoder,
            conf,
            flow_control,
            global_flow_control,
            rx: None,
            tx: None,
            to_send: None,
        }
    }

    pub fn setup_channels(&mut self, rx: StreamReceiver, tx: Sender<Arc<Vec<u8>>>) {
        self.rx = Some(rx);
        self.tx = Some(tx);
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
                    let response = method_handlers::handle_request(headers, &self.conf).await;
                    // TODO: Remove Bytes usage
                    let headers = Headers::new(
                        0x04 | if response.body.is_none() { 0x01 } else { 0x00 },
                        [(
                            b":status".to_vec(),
                            response.code.to_string().as_bytes().to_vec(),
                        )]
                        .into_iter()
                        .chain(
                            response
                                .headers
                                .iter()
                                .map(|(k, v)| (k.to_vec(), v.to_vec())),
                        )
                        .collect(),
                    );

                    if let Some(body) = response.body {
                        // TODO: Remove `Bytes` and avoid copy
                        self.to_send = Some((0, body.to_vec()));
                    }

                    let mut frame = Frame {
                        frame_type: FrameType::Header,
                        stream_id: self.id,
                        ..Default::default()
                    };

                    trace!("Send header to the connection thread");
                    if self
                        .tx
                        .as_ref()
                        .unwrap()
                        .send(
                            utils::frame_to_bytes(&mut frame, headers, Some(self.encoder.clone()))
                                .await
                                .into(),
                        )
                        .await
                        .is_err()
                    {
                        trace!("Failed to send data to main thread. Is the connection closed?");
                    }

                    if self.to_send.is_none() {
                        break;
                    }

                    self.send_data_within_control_flow().await
                }
                StreamFrame::WindowUpdate(window_update) => {
                    self.flow_control += window_update.size_increment;
                    debug!(
                        "Stream: {} has a flow control window of {} bytes",
                        self.id, self.flow_control
                    );

                    if self.to_send.is_some() {
                        self.send_data_within_control_flow().await;
                    }
                }
                StreamFrame::Header(_) => todo!("Continuation frame"),
            }
        }

        Ok(())
    }

    async fn send_data_within_control_flow(&mut self) {
        if self.to_send.is_none() {
            return;
        }

        let (sent, ref body) = self.to_send.as_mut().expect("unreachable");
        let to_send = &body[*sent..];
        let mut frame = Frame {
            stream_id: self.id,
            frame_type: FrameType::Data,
            ..Default::default()
        };

        for chunk in to_send.chunks(cmp::min(
            self.flow_control,
            self.global_flow_control.load(Ordering::Relaxed),
        ) as usize)
        {
            if chunk.len() > self.flow_control as usize
                || chunk.len() > self.global_flow_control.load(Ordering::Relaxed) as usize
            {
                debug!("Can't send more data. Pausing frame sending.");
                break;
            }

            let data = frames::data::Data {
                flags: 0x01,
                // TODO: Fix copy
                data: chunk.to_vec(),
            };

            trace!("Send chunk of data of size {}", chunk.len());
            self.flow_control -= chunk.len() as u32;
            self.global_flow_control
                .fetch_sub(chunk.len() as u32, Ordering::Relaxed);

            let send_bytes = utils::frame_to_bytes(&mut frame, data, None).await;
            if self
                .tx
                .as_ref()
                .unwrap()
                .send(Arc::new(send_bytes))
                .await
                .is_err()
            {
                error!("Failed to send data to connection thread");
                return;
            }
        }
    }
}

pub struct StreamManager {
    streams: HashMap<u32, Sender<Arc<StreamFrame>>>,
    stream_data_receiver: Receiver<Arc<Vec<u8>>>,
    stream_data_sender: Sender<Arc<Vec<u8>>>,
}

impl Default for StreamManager {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel(MAX_CONCURRENT_STREAM);

        Self {
            streams: HashMap::with_capacity(MAX_CONCURRENT_STREAM),
            stream_data_receiver: rx,
            stream_data_sender: tx,
        }
    }
}

impl StreamManager {
    pub fn register_new_stream_if_needed(
        &mut self,
        id: u32,
        conf: Arc<config::Config>,
        encoder: Arc<Mutex<fluke_hpack::Encoder<'static>>>,
        global_flow_control: Arc<AtomicU32>,
    ) {
        if self.streams.contains_key(&id) {
            trace!("Stream {id} already registered. Skipping.");
            return;
        }

        let (tx, rx) = mpsc::channel::<Arc<StreamFrame>>(MAX_CONCURRENT_STREAM);
        let mut stream = Stream::new(
            id,
            conf,
            DEFAULT_WINDOW_SIZE,
            global_flow_control,
            encoder.clone(),
        );

        stream.setup_channels(rx, self.stream_data_sender.clone());
        self.streams.insert(id, tx);

        tokio::spawn(async move { stream.do_stream_job().await });
    }

    pub async fn poll_data(&mut self) -> Option<Arc<Vec<u8>>> {
        self.stream_data_receiver.recv().await
    }

    pub async fn send_frame_to_stream(&mut self, stream_frame: Arc<StreamFrame>, id: u32) {
        let sender = self.streams.get_mut(&id).expect("Should be present");
        sender
            .send(stream_frame)
            .await
            .expect("Failed to send to stream");
    }
}
