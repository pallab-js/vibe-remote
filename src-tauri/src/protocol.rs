//! Binary wire protocol using MessagePack
//!
//! Replaces JSON+base64 with efficient binary MessagePack serialization

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WireFrame {
    pub seq: u64,
    pub w: u32,
    pub h: u32,
    pub ts: u64,
    pub keyframe: bool,
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WireInput {
    pub input_type: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub button: Option<String>,
    pub key: Option<String>,
    pub key_type: Option<String>,
    pub text: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "t")]
pub enum WireMessage {
    #[serde(rename = "frame")]
    Frame(WireFrame),
    #[serde(rename = "input")]
    Input(WireInput),
    #[serde(rename = "ping")]
    Ping { ts: u64 },
    #[serde(rename = "pong")]
    Pong { ts: u64 },
    #[serde(rename = "clipboard")]
    Clipboard { text: String },
    #[serde(rename = "file_begin")]
    FileBegin { id: u64, name: String, size: u64 },
    #[serde(rename = "file_chunk")]
    FileChunk { id: u64, offset: u64, data: Vec<u8> },
    #[serde(rename = "file_end")]
    FileEnd { id: u64 },
    #[serde(rename = "consent_req")]
    ConsentRequest { feature: String },
    #[serde(rename = "consent_res")]
    ConsentResponse { feature: String, granted: bool },
}

impl WireMessage {
    pub fn encode(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap_or_default()
    }

    pub fn decode(data: &[u8]) -> Result<Self, String> {
        rmp_serde::from_slice(data).map_err(|e| format!("WireMessage decode error: {}", e))
    }
}

pub fn encode_frame(seq: u64, w: u32, h: u32, ts: u64, keyframe: bool, data: Vec<u8>) -> Vec<u8> {
    WireMessage::Frame(WireFrame {
        seq,
        w,
        h,
        ts,
        keyframe,
        data,
    })
    .encode()
}

pub fn encode_input(
    input_type: &str,
    x: Option<i32>,
    y: Option<i32>,
    button: Option<&str>,
    key: Option<&str>,
    key_type: Option<&str>,
    text: Option<&str>,
) -> Vec<u8> {
    WireMessage::Input(WireInput {
        input_type: input_type.to_string(),
        x,
        y,
        button: button.map(String::from),
        key: key.map(String::from),
        key_type: key_type.map(String::from),
        text: text.map(String::from),
    })
    .encode()
}

pub fn encode_ping(ts: u64) -> Vec<u8> {
    WireMessage::Ping { ts }.encode()
}

pub fn encode_pong(ts: u64) -> Vec<u8> {
    WireMessage::Pong { ts }.encode()
}

pub fn encode_clipboard(text: &str) -> Vec<u8> {
    WireMessage::Clipboard {
        text: text.to_string(),
    }
    .encode()
}

// Protocol constants
pub const PROTOCOL_VERSION: u8 = 1;
pub const MAX_FRAME_SIZE: usize = 8 * 1024 * 1024; // 8MB max frame
pub const PING_INTERVAL_MS: u64 = 5000;
