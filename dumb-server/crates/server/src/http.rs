//! Axum HTTP surface: health probe, zone metrics, and the WebSocket endpoint.

use axum::extract::State;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};

use crate::zone::ZoneHandle;
use crate::{ingest, writer};

/// Replies (`joined`, errors) waiting for this connection's writer.
const UNICAST_QUEUE: usize = 16;

/// Build the application router, wired to a running Zone.
pub fn router(zone: ZoneHandle) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/ws", get(ws_upgrade))
        .with_state(zone)
}

/// Liveness probe: 200 `{"status":"ok"}`.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Zone stats as of the last tick.
async fn metrics(State(zone): State<ZoneHandle>) -> impl IntoResponse {
    Json(zone.metrics.snapshot())
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(zone): State<ZoneHandle>) -> Response {
    ws.on_upgrade(move |socket| connect(socket, zone))
}

/// Split one connection into its ingest (read) and writer (write) tasks.
async fn connect(socket: WebSocket, zone: ZoneHandle) {
    let (sink, stream) = socket.split();
    let (unicast_tx, unicast_rx) = mpsc::channel(UNICAST_QUEUE);
    let (player_id_tx, player_id_rx) = watch::channel(None);
    tokio::spawn(writer::run(sink, unicast_rx, zone.outbound, player_id_tx, zone.events.clone()));
    tokio::spawn(ingest::run(stream, zone.events, unicast_tx, player_id_rx));
}
