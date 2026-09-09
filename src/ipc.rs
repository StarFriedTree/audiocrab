use std::io::{Read, Write};

use interprocess::local_socket::{GenericNamespaced, Name, prelude::*};
use serde::{Deserialize, Serialize};

use crate::cli::Commands;

pub const SOCKET_NAME: &str = "audiocrab.sock";

pub fn socket_name() -> Name<'static> {
    SOCKET_NAME
        .to_ns_name::<GenericNamespaced>()
        .expect("Failed to format socket identifier")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Ack,
    Data(serde_json::Value),
    Err(String),
}

/// Send a command and receive a response (length-prefixed framing)
pub fn send_request(
    stream: &mut (impl Read + Write),
    command: &Commands,
) -> anyhow::Result<Response> {
    // Serialize command to JSON
    let payload = serde_json::to_vec(command)?;

    // Write length-prefixed frame: u32 LE + JSON
    let len = payload.len() as u32;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()?;

    // Read response: u32 LE length + JSON
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let resp_len = u32::from_le_bytes(len_bytes) as usize;

    let mut resp_buf = vec![0u8; resp_len];
    stream.read_exact(&mut resp_buf)?;

    let response: Response = serde_json::from_slice(&resp_buf)?;
    Ok(response)
}

/// Send a command without waiting for response (fire-and-forget, kept for compatibility)
pub fn send_command(stream: &mut impl Write, command: &Commands) {
    if let Ok(payload) = serde_json::to_vec(command) {
        let len = payload.len() as u32;
        let _ = stream.write_all(&len.to_le_bytes());
        let _ = stream.write_all(&payload);
        let _ = stream.flush();
    }
}
