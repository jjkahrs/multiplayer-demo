//! Per-connection read loop: parses and validates client frames into
//! [`ClientEvent`]s for the Zone. Bad frames get an `error` reply and are
//! dropped; the socket stays open. Ends on `leave`, socket close, or a socket
//! error. Ending drops the connection's unicast sender, which is what lets
//! the writer flush any queued reply and report `Closed` to the Zone.
//!
//! With network simulation on, text frames wait in an uplink [`DelayQueue`]
//! before they are handled; control frames are never delayed. Frames still
//! queued when the socket ends are drained (handled at their scheduled times)
//! before the loop returns.

use axum::extract::ws::{Message, Utf8Bytes, WebSocket};
use futures_util::StreamExt;
use futures_util::stream::SplitStream;
use protocol::{ClientMsg, MAX_NAME_LEN, NameError, ServerMsg, sanitize_name};
use tokio::sync::{mpsc, watch};

use crate::netsim::{DelayQueue, NetSim};
use crate::zone::ClientEvent;

/// Per-connection state the frame handler needs.
struct Ingest {
    events: mpsc::Sender<ClientEvent>,
    unicast_tx: mpsc::Sender<ServerMsg>,
    player_id_rx: watch::Receiver<Option<u64>>,
    join_sent: bool,
}

/// Whether ingest keeps handling frames after this one.
enum Flow {
    Continue,
    /// `leave`, or the Zone stopped.
    Stop,
}

pub async fn run(
    mut stream: SplitStream<WebSocket>,
    events: mpsc::Sender<ClientEvent>,
    unicast_tx: mpsc::Sender<ServerMsg>,
    player_id_rx: watch::Receiver<Option<u64>>,
    net_sim: NetSim,
) {
    let mut state = Ingest { events, unicast_tx, player_id_rx, join_sent: false };
    let mut uplink = DelayQueue::new(net_sim);

    loop {
        tokio::select! {
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) if net_sim.is_off() => {
                    if let Flow::Stop = handle(text, &mut state).await {
                        return;
                    }
                }
                Some(Ok(Message::Text(text))) => uplink.push(text),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            text = uplink.next() => {
                if let Flow::Stop = handle(text, &mut state).await {
                    return;
                }
            }
        }
    }

    while !uplink.is_empty() {
        if let Flow::Stop = handle(uplink.next().await, &mut state).await {
            break;
        }
    }
}

/// Parse, validate and forward one text frame.
async fn handle(text: Utf8Bytes, state: &mut Ingest) -> Flow {
    let msg = match serde_json::from_str::<ClientMsg>(&text) {
        Ok(msg) => msg,
        Err(err) => {
            reply_error(&state.unicast_tx, "bad_message", err.to_string()).await;
            return Flow::Continue;
        }
    };
    let event = match msg {
        // One connection is one player: only the first accepted join counts.
        ClientMsg::Join { name } if !state.join_sent => match sanitize_name(&name) {
            Ok(name) => {
                state.join_sent = true;
                ClientEvent::Join { name, unicast_tx: state.unicast_tx.clone() }
            }
            Err(err) => {
                reply_error(&state.unicast_tx, "bad_name", name_error_message(err)).await;
                return Flow::Continue;
            }
        },
        ClientMsg::Join { .. } => return Flow::Continue,
        ClientMsg::Input { vx, vz, seq, t0 } => {
            // Input before `joined` has no player to steer.
            let Some(player_id) = *state.player_id_rx.borrow() else {
                return Flow::Continue;
            };
            ClientEvent::Input { player_id, vx, vz, seq, t0 }
        }
        ClientMsg::Leave => return Flow::Stop,
    };
    if state.events.send(event).await.is_err() {
        return Flow::Stop; // Zone stopped.
    }
    Flow::Continue
}

async fn reply_error(unicast_tx: &mpsc::Sender<ServerMsg>, code: &str, message: String) {
    // Err only means the writer is gone; the loop ends on the next read.
    let _ = unicast_tx.send(ServerMsg::ErrorMsg { code: code.to_owned(), message }).await;
}

fn name_error_message(err: NameError) -> String {
    match err {
        NameError::Empty => "name must not be empty".to_owned(),
        NameError::TooLong => format!("name must be at most {MAX_NAME_LEN} characters"),
        NameError::IllegalChar => "name must not contain control characters".to_owned(),
    }
}
