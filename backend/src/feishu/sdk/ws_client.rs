use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, trace};
use prost::Message as ProstMessage;
use serde::Deserialize;
use serde_json::json;
use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::mpsc,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{frame::coding::CloseCode, Message},
    MaybeTlsStream, WebSocketStream,
};
use url::Url;

use super::{
    api_types::BaseResponse,
    cache::QuickCache,
    config::{Config, FEISHU_BASE_URL},
    event::EventDispatcherHandler,
};

// --- Proto types (inline prost definitions) ---

#[derive(Clone, PartialEq, prost::Message)]
pub struct Header {
    #[prost(string, required, tag = "1")]
    pub key: String,
    #[prost(string, required, tag = "2")]
    pub value: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Frame {
    #[prost(uint64, required, tag = "1")]
    pub seq_id: u64,
    #[prost(uint64, required, tag = "2")]
    pub log_id: u64,
    #[prost(int32, required, tag = "3")]
    pub service: i32,
    #[prost(int32, required, tag = "4")]
    pub method: i32,
    #[prost(message, repeated, tag = "5")]
    pub headers: Vec<Header>,
    #[prost(string, optional, tag = "6")]
    pub payload_encoding: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub payload_type: Option<String>,
    #[prost(bytes = "vec", optional, tag = "8")]
    pub payload: Option<Vec<u8>>,
    #[prost(string, optional, tag = "9")]
    pub log_id_new: Option<String>,
}

// --- State machine ---

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Initial,
    Connecting,
    Connected,
    Disconnecting,
    Disconnected { reason: Option<CloseReason> },
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CloseReason {
    pub code: u16,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub enum StateMachineEvent {
    StartConnection,
    ConnectionEstablished,
    DataReceived,
    RequestDisconnect,
    ConnectionClosed(Option<CloseReason>),
    ErrorOccurred(String),
}

pub struct WebSocketStateMachine {
    state: ConnectionState,
}

impl WebSocketStateMachine {
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Initial,
        }
    }

    pub fn current_state(&self) -> &ConnectionState {
        &self.state
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn handle_event(&mut self, event: StateMachineEvent) -> Result<(), String> {
        use ConnectionState::*;
        use StateMachineEvent::*;

        let new_state = match (&self.state, event.clone()) {
            (Initial, StartConnection) => Connecting,
            (Connecting, ConnectionEstablished) => Connected,
            (Connected, DataReceived) => Connected,
            (Connected, RequestDisconnect) => Disconnecting,
            (Disconnecting, ConnectionClosed(reason)) => Disconnected { reason },
            (Connected, ConnectionClosed(reason)) => Disconnected { reason },
            (_, ErrorOccurred(msg)) => Error { message: msg },
            _ => {
                return Err(format!(
                    "Invalid state transition from {:?} with event {:?}",
                    self.state, event
                ));
            }
        };

        self.state = new_state;
        Ok(())
    }

    pub fn can_send_data(&self) -> bool {
        matches!(self.state, ConnectionState::Connected)
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.state, ConnectionState::Connected)
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(
            self.state,
            ConnectionState::Disconnected { .. } | ConnectionState::Error { .. }
        )
    }
}

impl Default for WebSocketStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

// --- Frame handler ---

pub struct FrameHandler;

impl FrameHandler {
    pub async fn handle_frame(
        frame: Frame,
        event_handler: &EventDispatcherHandler,
    ) -> Option<Frame> {
        match frame.method {
            0 => Self::handle_control_frame(frame),
            1 => Self::handle_data_frame(frame, event_handler).await,
            _ => {
                error!("Unknown frame method: {}", frame.method);
                None
            }
        }
    }

    fn handle_control_frame(frame: Frame) -> Option<Frame> {
        let frame_type = Self::get_header_value(&frame.headers, "type")?;
        trace!("Received control frame: {frame_type}");

        match frame_type.as_str() {
            "pong" => Some(frame),
            _ => None,
        }
    }

