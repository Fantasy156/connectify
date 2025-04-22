use crate::{Connectify, ReceivedMessage};
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream};
use tokio_tungstenite::tungstenite::handshake::client::{generate_key, Request};
use futures_util::{StreamExt, SinkExt};
use std::sync::Arc;
use std::time::Duration;
use http::HeaderValue;
use tokio::sync::Mutex;
use url::Url;

pub async fn ws_connect(
    client: &Connectify,
    url: &str,
    authentication: Option<String>,
    max_retries: Option<u32>,
    retry_delay: Option<Duration>,
) -> Result<(), Box<dyn std::error::Error>> {
    let retry_delay = retry_delay.unwrap_or(Duration::from_secs(5));
    let max_retries = max_retries.unwrap_or(3);
    let mut retries = 0;

    loop {
        let parsed_url = Url::parse(url)?;
        let host = match parsed_url.port() {
            Some(port) => format!("{}:{}", parsed_url.host_str().unwrap(), port),
            None => parsed_url.host_str().unwrap().to_string(),
        };
        // Build WebSocket request
        let mut req_builder = Request::builder()
            .uri(url)
            .header("Host", host)
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", generate_key());

        if let Some(token) = &authentication {
            req_builder = req_builder.header(
                "Authorization",
                HeaderValue::from_str(&format!("Bearer {}", token))?
            );
        }

        let request = req_builder.body(())?;

        match connect_async(request).await {
            Ok((ws_stream, _)) => {
                let (write, mut read) = ws_stream.split();
                let conn_id = client.counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Create and configure the connection
                let connection = Arc::new(Mutex::new(crate::Connection::new(conn_id)));
                {
                    let mut conn = connection.lock().await;
                    conn.set_ws_stream(write); // Set the write half of the stream
                }

                client.connections.lock().await.insert(conn_id, connection.clone());

                let sender = client.sender.clone();
                let read_conn = connection.clone();
                tokio::spawn(async move {
                    while let Some(msg_result) = read.next().await {
                        match msg_result {
                            Ok(Message::Text(text)) => {
                                let _ = sender.send(ReceivedMessage {
                                    content: text.parse().unwrap(),
                                    connection: read_conn.clone(),
                                }).await;
                            }
                            Ok(Message::Close(_)) => break,
                            _ => continue,
                        }
                    }
                });
                break;
            }
            Err(e) => {
                retries += 1;
                if retries >= max_retries {
                    return Err(Box::new(e));
                }
                tokio::time::sleep(retry_delay).await;
            }
        }
    }

    Ok(())
}