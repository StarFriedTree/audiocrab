pub mod cli;

use clap::Parser;
use interprocess::local_socket::prelude::*;

mod download;
mod ipc;
mod playback;
mod server;

use crate::cli::Cli;

pub fn run() {
	let args = Cli::parse();

	if let Some(link) = args.download.as_deref() {
		if args.no_embed_metadata == Some(true) {
			download::download_without_metadata(link);
		} else {
			download::download_from_link(link, &args);
		}
		return;
	}

	let sock_name = ipc::socket_name();

	match LocalSocketStream::connect(sock_name.clone()) {
		Ok(mut stream) => {
			println!("Server found alive");

			if let Some(command) = args.command.as_ref() {
				ipc::send_command(&mut stream, command);
			}
		}
		Err(_) => {
			println!("Creating background player...");
			server::run(sock_name, args.audio_file.expect("No audio file provided"));
		}
	}
}