    async fn handle_data_frame(
        mut frame: Frame,
        event_handler: &EventDispatcherHandler,
    ) -> Option<Frame> {
        let msg_type = Self::get_header_value(&frame.headers, "type").unwrap_or_default();
        let msg_id = Self::get_header_value(&frame.headers, "message_id").unwrap_or_default();
        let trace_id = Self::get_header_value(&frame.headers, "trace_id").unwrap_or_default();

        let Some(payload) = frame.payload else {
            error!("Data frame missing payload");
            return None;
        };

        debug!(
            "Received data frame - type: {msg_type}, message_id: {msg_id}, trace_id: {trace_id}"
        );

        match msg_type.as_str() {
            "event" => {
                let start = Instant::now();
                match event_handler.do_without_validation(&payload) {
                    Ok(_) => {
                        let elapsed = start.elapsed().as_millis();
                        let response = NewWsResponse::ok();
                        frame.payload = Some(serde_json::to_vec(&response).unwrap_or_default());
                        frame.headers.push(Header {
                            key: "biz_rt".to_string(),
                            value: elapsed.to_string(),
                        });
                        Some(frame)
                    }
                    Err(err) => {
                        error!("Failed to handle event: {err:?}");
                        let response = NewWsResponse::ok();
                        frame.payload = Some(serde_json::to_vec(&response).unwrap_or_default());
                        Some(frame)
                    }
                }
            }
            _ => None,
        }
    }

    fn get_header_value(headers: &[Header], key: &str) -> Option<String> {
        headers
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.value.clone())
    }

    pub fn build_ping_frame(service_id: i32) -> Frame {
        Frame {
            seq_id: 0,
            log_id: 0,
            service: service_id,
            method: 0,
            headers: vec![Header {
                key: "type".to_string(),
                value: "ping".to_string(),
            }],
            payload_encoding: None,
            payload_type: None,
            payload: None,
            log_id_new: None,
        }
    }
}

#[derive(serde::Serialize, Deserialize, Debug)]
struct NewWsResponse {
    code: u16,
    headers: std::collections::HashMap<String, String>,
    data: Vec<u8>,
}

impl NewWsResponse {
    fn ok() -> Self {
        Self {
            code: 200,
            headers: Default::default(),
            data: Default::default(),
        }
    }
}

// --- WsClient ---

const END_POINT_URL: &str = "/callback/ws/endpoint";
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct LarkWsClient {
    frame_tx: mpsc::UnboundedSender<Frame>,
    event_rx: mpsc::UnboundedReceiver<WsEvent>,
    cache: QuickCache<Vec<Vec<u8>>>,
    state_machine: WebSocketStateMachine,
}

impl LarkWsClient {
    pub async fn open(
        config: std::sync::Arc<Config>,
        event_handler: EventDispatcherHandler,
    ) -> WsClientResult<()> {
        let end_point = get_conn_url(&config).await?;
        let conn_url = end_point.url.ok_or(WsClientError::UnexpectedResponse)?;
        let client_config = end_point
            .client_config
            .ok_or(WsClientError::UnexpectedResponse)?;
        let url = Url::parse(&conn_url)?;
        let query_pairs: HashMap<_, _> = url.query_pairs().into_iter().collect();
        let service_id: i32 = query_pairs
            .get("service_id")
            .ok_or(WsClientError::UnexpectedResponse)?
            .parse()
            .map_err(|_| WsClientError::UnexpectedResponse)?;

        let mut state_machine = WebSocketStateMachine::new();
        let _ = state_machine.handle_event(StateMachineEvent::StartConnection);

        let (conn, _response) = connect_async(conn_url).await?;
        info!("connected to {url}");

        let _ = state_machine.handle_event(StateMachineEvent::ConnectionEstablished);
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        tokio::spawn(client_loop(
            service_id,
            client_config,
            conn,
            frame_rx,
            event_tx,
        ));
        let mut client = LarkWsClient {
            frame_tx,
            event_rx,
            cache: QuickCache::new(),
            state_machine,
        };

        client.handler_loop(event_handler).await;

        Ok(())
    }

    async fn handler_loop(&mut self, event_handler: EventDispatcherHandler) {
        while let Some(ws_event) = self.event_rx.recv().await {
            if let WsEvent::Data(frame) = ws_event {
                let _ = self.state_machine.handle_event(StateMachineEvent::DataReceived);

                if !self.state_machine.can_send_data() {
                    continue;
                }

                let processed_frame = self.process_frame_packages_internal(frame).await;
                let Some(frame) = processed_frame else {
                    continue;
                };

                if let Some(response_frame) =
                    FrameHandler::handle_frame(frame, &event_handler).await
                {
                    if let Err(e) = self.frame_tx.send(response_frame) {
                        error!("Failed to send response frame: {e:?}");
                    }
                }
            }
        }
    }

