//! Axum HTTP surface: health probe, zone metrics, and the WebSocket endpoint.

use axum::extract::State;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};

use crate::netsim::NetSim;
use crate::zone::ZoneHandle;
use crate::{ingest, writer};

/// Replies (`joined`, errors) waiting for this connection's writer.
const UNICAST_QUEUE: usize = 16;

/// Build the application router, wired to a running Zone. `net_sim` applies
/// to every connection; kept out of `ZoneHandle` so the Zone stays unaware
/// of transport simulation.
pub fn router(zone: ZoneHandle, net_sim: NetSim) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/ws", get(ws_upgrade))
        .with_state((zone, net_sim))
}

/// Liveness probe: 200 `{"status":"ok"}`.
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// Zone stats as of the last tick.
async fn metrics(State((zone, _)): State<(ZoneHandle, NetSim)>) -> impl IntoResponse {
    Json(zone.metrics.snapshot())
}

async fn ws_upgrade(ws: WebSocketUpgrade, State((zone, net_sim)): State<(ZoneHandle, NetSim)>) -> Response {
    ws.on_upgrade(move |socket| connect(socket, zone, net_sim))
}

/// Split one connection into its ingest (read) and writer (write) tasks.
async fn connect(socket: WebSocket, zone: ZoneHandle, net_sim: NetSim) {
    let (sink, stream) = socket.split();
    let (unicast_tx, unicast_rx) = mpsc::channel(UNICAST_QUEUE);
    let (player_id_tx, player_id_rx) = watch::channel(None);
    tokio::spawn(writer::run(sink, unicast_rx, zone.outbound, player_id_tx, zone.events.clone(), net_sim));
    tokio::spawn(ingest::run(stream, zone.events, unicast_tx, player_id_rx, net_sim));
}
