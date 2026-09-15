//! Serialization round-trips and exact-wire-bytes assertions for the protocol.
//!
//! The design's example JSON ([TECHNICAL_DESIGN.md](../../docs/TECHNICAL_DESIGN.md))
//! is written with cosmetic spaces after colons/commas. `serde_json`'s default
//! output is compact, which is the actual byte format on the wire; the exact-string
//! tests below assert that compact form (identical tokens, whitespace normalized).

use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

use protocol::messages::*;
use protocol::validation::{sanitize_name, NameError};

fn assert_roundtrip<T>(msg: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let value = serde_json::to_value(msg).expect("serialize to value");
    let back: T = serde_json::from_value(value).expect("deserialize from value");
    assert_eq!(msg, &back);

    let text = serde_json::to_string(msg).expect("serialize to string");
    let back_str: T = serde_json::from_str(&text).expect("deserialize from string");
    assert_eq!(msg, &back_str);
}

fn snapshot_player(id: u64, name: &str, x: f64, z: f64) -> SnapshotPlayer {
    SnapshotPlayer {
        id,
        name: name.to_owned(),
        x,
        z,
        yaw: 2.0,
        state: PlayerState::Walk,
        seq: 1,
        t0: 9000,
        age_ms: 150,
    }
}

fn bob_joined() -> ServerMsg {
    ServerMsg::Joined {
        player_id: 7,
        name: "Bob".to_owned(),
        x: 0.0,
        z: 0.0,
        yaw: 1.5708,
        speed: 5.0,
        world_half: 50.0,
        tick_hz: 20,
    }
}

const JOINED_JSON: &str =
    r#"{"type":"joined","playerId":7,"name":"Bob","x":0.0,"z":0.0,"yaw":1.5708,"speed":5.0,"worldHalf":50.0,"tickHz":20}"#;
const SNAPSHOT_JSON: &str = r#"{"type":"snapshot","tick":1234,"players":[{"id":7,"name":"Bob","x":12.0,"z":-3.5,"yaw":2.0,"state":"walk","seq":1,"t0":9000,"ageMs":150}]}"#;

#[test]
fn client_messages_roundtrip() {
    assert_roundtrip(&ClientMsg::Join {
        name: "Bob".to_owned(),
    });
    assert_roundtrip(&ClientMsg::Input {
        vx: 0.0,
        vz: 1.0,
        seq: 42,
        t0: 912_345,
    });
    assert_roundtrip(&ClientMsg::Leave);
}

#[test]
fn client_messages_roundtrip_non_default_fields() {
    assert_roundtrip(&ClientMsg::Input {
        vx: -0.75,
        vz: 0.5,
        seq: 9_999,
        t0: 1_000_000,
    });
}

#[test]
fn server_messages_roundtrip() {
    assert_roundtrip(&bob_joined());
    assert_roundtrip(&ServerMsg::Snapshot {
        tick: 0,
        players: vec![
            snapshot_player(7, "Bob", 12.0, -3.5),
            snapshot_player(8, "Alice", -1.25, 9.5),
        ],
    });
    assert_roundtrip(&ServerMsg::PlayerJoined {
        id: 9,
        name: "Alice".to_owned(),
    });
    assert_roundtrip(&ServerMsg::PlayerLeft { id: 9 });
    assert_roundtrip(&ServerMsg::ErrorMsg {
        code: "bad_name".to_owned(),
        message: "name too long".to_owned(),
    });
}

#[test]
fn server_messages_roundtrip_non_default_fields() {
    assert_roundtrip(&ServerMsg::Snapshot {
        tick: u64::MAX,
        players: vec![SnapshotPlayer {
            id: u64::MAX,
            name: "Mover".to_owned(),
            x: -49.75,
            z: 50.0,
            yaw: 3.14159,
            state: PlayerState::Idle,
            seq: 42,
            t0: 912_345,
            age_ms: u64::MAX,
        }],
    });
    assert_roundtrip(&ServerMsg::Joined {
        player_id: u64::MAX,
        name: "Mover".to_owned(),
        x: -49.75,
        z: 50.0,
        yaw: 3.14159,
        speed: 7.5,
        world_half: 125.0,
        tick_hz: 60,
    });
}

