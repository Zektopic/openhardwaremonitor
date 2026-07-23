//! `/ws/telemetry` — real-time telemetry broadcast to browsers.
//!
//! # Fan-out design
//!
//! The poll thread serializes each frame **once** into an `Arc<String>` and
//! pushes it into a `tokio::sync::broadcast` channel. Every connected client
//! holds its own `Receiver` and forwards the *same shared allocation* — so
//! adding clients costs a pointer clone each, not another serialization.
//!
//! # Backpressure stops here
//!
//! `broadcast::Sender::send` is synchronous and never blocks, so a browser on a
//! congested Wi-Fi link cannot slow the hardware poller or the GUI. When a
//! client falls further behind than the channel capacity it receives
//! [`RecvError::Lagged`] instead of a silent gap, and we resync it from the
//! store's latest frame. Dropping stale intermediate frames is exactly right
//! for a live dashboard: nobody wants to watch a delayed replay.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use super::AppState;

/// Upgrade an HTTP request to a WebSocket. Auth already ran in middleware.
pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| client_loop(socket, state))
}

/// Per-client task: forward every published frame until either side hangs up.
async fn client_loop(socket: WebSocket, state: AppState) {
    // Split so we can await a broadcast and a client message concurrently —
    // `tokio::select!` cannot borrow the socket mutably in two branches.
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.store.subscribe();

    // Prime the client with the current frame so the dashboard renders
    // immediately instead of staying blank until the next tick.
    if send_latest(&mut sink, &state).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            broadcast = rx.recv() => match broadcast {
                Ok(json) => {
                    // Same Arc every client forwards — no re-serialization.
                    if sink.send(Message::Text(json.as_str().into())).await.is_err() {
                        break; // client vanished mid-write
                    }
                }
                // Too far behind: skip the backlog and jump to the present.
                Err(RecvError::Lagged(_)) => {
                    if send_latest(&mut sink, &state).await.is_err() {
                        break;
                    }
                }
                // The store was dropped — the app is shutting down.
                Err(RecvError::Closed) => break,
            },

            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if handle_command(&text, &mut sink, &state).await.is_err() {
                        break;
                    }
                }
                // Ping/Pong are answered by axum itself; nothing to do.
                Some(Ok(_)) => {}
                // Close frame, transport error, or end of stream.
                Some(Err(_)) | None => break,
            },
        }
    }
}

/// Commands a dashboard may send. Deliberately tiny — the socket is a
/// broadcast, not an RPC surface, and it must never be able to mutate hardware.
async fn handle_command(
    text: &str,
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &AppState,
) -> Result<(), ()> {
    match text.trim() {
        // Explicit resync, e.g. after the tab was backgrounded.
        "snapshot" | r#"{"cmd":"snapshot"}"# => send_latest(sink, state).await,
        // Application-level keepalive for proxies that strip WebSocket pings.
        "ping" | r#"{"cmd":"ping"}"# => sink
            .send(Message::Text(r#"{"pong":true}"#.into()))
            .await
            .map_err(|_| ()),
        // Unknown input is ignored rather than closing the socket: a future
        // dashboard version talking to an older app should degrade, not break.
        _ => Ok(()),
    }
}

/// Send the store's current frame. Reads a pre-serialized `Arc<String>`, so no
/// lock is taken and nothing is held across the await.
async fn send_latest(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &AppState,
) -> Result<(), ()> {
    let json = state.store.json(); // Arc clone; guard-free by construction
    sink.send(Message::Text(json.as_str().into())).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use crate::state::{TelemetryFrame, TelemetryStore};
    use std::sync::Arc;
    use tokio::sync::broadcast::error::TryRecvError;

    /// The fan-out property the whole design rests on: N subscribers receive
    /// the *same* allocation, so publishing is O(1) in client count.
    #[test]
    fn all_subscribers_share_one_serialization() {
        let store = TelemetryStore::new(8);
        let mut a = store.subscribe();
        let mut b = store.subscribe();
        let mut c = store.subscribe();

        store.publish(TelemetryFrame { seq: 5, ..Default::default() });

        let (ra, rb, rc) = (a.try_recv().unwrap(), b.try_recv().unwrap(), c.try_recv().unwrap());
        assert!(Arc::ptr_eq(&ra, &rb) && Arc::ptr_eq(&rb, &rc));
        assert!(ra.contains(r#""seq":5"#));
    }

    /// A client that never reads must not stall the publisher — this is what
    /// keeps a slow browser from slowing the hardware loop.
    #[test]
    fn a_stalled_client_cannot_block_publishing() {
        let store = TelemetryStore::new(2);
        let _stalled = store.subscribe(); // subscribed, never reads

        let start = std::time::Instant::now();
        for seq in 1..=500 {
            store.publish(TelemetryFrame { seq, ..Default::default() });
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(store.load().seq, 500);
    }

    /// And when it does read, it's told it lagged and can resync from the
    /// store — which is exactly what `send_latest` does in the live loop.
    #[test]
    fn lagged_client_resyncs_from_the_store() {
        let store = TelemetryStore::new(2);
        let mut rx = store.subscribe();
        for seq in 1..=10 {
            store.publish(TelemetryFrame { seq, ..Default::default() });
        }
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Lagged(_))));
        assert!(store.json().contains(r#""seq":10"#));
    }
}
