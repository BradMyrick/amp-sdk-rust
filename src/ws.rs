//! AMP WebSocket — live event stream with auto-reconnect.
//!
//! The stream reconnects with exponential backoff (1s → 15s cap) whenever
//! the connection drops, re-authenticating with the session token. The
//! receiver stays valid across reconnects: callers hold one `mpsc::Receiver`
//! for the whole lifetime and never see the churn.

use crate::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct AmpWebSocket {
    pub rx: mpsc::Receiver<Value>,
    handle: tokio::task::JoinHandle<()>,
}

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

impl AmpWebSocket {
    /// Connect with auto-reconnect. Fails only if the FIRST connection
    /// attempt fails — after that, drops are retried forever.
    pub async fn connect(server: &str, token: &str) -> Result<Self> {
        let (tx, rx) = mpsc::channel(64);
        let ws_url = Self::url(server, token);
        Self::connect_once(&ws_url).await?; // fail fast on bad URL/token

        let url = ws_url;
        let handle = tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            loop {
                match tokio::time::timeout(Duration::from_secs(15), Self::connect_once(&url)).await {
                    Ok(Ok(ws)) => {
                        backoff = Duration::from_secs(1); // reset on success
                        if Self::pump(ws, &tx).await.is_err() {
                            continue; // channel closed — caller dropped us
                        }
                        break; // pump returned Ok => receiver dropped
                    }
                    Ok(Err(_)) | Err(_) => {
                        // backoff, then retry
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(15));
                    }
                }
            }
        });

        Ok(Self { rx, handle })
    }

    fn url(server: &str, token: &str) -> String {
        server
            .trim_end_matches('/')
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1)
            + &format!("/v1/ws?token={token}")
    }

    async fn connect_once(url: &str) -> Result<WsStream> {
        Ok(tokio_tungstenite::connect_async(url).await?.0)
    }

    /// Pump events until the socket drops or the receiver goes away.
    /// Ok(()) = receiver dropped (stop reconnecting).
    async fn pump(ws: WsStream, tx: &mpsc::Sender<Value>) -> Result<()> {
        use tokio_tungstenite::tungstenite::Message;
        let (mut sink, mut stream) = ws.split();
        let mut ping = tokio::time::interval(Duration::from_secs(25));
        ping.tick().await; // immediate first tick

        loop {
            tokio::select! {
                msg = stream.next() => match msg {
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(v) = serde_json::from_str::<Value>(&t) {
                            if tx.send(v).await.is_err() {
                                return Ok(()); // caller gone
                            }
                        }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        let _ = sink.send(Message::Pong(p)).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => return Err(crate::AmpError::Other("stream dropped".into())),
                },
                _ = ping.tick() => {
                    if sink
                        .send(Message::Text("{\"type\":\"ping\"}".into()))
                        .await
                        .is_err()
                    {
                        return Err(crate::AmpError::Other("stream dropped".into()));
                    }
                }
            }
        }
    }

    /// Await the next event of a specific type (e.g. `"hello"`).
    pub async fn next_event(&mut self, event_type: &str) -> Option<Value> {
        while let Some(v) = self.rx.recv().await {
            if v.get("type").and_then(|t| t.as_str()) == Some(event_type) {
                return Some(v);
            }
        }
        None
    }
}

impl Drop for AmpWebSocket {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