#[test]
fn snapshot_with_one_player_roundtrips() {
    let snapshot = ServerMsg::Snapshot {
        tick: 1,
        players: vec![snapshot_player(7, "Bob", 12.0, -3.5)],
    };
    assert_roundtrip(&snapshot);
}

#[test]
fn snapshot_with_three_players_roundtrips() {
    let snapshot = ServerMsg::Snapshot {
        tick: 2,
        players: vec![
            snapshot_player(7, "Bob", 0.0, 0.0),
            snapshot_player(8, "Alice", 10.0, 5.0),
            snapshot_player(9, "Carol", -7.0, 20.0),
        ],
    };
    assert_roundtrip(&snapshot);
}

#[test]
fn join_serializes_to_design_json() {
    let msg = ClientMsg::Join {
        name: "Bob".to_owned(),
    };
    assert_eq!(
        serde_json::to_string(&msg).unwrap(),
        r#"{"type":"join","name":"Bob"}"#
    );
}

#[test]
fn joined_serializes_to_design_json() {
    assert_eq!(serde_json::to_string(&bob_joined()).unwrap(), JOINED_JSON);
}

#[test]
fn single_player_snapshot_serializes_to_design_json() {
    let msg = ServerMsg::Snapshot {
        tick: 1234,
        players: vec![snapshot_player(7, "Bob", 12.0, -3.5)],
    };
    assert_eq!(serde_json::to_string(&msg).unwrap(), SNAPSHOT_JSON);
}

#[test]
fn error_msg_uses_design_type_tag() {
    let msg = ServerMsg::ErrorMsg {
        code: "bad_name".to_owned(),
        message: "name too long".to_owned(),
    };
    assert_eq!(
        serde_json::to_string(&msg).unwrap(),
        r#"{"type":"error","code":"bad_name","message":"name too long"}"#
    );
}

#[test]
fn exact_design_strings_deserialize() {
    let join: ClientMsg =
        serde_json::from_str(r#"{"type":"join","name":"Bob"}"#).expect("join parses");
    assert_eq!(
        join,
        ClientMsg::Join {
            name: "Bob".to_owned()
        }
    );

    let joined: ServerMsg = serde_json::from_str(JOINED_JSON).expect("joined parses");
    assert_eq!(joined, bob_joined());

    let snapshot: ServerMsg = serde_json::from_str(SNAPSHOT_JSON).expect("snapshot parses");
    assert_eq!(
        snapshot,
        ServerMsg::Snapshot {
            tick: 1234,
            players: vec![snapshot_player(7, "Bob", 12.0, -3.5)]
        }
    );
}

mod validation {
    use super::*;

    #[test]
    fn trims_whitespace() {
        assert_eq!(sanitize_name("  Bob  "), Ok("Bob".to_owned()));
    }

    #[test]
    fn rejects_empty_names() {
        assert_eq!(sanitize_name(""), Err(NameError::Empty));
        assert_eq!(sanitize_name("   "), Err(NameError::Empty));
        assert_eq!(sanitize_name("\n"), Err(NameError::Empty));
    }

    #[test]
    fn rejects_seventeen_char_names() {
        let seventeen = "12345678901234567";
        assert_eq!(seventeen.chars().count(), 17);
        assert_eq!(sanitize_name(seventeen), Err(NameError::TooLong));
    }

    #[test]
    fn accepts_sixteen_char_names() {
        let sixteen = "1234567890123456";
        assert_eq!(sixteen.chars().count(), 16);
        assert_eq!(sanitize_name(sixteen), Ok(sixteen.to_owned()));
    }

    #[test]
    fn rejects_control_chars() {
        assert_eq!(sanitize_name("Bo\u{1}b"), Err(NameError::IllegalChar));
    }
}
