use std::{
    fs::File,
    io::{ self, BufReader, Read, Write},
    sync::Arc,
};

use interprocess::local_socket::{ prelude::*, ListenerOptions, Name, GenericNamespaced };
use rodio::{DeviceSinkBuilder, Player, Decoder};
use clap::Parser;
use serde_json;

mod cli;
use cli::{ Cli, Commands };

const SOCKET_NAME: &str = "audiocrab.sock";

fn main() {

    let args = Cli::parse();

    let sock_name = SOCKET_NAME.to_ns_name::<GenericNamespaced>().expect("Failed to format socket identifier");

    match LocalSocketStream::connect(sock_name.clone()) {
        Ok(mut stream) => {
            println!("Server found alive");
            // TODO: handle communication

            if let Some(cmd) = args.command {

                let payload = serde_json::to_vec(&cmd).unwrap();

                stream.write_all(&payload).unwrap();

            }

        },
        Err (_) => {
            println!("Creating background player...");
            
            run_server (sock_name, args);
        }
    }
}


// TODO: refactor and complete functionality
fn run_server (sock_name: Name<'static>, args: Cli) {
    let handle = DeviceSinkBuilder::open_default_sink()
        .expect("Failed to open default audio stream");

    let player = Arc::new(
        Player::connect_new (handle.mixer())
    );

    let file = File::open(args.audio_file.expect("No audio file provided")).expect("File not found");
    let reader = BufReader::new(file);

    let source = Decoder::new(reader).expect("failed to decode audio");

    player.append(source);

    let listener = match ListenerOptions::new().name(sock_name).create_sync() {
        Err (e) if e.kind() == io::ErrorKind::AddrInUse => {
            panic! (
                "Error: could not start server because the socket file is \
                occupied. Please check if {SOCKET_NAME} is in use by another \
                process and try again."
            );
            // TODO: create a way to reclaim name safely when needed.
        },
        x => x.expect("Should work for now. will get back to it")
    };

    for stream in listener.incoming().filter_map(|s| s.ok()) {
        let player_clone = Arc::clone(&player);

        std::thread::spawn (move || {
            let mut stream = stream;
            let mut buffer = vec![0u8; 128];

            if let Ok(bytes_in) = stream.read(&mut buffer) {
                if bytes_in == 0 { return; }

                if let Ok(command) = serde_json::from_slice(&buffer[..bytes_in]) {
                    match command {
                        Commands::Play => {
                            eprintln!("received Play command!");
                            player_clone.play();
                        },
                        Commands::Pause => {
                            eprintln!("recieved Pause command!");
                            player_clone.pause();
                        },
                        Commands::Stop => {
                            eprintln!("recieved Stop command!");
                            player_clone.stop();
                        }
                        _ => {
                            eprintln!("not implemented!")
                            // TODO: implement other commands
                        }
                    }
                }
            }
        });
        
        if player.empty() {
            break;
        }
    }

}