    async fn process_frame_packages_internal(&mut self, mut frame: Frame) -> Option<Frame> {
        let headers: &[Header] = frame.headers.as_ref();

        let sum: usize = headers
            .iter()
            .find(|h| h.key == "sum")
            .and_then(|h| h.value.parse().ok())
            .unwrap_or(1);

        let seq: usize = headers
            .iter()
            .find(|h| h.key == "seq")
            .and_then(|h| h.value.parse().ok())
            .unwrap_or(0);

        let msg_id = headers
            .iter()
            .find(|h| h.key == "message_id")
            .map(|h| h.value.as_str())
            .unwrap_or("");

        let Some(payload) = frame.payload.take() else {
            error!("Frame payload is empty");
            return None;
        };

        if sum > 1 {
            match self.combine(msg_id, sum, seq, &payload) {
                Some(combined_payload) => {
                    frame.payload = Some(combined_payload);
                }
                None => {
                    return None;
                }
            }
        } else {
            frame.payload = Some(payload);
        }

        Some(frame)
    }

    fn combine(&mut self, msg_id: &str, sum: usize, seq: usize, bs: &[u8]) -> Option<Vec<u8>> {
        let val = self.cache.get(msg_id);
        if val.is_none() {
            let mut buf = vec![Vec::new(); sum];
            buf[seq] = bs.to_vec();
            self.cache.set(msg_id, buf, 5);
            return None;
        }

        let mut val = val?;
        val[seq] = bs.to_vec();
        let mut pl = Vec::new();
        for v in val.iter() {
            if v.is_empty() {
                self.cache.set(msg_id, val, 5);
                return None;
            }
            pl.extend_from_slice(v);
        }

        Some(pl)
    }
}

// --- Connection endpoint ---

async fn get_conn_url(config: &std::sync::Arc<Config>) -> WsClientResult<EndPointResponse> {
    let body = json!({
        "AppID": &config.app_id,
        "AppSecret": &config.app_secret
    });

    let req = config
        .http_client
        .post(format!("{FEISHU_BASE_URL}/{END_POINT_URL}"))
        .header("locale", "zh")
        .json(&body)
        .send()
        .await?;

    let resp = req.json::<BaseResponse<EndPointResponse>>().await?;

    if !resp.success() {
        return match resp.code() {
            1 | 1000040343 => Err(WsClientError::ServerError {
                code: resp.code(),
                message: resp.msg().to_string(),
            }),
            _ => Err(WsClientError::ClientError {
                code: resp.code(),
                message: resp.msg().to_string(),
            }),
        };
    }

    let end_point = resp.data.ok_or(WsClientError::UnexpectedResponse)?;
    // 使用 map_or 替代 is_none_or 以兼容 MSRV 1.81（is_none_or 需要 1.82+）
    if end_point.url.as_ref().map_or(true, |url| url.is_empty()) {
        return Err(WsClientError::ServerError {
            code: 500,
            message: "No available endpoint".to_string(),
        });
    }

    Ok(end_point)
}

#[derive(Debug, Deserialize)]
pub struct EndPointResponse {
    #[serde(rename = "URL")]
    pub url: Option<String>,
    #[serde(rename = "ClientConfig")]
    pub client_config: Option<ClientConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    #[serde(rename = "PingInterval")]
    pub ping_interval: i32,
}

// --- Client loop ---

