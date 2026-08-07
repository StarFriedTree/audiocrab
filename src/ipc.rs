use std::io::Write;

use interprocess::local_socket::{prelude::*, GenericNamespaced, Name};

use crate::cli::Commands;

pub const SOCKET_NAME: &str = "audiocrab.sock";

pub fn socket_name() -> Name<'static> {
    SOCKET_NAME
        .to_ns_name::<GenericNamespaced>()
        .expect("Failed to format socket identifier")
}

pub fn send_command(stream: &mut impl Write, command: &Commands) {
    let payload = serde_json::to_vec(command).unwrap();
    stream.write_all(&payload).unwrap();
}