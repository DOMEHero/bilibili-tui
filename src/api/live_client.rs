//! Bilibili Live WebSocket Client
//!
//! Manages WebSocket connection for receiving live stream messages.

use super::live_ws::{
    DanmuInfoData, LiveMessage, Packet, make_auth_packet, make_heartbeat_packet, parse_message,
};
use anyhow::{Result, anyhow};
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Live WebSocket client
pub struct LiveClient {
    /// Sender to signal shutdown
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Receiver for live messages
    message_rx: mpsc::Receiver<LiveMessage>,
    last_error: Arc<Mutex<Option<String>>>,
    connection_state: Arc<AtomicU8>,
}

impl LiveClient {
    /// Connect to live room WebSocket
    pub async fn connect(
        room_id: i64,
        uid: i64,
        buvid: String,
        danmu_info: &DanmuInfoData,
    ) -> Result<Self> {
        let urls = danmu_info
            .host_list
            .iter()
            .map(|host| host.wss_url())
            .collect::<Vec<_>>();
        if urls.is_empty() {
            return Err(anyhow!("No WebSocket hosts available"));
        }
        let token = danmu_info.token.clone();

        // Create channels
        let (message_tx, message_rx) = mpsc::channel::<LiveMessage>(256);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let last_error = Arc::new(Mutex::new(None));
        let task_error = Arc::clone(&last_error);
        let connection_state = Arc::new(AtomicU8::new(0));
        let task_state = Arc::clone(&connection_state);

        // Spawn connection task
        tokio::spawn(async move {
            run_connections(
                &urls,
                room_id,
                uid,
                &token,
                &buvid,
                message_tx,
                &mut shutdown_rx,
                task_error,
                task_state,
            )
            .await;
        });

        Ok(Self {
            shutdown_tx: Some(shutdown_tx),
            message_rx,
            last_error,
            connection_state,
        })
    }

    /// Try to receive a message (non-blocking)
    pub fn try_recv(&mut self) -> Option<LiveMessage> {
        self.message_rx.try_recv().ok()
    }

    /// Return and clear the most recent connection error. The client keeps
    /// reconnecting after reporting it.
    pub fn take_error(&self) -> Option<String> {
        self.last_error.lock().ok()?.take()
    }

    pub fn is_connected(&self) -> bool {
        self.connection_state.load(Ordering::Acquire) == 1
    }

    /// Disconnect from WebSocket
    pub async fn disconnect(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_connections(
    urls: &[String],
    room_id: i64,
    uid: i64,
    token: &str,
    buvid: &str,
    message_tx: mpsc::Sender<LiveMessage>,
    shutdown_rx: &mut mpsc::Receiver<()>,
    last_error: Arc<Mutex<Option<String>>>,
    connection_state: Arc<AtomicU8>,
) {
    loop {
        for url in urls {
            connection_state.store(0, Ordering::Release);
            match run_connection(
                url,
                room_id,
                uid,
                token,
                buvid,
                message_tx.clone(),
                shutdown_rx,
                &connection_state,
            )
            .await
            {
                Ok(true) => return,
                Ok(false) => {
                    connection_state.store(2, Ordering::Release);
                    if let Ok(mut error) = last_error.lock() {
                        *error = Some(format!("live connection closed: {url}"));
                    }
                }
                Err(error) => {
                    connection_state.store(2, Ordering::Release);
                    if let Ok(mut value) = last_error.lock() {
                        *value = Some(format!("live connection failed for {url}: {error}"));
                    }
                }
            }
        }

        tokio::select! {
            _ = shutdown_rx.recv() => return,
            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
        }
    }
}

impl Drop for LiveClient {
    fn drop(&mut self) {
        // Signal shutdown (best effort)
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
    }
}

/// Run WebSocket connection loop
#[allow(clippy::too_many_arguments)]
async fn run_connection(
    url: &str,
    room_id: i64,
    uid: i64,
    token: &str,
    buvid: &str,
    message_tx: mpsc::Sender<LiveMessage>,
    shutdown_rx: &mut mpsc::Receiver<()>,
    connection_state: &AtomicU8,
) -> Result<bool> {
    // Connect to WebSocket
    let (ws_stream, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(url))
        .await
        .map_err(|_| anyhow!("WebSocket connection timed out"))??;
    let (mut write, mut read) = ws_stream.split();

    // Send auth packet
    let auth_packet = make_auth_packet(room_id, uid, token, buvid);
    write.send(Message::Binary(auth_packet.into())).await?;

    // Heartbeat interval (30 seconds)
    let mut heartbeat_interval = interval(Duration::from_secs(30));
    heartbeat_interval.tick().await; // Skip first immediate tick

    loop {
        tokio::select! {
            // Check for shutdown signal
            _ = shutdown_rx.recv() => {
                return Ok(true);
            }

            // Send heartbeat
            _ = heartbeat_interval.tick() => {
                let hb = make_heartbeat_packet();
                if write.send(Message::Binary(hb.into())).await.is_err() {
                    break;
                }
            }

            // Receive messages
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        if let Some(code) = process_message(&data[..], &message_tx).await? {
                            if code != 0 {
                                return Err(anyhow!("live authentication failed: {code}"));
                            }
                            connection_state.store(1, Ordering::Release);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Err(_)) => {
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(false)
}

/// Process received WebSocket message
async fn process_message(
    data: &[u8],
    message_tx: &mpsc::Sender<LiveMessage>,
) -> Result<Option<i32>> {
    let packets = Packet::decode(data)?;
    let mut auth_code = None;

    for packet in packets {
        if let Some(msg) = parse_message(&packet) {
            if let LiveMessage::AuthReply { code } = msg {
                auth_code = Some(code);
            }
            // Send message (ignore if channel is full)
            let _ = message_tx.try_send(msg);
        }
    }

    Ok(auth_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_status_can_recover_after_an_error() {
        let (shutdown_tx, _shutdown_rx) = mpsc::channel(1);
        let (_message_tx, message_rx) = mpsc::channel(1);
        let state = Arc::new(AtomicU8::new(2));
        let client = LiveClient {
            shutdown_tx: Some(shutdown_tx),
            message_rx,
            last_error: Arc::new(Mutex::new(Some("temporary".into()))),
            connection_state: Arc::clone(&state),
        };
        assert!(!client.is_connected());
        assert_eq!(client.take_error().as_deref(), Some("temporary"));
        state.store(1, Ordering::Release);
        assert!(client.is_connected());
    }
}
