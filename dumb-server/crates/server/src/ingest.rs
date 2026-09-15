//! Per-connection read loop: parses and validates client frames into
//! [`ClientEvent`]s for the Zone. Bad frames get an `error` reply and are
//! dropped; the socket stays open. Ends on `leave`, socket close, or a socket
//! error. Ending drops the connection's unicast sender, which is what lets
//! the writer flush any queued reply and report `Closed` to the Zone.

use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use futures_util::stream::SplitStream;
use protocol::{ClientMsg, MAX_NAME_LEN, NameError, ServerMsg, sanitize_name};
use tokio::sync::{mpsc, watch};

use crate::zone::ClientEvent;

pub async fn run(
    mut stream: SplitStream<WebSocket>,
    events: mpsc::Sender<ClientEvent>,
    unicast_tx: mpsc::Sender<ServerMsg>,
    player_id_rx: watch::Receiver<Option<u64>>,
) {
    let mut join_sent = false;
    while let Some(Ok(frame)) = stream.next().await {
        let text = match frame {
            Message::Text(text) => text,
            Message::Close(_) => break,
            _ => continue,
        };
        let msg = match serde_json::from_str::<ClientMsg>(&text) {
            Ok(msg) => msg,
            Err(err) => {
                reply_error(&unicast_tx, "bad_message", err.to_string()).await;
                continue;
            }
        };
        let event = match msg {
            // One connection is one player: only the first accepted join counts.
            ClientMsg::Join { name } if !join_sent => match sanitize_name(&name) {
                Ok(name) => {
                    join_sent = true;
                    ClientEvent::Join { name, unicast_tx: unicast_tx.clone() }
                }
                Err(err) => {
                    reply_error(&unicast_tx, "bad_name", name_error_message(err)).await;
                    continue;
                }
            },
            ClientMsg::Join { .. } => continue,
            ClientMsg::Input { vx, vz, seq, t0 } => {
                // Input before `joined` has no player to steer.
                let Some(player_id) = *player_id_rx.borrow() else {
                    continue;
                };
                ClientEvent::Input { player_id, vx, vz, seq, t0 }
            }
            ClientMsg::Leave => break,
        };
        if events.send(event).await.is_err() {
            break; // Zone stopped.
        }
    }
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
