//! Per-connection write loop: forwards this connection's unicast replies and,
//! once joined, the Zone broadcast. Owns the player id for the connection
//! (learned from the `joined` reply it forwards) and reports `Closed` when the
//! connection ends. Never blocks the Zone: broadcast lag skips frames.

use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use bytes::Bytes;
use futures_util::SinkExt;
use futures_util::stream::SplitSink;
use protocol::ServerMsg;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::{mpsc, watch};

use crate::zone::{ClientEvent, Outbound};

pub async fn run(
    mut sink: SplitSink<WebSocket, Message>,
    mut unicast_rx: mpsc::Receiver<ServerMsg>,
    outbound: broadcast::Sender<Outbound>,
    player_id_tx: watch::Sender<Option<u64>>,
    events: mpsc::Sender<ClientEvent>,
) {
    let mut player_id = None;
    // Subscribed only after `joined`, so that reply is always the first frame.
    let mut broadcasts = None;

    loop {
        let frame = tokio::select! {
            reply = unicast_rx.recv() => {
                // None: ingest ended and every queued reply has been sent.
                let Some(reply) = reply else { break };
                if let ServerMsg::Joined { player_id: id, .. } = &reply {
                    player_id = Some(*id);
                    player_id_tx.send_replace(Some(*id));
                    broadcasts = Some(outbound.subscribe());
                }
                encode(&reply)
            }
            frame = next_broadcast(&mut broadcasts) => match frame {
                Ok(frame) => encode_outbound(frame),
                Err(RecvError::Lagged(skipped)) => {
                    tracing::debug!(skipped, "writer lagged behind broadcast");
                    continue;
                }
                Err(RecvError::Closed) => break,
            },
        };
        let Some(frame) = frame else { continue };
        if sink.send(frame).await.is_err() {
            break; // Dead socket.
        }
    }

    if let Some(player_id) = player_id {
        let _ = events.send(ClientEvent::Closed { player_id }).await;
    }
    let _ = sink.close().await;
}

async fn next_broadcast(rx: &mut Option<broadcast::Receiver<Outbound>>) -> Result<Outbound, RecvError> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

fn encode_outbound(frame: Outbound) -> Option<Message> {
    match frame {
        // Shares the tick's single serialization; only UTF-8 validation per send.
        Outbound::Snapshot(json) => Utf8Bytes::try_from(Bytes::from_owner(json)).ok().map(Message::Text),
        Outbound::Event(msg) => encode(&msg),
    }
}

fn encode(msg: &ServerMsg) -> Option<Message> {
    match serde_json::to_string(msg) {
        Ok(text) => Some(Message::Text(text.into())),
        Err(err) => {
            tracing::error!(%err, "failed to encode server message");
            None
        }
    }
}
