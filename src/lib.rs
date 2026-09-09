pub mod cli;
mod config;
mod db;
mod download;
mod ipc;
mod job;
mod playback;
mod queue;
mod server;

use crate::{
    cli::{Cli, Commands, FilterSpec},
    config::Config,
    db::Db,
    queue::{PlaybackQueue, QueueMode},
};
use clap::Parser;
use interprocess::local_socket::prelude::*;

pub fn run() {
    let args = Cli::parse();
    let cfg = Config::load().expect("failed to load config");
    if let Some(Commands::Config { action }) = args.command.as_ref() {
        match action {
            cli::ConfigAction::Get { key } => match cfg.get(key) {
                Some(value) => println!("{value}"),
                None => eprintln!("unknown config key: {key}"),
            },
            cli::ConfigAction::Set { key, value } => {
                let mut config = cfg.clone();
                config.set(key, value).expect("failed to update config");
                config.save().expect("failed to save config");
                println!("updated {key}");
            }
            cli::ConfigAction::Path => println!("{}", Config::config_path().display()),
        }
        return;
    }
    if let Some(link) = args.download.as_deref() {
        let mut config = cfg.clone();
        if let Some(browser) = args.cookies_from_browser {
            config.cookies_from_browser = Some(browser);
        }
        match job::run_download_job(link, &config) {
            Ok(summary) => println!("{summary:?}"),
            Err(error) => eprintln!("download job failed: {error}"),
        }
        return;
    }
    if let Some(command) = args.command.as_ref() {
        match command {
            Commands::Tags { action } => {
                if let Err(e) = handle_tags_query(&cfg, action) {
                    eprintln!("tags query failed: {e}");
                }
                return;
            }
            Commands::Playlists => {
                if let Err(e) = handle_playlists_query(&cfg) {
                    eprintln!("playlists query failed: {e}");
                }
                return;
            }
            Commands::Info => {
                let socket = ipc::socket_name();
                if let Ok(mut stream) = LocalSocketStream::connect(socket) {
                    print_response(ipc::send_request(&mut stream, command));
                } else if let Err(e) = handle_info_query(&cfg) {
                    eprintln!("info query failed: {e}");
                }
                return;
            }
            _ => {}
        }
    }

    let command = resolve_startup_command(&args);
    let socket = ipc::socket_name();
    match LocalSocketStream::connect(socket.clone()) {
        Ok(mut stream) => print_response(ipc::send_request(&mut stream, &command)),
        Err(_) => {
            let db = match Db::open(&cfg.db_path) {
                Ok(db) => db,
                Err(e) => {
                    eprintln!("database open failed: {e}");
                    return;
                }
            };
            match build_initial_queue(&db, &command) {
                Ok(queue) if !queue.is_empty() => server::run(socket, cfg.db_path.clone(), queue),
                Ok(_) => eprintln!("no matching tracks; player was not started"),
                Err(e) => eprintln!("could not build playback queue: {e}"),
            }
        }
    }
}

fn resolve_startup_command(args: &Cli) -> Commands {
    let filter = FilterSpec {
        playlist: args.playlist.clone(),
        tags: args.tag.clone(),
        artist: args.artist.clone(),
        album: args.album.clone(),
        match_mode: args.match_mode.clone(),
    };
    match &args.command {
        Some(Commands::Ascending { sort_by, .. }) => Commands::Ascending {
            sort_by: *sort_by,
            filter,
        },
        Some(Commands::Descending { sort_by, .. }) => Commands::Descending {
            sort_by: *sort_by,
            filter,
        },
        None | Some(Commands::Shuffle { .. }) => Commands::Shuffle { filter },
        Some(other) => other.clone(),
    }
}

fn build_initial_queue(db: &Db, command: &Commands) -> anyhow::Result<PlaybackQueue> {
    match command {
        Commands::Shuffle { filter } => Ok(PlaybackQueue::new_shuffle(
            db.filtered_video_ids_and_weights(filter)?,
            seed(),
        )),
        Commands::Ascending { sort_by, filter } => Ok(PlaybackQueue::new_sorted(
            db.filtered_video_ids_sorted(filter, &sort_by.to_string(), false)?,
            QueueMode::Ascending,
        )),
        Commands::Descending { sort_by, filter } => Ok(PlaybackQueue::new_sorted(
            db.filtered_video_ids_sorted(filter, &sort_by.to_string(), true)?,
            QueueMode::Descending,
        )),
        _ => anyhow::bail!(
            "no player running - start one first with a bare invocation, shuffle, ascending, or descending"
        ),
    }
}

fn print_response(result: anyhow::Result<ipc::Response>) {
    match result {
        Ok(ipc::Response::Ack) => println!("OK"),
        Ok(ipc::Response::Data(data)) => println!(
            "{}",
            serde_json::to_string_pretty(&data).unwrap_or_else(|_| "error".to_string())
        ),
        Ok(ipc::Response::Err(error)) => eprintln!("Error: {error}"),
        Err(error) => eprintln!("IPC request failed: {error}"),
    }
}

fn handle_tags_query(cfg: &Config, action: &cli::TagsAction) -> anyhow::Result<()> {
    let db = Db::open(&cfg.db_path)?;
    match action {
        cli::TagsAction::List => {
            for (id, name) in db.list_tags()? {
                println!("{id}: {name}");
            }
        }
    }
    Ok(())
}
fn handle_playlists_query(cfg: &Config) -> anyhow::Result<()> {
    for (id, title) in Db::open(&cfg.db_path)?.list_source_playlists()? {
        println!("{id}: {title}");
    }
    Ok(())
}
fn handle_info_query(cfg: &Config) -> anyhow::Result<()> {
    match Db::open(&cfg.db_path)?.load_player_state()? {
        Some((mode, sort, filter, seed, cursor)) => {
            println!("mode={mode} sort={sort:?} filter={filter} seed={seed:?} cursor={cursor}")
        }
        None => println!("No active playback session"),
    }
    Ok(())
}
fn seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}
