//! AMP WebSocket — live event stream with typed handlers.

use crate::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// A live connection to the matchmaker event stream.
pub struct AmpWebSocket {
    pub rx: mpsc::Receiver<Value>,
    handle: tokio::task::JoinHandle<()>,
}

impl AmpWebSocket {
    /// Connect and authenticate via query token. Returns a receiver of
    /// decoded JSON events ({"type": "...", "data": {...}}).
    pub async fn connect(server: &str, token: &str) -> Result<Self> {
        let ws_url = server
            .trim_end_matches('/')
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1)
            + &format!("/v1/ws?token={token}");

        let (ws, _resp) = tokio_tungstenite::connect_async(ws_url).await?;
        let (tx, rx) = mpsc::channel(64);

        let handle = tokio::spawn(async move {
            let (mut sink, mut stream) = ws.split();

            // Keepalive ping every 25 s
            let mut ping = tokio::time::interval(std::time::Duration::from_secs(25));
            ping.tick().await; // first tick is immediate

            loop {
                tokio::select! {
                    msg = stream.next() => match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t))) => {
                            if let Ok(v) = serde_json::from_str::<Value>(&t) {
                                if tx.send(v).await.is_err() {
                                    break; // receiver dropped
                                }
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(p))) => {
                            let _ = sink.send(tokio_tungstenite::tungstenite::Message::Pong(p)).await;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => break,
                    },
                    _ = ping.tick() => {
                        if sink.send(tokio_tungstenite::tungstenite::Message::Text(
                            "{\"type\":\"ping\"}".into(),
                        )).await.is_err() {
                            break;
                        }
                    },
                }
            }
        });

        Ok(Self { rx, handle })
    }

    /// Await the next event of a specific type (e.g. `"hello"`).
    /// Events of other types are skipped.
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

/// Type alias kept for symmetry with other SDKs.
pub type EventHandler = Arc<dyn Fn(&Value) + Send + Sync>;

/// Simple handler registry (optional convenience over raw rx).
pub struct EventHandlers {
    map: HashMap<String, EventHandler>,
}

impl Default for EventHandlers {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHandlers {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn on<F: Fn(&Value) + Send + Sync + 'static>(&mut self, event: &str, f: F) {
        self.map.insert(event.to_string(), Arc::new(f));
    }

    pub fn dispatch(&self, v: &Value) {
        if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
            if let Some(h) = self.map.get(t) {
                h(v);
            }
        }
    }
}