async fn client_loop(
    service_id: i32,
    client_config: ClientConfig,
    conn: WebSocketStream<MaybeTlsStream<TcpStream>>,
    mut frame_tx: mpsc::UnboundedReceiver<Frame>,
    mut event_sender: mpsc::UnboundedSender<WsEvent>,
) {
    let (mut sink, mut stream) = conn.split();
    let mut ping_frame_interval =
        tokio::time::interval(Duration::from_secs(client_config.ping_interval as u64));
    let mut checkout_timeout = tokio::time::interval(Duration::from_secs(1));
    let mut ping_time = Instant::now();

    loop {
        tokio::select! {
            item = stream.next() => {
                match item.transpose() {
                    Ok(Some(msg)) => {
                        if msg.is_ping() {
                            ping_time = Instant::now();
                        }
                        if let Err(e) = handle_message(msg, &mut sink, &mut event_sender, service_id).await {
                            let _ = event_sender.send(WsEvent::Error(e));
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = event_sender.send(WsEvent::Error(WsClientError::ConnectionClosed { reason: None }));
                        break;
                    }
                    Err(e) => {
                        let _ = event_sender.send(WsEvent::Error(e.into()));
                        break;
                    }
                }
            }
            item = frame_tx.recv() => {
                match item {
                    Some(frame) => {
                        let msg = Message::Binary(frame.encode_to_vec());
                        if let Err(e) = sink.send(msg).await {
                            error!("Failed to send frame: {e:?}");
                        }
                    }
                    None => break,
                }
            }
            _ = ping_frame_interval.tick() => {
                let frame = FrameHandler::build_ping_frame(service_id);
                let msg = Message::Binary(frame.encode_to_vec());
                trace!("Sending ping message: service_id={service_id}");
                if let Err(e) = sink.send(msg).await {
                    error!("Failed to send ping: {e:?}");
                    break;
                }
            }
            _ = checkout_timeout.tick() => {
                if (Instant::now() - ping_time) > HEARTBEAT_TIMEOUT {
                    let _ = event_sender.send(WsEvent::Error(WsClientError::ConnectionClosed { reason: None }));
                    break;
                }
            }
        }
    }
}

async fn handle_message(
    msg: Message,
    sink: &mut futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    event_sender: &mut mpsc::UnboundedSender<WsEvent>,
    _service_id: i32,
) -> WsClientResult<()> {
    match msg {
        Message::Ping(data) => {
            sink.send(Message::Pong(data)).await?;
        }
        Message::Binary(data) => {
            let frame = Frame::decode(&*data)?;
            trace!("Received frame: {frame:?}");

            match frame.method {
                0 => {
                    // Control frame - handle pong locally
                    let frame_type = frame
                        .headers
                        .iter()
                        .find(|h| h.key == "type")
                        .map(|h| h.value.as_str())
                        .unwrap_or("");
                    if frame_type == "pong" {
                        if let Some(payload) = &frame.payload {
                            if let Ok(config) = serde_json::from_slice::<ClientConfig>(payload) {
                                debug!("Received pong with config: ping_interval={}", config.ping_interval);
                            }
                        }
                    }
                }
                1 => {
                    if let Err(e) = event_sender.send(WsEvent::Data(frame)) {
                        error!("Failed to send data event: {e:?}");
                    }
                }
                _ => {}
            }
        }
        Message::Close(Some(close_frame)) => {
            return Err(WsClientError::ConnectionClosed {
                reason: Some(WsCloseReason {
                    code: close_frame.code,
                    message: close_frame.reason.into_owned(),
                }),
            });
        }
        _ => return Err(WsClientError::UnexpectedResponse),
    }
    Ok(())
}

// --- Errors and events ---

pub type WsClientResult<T> = Result<T, WsClientError>;

#[derive(Debug, Error)]
pub enum WsClientError {
    #[error("unexpected response")]
    UnexpectedResponse,
    #[error("Request error: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("Url parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Server error: {code}, {message}")]
    ServerError { code: i32, message: String },
    #[error("Client error: {code}, {message}")]
    ClientError { code: i32, message: String },
    #[error("connection closed")]
    ConnectionClosed {
        reason: Option<WsCloseReason>,
    },
    #[error("WebSocket error: {0}")]
    WsError(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("Prost error: {0}")]
    ProstError(#[from] prost::DecodeError),
}

impl From<tokio_tungstenite::tungstenite::Error> for WsClientError {
    fn from(error: tokio_tungstenite::tungstenite::Error) -> Self {
        WsClientError::WsError(Box::new(error))
    }
}

#[derive(Debug)]
pub enum WsEvent {
    Error(WsClientError),
    Data(Frame),
}

#[derive(Debug)]
pub struct WsCloseReason {
    pub code: CloseCode,
    pub message: String,
}
