use std::{
    io::{self, Read},
    path::PathBuf,
    sync::Arc,
};

use interprocess::local_socket::{prelude::*, ListenerOptions, Name};
use rodio::Player;

use crate::{cli::Commands, ipc, playback};

pub fn run(sock_name: Name<'static>, audio_file: PathBuf) {
    let player = playback::start_player(&audio_file);

    let listener = match ListenerOptions::new().name(sock_name).create_sync() {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            panic!(
                "Error: could not start server because the socket file is \
                occupied. Please check if {} is in use by another process and try again.",
                ipc::SOCKET_NAME
            );
            // TODO: create a way to reclaim name safely when needed.
        }
        x => x.expect("Should work for now. will get back to it"),
    };

    for stream in listener.incoming().filter_map(|stream| stream.ok()) {
        let player_clone = Arc::clone(&player);

        std::thread::spawn(move || handle_connection(stream, player_clone));

        if player.empty() {
            break;
        }
    }
}

fn handle_connection(mut stream: impl Read, player: Arc<Player>) {
    let mut buffer = vec![0u8; 128];

    if let Ok(bytes_in) = stream.read(&mut buffer) {
        if bytes_in == 0 {
            return;
        }

        if let Ok(command) = serde_json::from_slice(&buffer[..bytes_in]) {
            dispatch_command(command, player);
        }
    }
}

fn dispatch_command(command: Commands, player: Arc<Player>) {
    match command {
        Commands::Play => {
            eprintln!("received Play command!");
            player.play();
        }
        Commands::Pause => {
            eprintln!("recieved Pause command!");
            player.pause();
        }
        Commands::Stop => {
            eprintln!("recieved Stop command!");
            player.stop();
        }
        _ => {
            eprintln!("not implemented!");
        }
    }
}