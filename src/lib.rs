use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use std::sync::atomic::{AtomicU32, Ordering};
use futures_util::SinkExt;
use futures_util::stream::SplitSink;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::Utf8Bytes;

mod websocket;
// 引入 WebSocket.rs

// 连接对象
pub struct Connection {
    conn_id: u32,
    uid: u32,
    ws_stream: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>,
}

impl Connection {
    pub fn new(conn_id: u32) -> Self {
        Connection {
            conn_id,
            uid: conn_id, // 初始化时，uid 与 conn_id 一致
            ws_stream: None, // 默认没有 WebSocket 流
        }
    }

    pub fn id(&self) -> u32 {
        self.conn_id
    }

    // 修改 uid
    pub fn set_uid(&mut self, new_uid: u32) {
        self.uid = new_uid;
    }

    // 获取 uid
    pub fn get_uid(&self) -> u32 {
        self.uid
    }

    // 设置 WebSocket 连接流
    pub fn set_ws_stream(
        &mut self,
        ws_stream: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>
    ) {
        self.ws_stream = Some(ws_stream);
    }

    // 发送消息
    pub async fn send_message(&mut self, msg: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut ws_stream) = self.ws_stream {
            ws_stream.send(Message::Text(Utf8Bytes::from(msg.to_string()))).await?;
            Ok(())
        } else {
            Err("WebSocket stream not available".into())
        }
    }
}

// 接收到的消息
pub struct ReceivedMessage {
    pub content: String,
    pub connection: Arc<Mutex<Connection>>,
}

impl ReceivedMessage {
    pub async fn reply(&mut self, msg: &str) {
        let mut conn = self.connection.lock().await;
        if let Err(e) = conn.send_message(msg).await {
            eprintln!("发送消息失败: {}", e);
        }
    }
}

// 连接管理
pub struct Connectify {
    counter: AtomicU32,
    pub connections: Arc<Mutex<HashMap<u32, Arc<Mutex<Connection>>>>>,
    sender: mpsc::Sender<ReceivedMessage>,
    pub receiver: Arc<Mutex<mpsc::Receiver<ReceivedMessage>>>,
}

impl Connectify {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(1000);
        Connectify {
            counter: AtomicU32::new(1),
            connections: Arc::new(Mutex::new(HashMap::new())),
            sender: tx,
            receiver: Arc::new(Mutex::new(rx)),
        }
    }

    pub async fn ws_new(
        &self,
        url: &str,
        authentication: Option<String>,
        max_retries: Option<u32>,
        retry_delay: Option<Duration>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        websocket::ws_connect(self, url, authentication, max_retries, retry_delay).await
    }

    pub async fn send(&self, conn_id: u32, msg: &str) -> Result<(), Box<dyn std::error::Error>> {
        let connections = self.connections.lock().await;
        if let Some(conn) = connections.get(&conn_id) {
            let mut conn_guard = conn.lock().await;
            conn_guard.send_message(msg).await
        } else {
            Err("连接不存在".into())
        }
    }

}
