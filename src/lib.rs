pub mod cli;
mod config;
mod db;
mod download;
mod ipc;
mod job;
mod playback;
mod server;

use clap::Parser;
use interprocess::local_socket::prelude::*;

use crate::{
    cli::{Cli, Commands},
    config::Config,
};

pub fn run() {
    let args = Cli::parse();
    let cfg = Config::load().expect("failed to load config");

    if let Some(Commands::Config { action }) = args.command.as_ref() {
        match action {
            crate::cli::ConfigAction::Get { key } => {
                match cfg.get(key) {
                    Some(value) => println!("{value}"),
                    None => eprintln!("unknown config key: {key}"),
                }
            }
            crate::cli::ConfigAction::Set { key, value } => {
                let mut config = cfg.clone();
                config
                    .set(key, value)
                    .expect("failed to update config");
                config.save().expect("failed to save config");
                println!("updated {key}");
            }
            crate::cli::ConfigAction::Path => println!("{}", Config::config_path().display()),
        }
        return;
    }

    if let Some(link) = args.download.as_deref() {      
		match job::run_download_job(link, &cfg) {
			Ok(summary) => println!("{summary:?}"),
			Err(err) => eprintln!("download job failed: {err}"),
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