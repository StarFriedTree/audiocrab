use crate::{
    cli::{Commands, FilterSpec, SortKey},
    db::Db,
    ipc, playback,
    queue::{PlaybackQueue, QueueMode},
};
use interprocess::local_socket::{ListenerOptions, Name, prelude::*};
use rodio::Player;
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, mpsc},
};

#[derive(Debug)]
pub struct ControllerRequest {
    pub command: Commands,
    pub response_tx: mpsc::Sender<ipc::Response>,
}
#[derive(Debug)]
pub enum ControllerMessage {
    Ipc(ControllerRequest),
    TrackFinished(String),
}

pub fn run(sock_name: Name<'static>, db_path: PathBuf, initial_queue: PlaybackQueue) {
    let (player, _handle) = playback::start_player();
    let (tx, rx) = mpsc::channel();
    let controller_tx = tx.clone();
    let player_clone = Arc::clone(&player);
    std::thread::spawn(move || {
        controller_loop(rx, player_clone, db_path, initial_queue, controller_tx)
    });
    let listener = match ListenerOptions::new().name(sock_name).create_sync() {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            panic!("Error: socket {} is already in use", ipc::SOCKET_NAME)
        }
        result => result.expect("Failed to create socket listener"),
    };
    for stream in listener.incoming().filter_map(|stream| stream.ok()) {
        let tx_clone = tx.clone();
        std::thread::spawn(move || handle_connection(stream, tx_clone));
    }
}

fn handle_connection(mut stream: impl Read + Write, tx: mpsc::Sender<ControllerMessage>) {
    let mut len_bytes = [0u8; 4];
    if stream.read_exact(&mut len_bytes).is_err() {
        return;
    }
    let length = u32::from_le_bytes(len_bytes) as usize;
    let mut payload = vec![0u8; length];
    if stream.read_exact(&mut payload).is_err() {
        return;
    }
    let command: Commands = match serde_json::from_slice(&payload) {
        Ok(command) => command,
        Err(error) => {
            let _ = send_response(&mut stream, &ipc::Response::Err(error.to_string()));
            return;
        }
    };
    let (response_tx, response_rx) = mpsc::channel();
    if tx
        .send(ControllerMessage::Ipc(ControllerRequest {
            command,
            response_tx,
        }))
        .is_err()
    {
        let _ = send_response(
            &mut stream,
            &ipc::Response::Err("controller stopped".to_string()),
        );
        return;
    }
    if let Ok(response) = response_rx.recv() {
        let _ = send_response(&mut stream, &response);
    }
}

fn send_response(stream: &mut impl Write, response: &ipc::Response) -> io::Result<()> {
    let payload =
        serde_json::to_vec(response).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    stream.write_all(&(payload.len() as u32).to_le_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()
}

fn controller_loop(
    rx: mpsc::Receiver<ControllerMessage>,
    player: Arc<Player>,
    db_path: PathBuf,
    mut queue: PlaybackQueue,
    self_tx: mpsc::Sender<ControllerMessage>,
) {
    let db = match Db::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("playback database failed: {e}");
            return;
        }
    };
    if queue.is_empty() {
        eprintln!("no matching tracks");
        return;
    }
    let mut expected = load_track(&db, &player, queue.current_id(), &self_tx);
    if expected.is_none() {
        expected = find_playable_track(&db, &player, &mut queue, &self_tx);
    }
    if expected.is_none() {
        eprintln!("no playable matching tracks");
        return;
    }
    load_lookahead(&db, &player, queue.next_id(), &self_tx);
    persist_state(&db, &queue, &FilterSpec::default(), None);

    while let Ok(message) = rx.recv() {
        match message {
            ControllerMessage::TrackFinished(id) => {
                if expected.as_deref() != Some(id.as_str()) {
                    continue;
                }
                let _ = db.record_play(&id);
                queue.advance();
                expected = queue.current_id().map(str::to_string);
                load_lookahead(&db, &player, queue.next_id(), &self_tx);
                persist_state(&db, &queue, &FilterSpec::default(), None);
            }
            ControllerMessage::Ipc(request) => {
                let response = handle_command(
                    &db,
                    &player,
                    &mut queue,
                    &mut expected,
                    &self_tx,
                    request.command,
                );
                let _ = request.response_tx.send(response);
            }
        }
    }
}

fn handle_command(
    db: &Db,
    player: &Arc<Player>,
    queue: &mut PlaybackQueue,
    expected: &mut Option<String>,
    self_tx: &mpsc::Sender<ControllerMessage>,
    command: Commands,
) -> ipc::Response {
    match command {
        Commands::Play => {
            player.play();
            ipc::Response::Ack
        }
        Commands::Pause => {
            player.pause();
            ipc::Response::Ack
        }
        Commands::Stop => {
            player.stop();
            *expected = None;
            ipc::Response::Ack
        }
        Commands::SetVolume { level } => {
            player.set_volume((level as f32 / 100.0).clamp(0.0, 1.0));
            ipc::Response::Ack
        }
        Commands::VolumeUp => {
            player.set_volume((player.volume() + 0.05).min(1.0));
            ipc::Response::Ack
        }
        Commands::VolumeDown => {
            player.set_volume((player.volume() - 0.05).max(0.0));
            ipc::Response::Ack
        }
        Commands::Next | Commands::Skip => {
            player.skip_one();
            queue.advance();
            *expected = queue.current_id().map(str::to_string);
            load_lookahead(db, player, queue.next_id(), self_tx);
            persist_state(db, queue, &FilterSpec::default(), None);
            ipc::Response::Ack
        }
        Commands::Previous => {
            player.stop();
            queue.previous();
            *expected = queue.current_id().map(str::to_string);
            let _ = load_track(db, player, queue.current_id(), self_tx);
            load_lookahead(db, player, queue.next_id(), self_tx);
            persist_state(db, queue, &FilterSpec::default(), None);
            ipc::Response::Ack
        }
        Commands::Like { weight } => match queue.current_id() {
            Some(id) => match db.set_weight(id, weight.unwrap_or(2).clamp(1, 10) as f64) {
                Ok(()) => ipc::Response::Ack,
                Err(e) => ipc::Response::Err(e.to_string()),
            },
            None => ipc::Response::Err("no current track".to_string()),
        },
        Commands::Shuffle { filter } => match db.filtered_video_ids_and_weights(&filter) {
            Ok(weighted) => {
                let current = queue.current_id().map(str::to_string);
                *queue = PlaybackQueue::new_shuffle(weighted, seed());
                if let Some(id) = current {
                    queue.set_current_id(&id);
                }
                ipc::Response::Ack
            }
            Err(e) => ipc::Response::Err(e.to_string()),
        },
        Commands::Ascending { sort_by, filter } => {
            rebuild_sorted(db, queue, sort_by, filter, false)
        }
        Commands::Descending { sort_by, filter } => {
            rebuild_sorted(db, queue, sort_by, filter, true)
        }
        Commands::Info => ipc::Response::Data(
            serde_json::json!({"currently_playing": queue.current_id(), "queue_position": queue.cursor(), "queue_length": queue.len(), "is_paused": player.is_paused(), "volume": player.volume()}),
        ),
        Commands::Name => ipc::Response::Data(serde_json::json!("audiocrab")),
        _ => ipc::Response::Err("command is not handled by the player".to_string()),
    }
}

fn rebuild_sorted(
    db: &Db,
    queue: &mut PlaybackQueue,
    sort_by: SortKey,
    filter: FilterSpec,
    descending: bool,
) -> ipc::Response {
    match db.filtered_video_ids_sorted(&filter, &sort_by.to_string(), descending) {
        Ok(ids) => {
            let current = queue.current_id().map(str::to_string);
            *queue = PlaybackQueue::new_sorted(
                ids,
                if descending {
                    QueueMode::Descending
                } else {
                    QueueMode::Ascending
                },
            );
            if let Some(id) = current {
                queue.set_current_id(&id);
            }
            ipc::Response::Ack
        }
        Err(e) => ipc::Response::Err(e.to_string()),
    }
}

fn load_track(
    db: &Db,
    player: &Arc<Player>,
    id: Option<&str>,
    tx: &mpsc::Sender<ControllerMessage>,
) -> Option<String> {
    let id = id?;
    let meta = db.get_video_meta(id).ok()?;
    let path = meta.audio_path.as_deref()?;
    playback::append_track_with_finish_signal(player, path, id.to_string(), tx.clone())
        .then(|| id.to_string())
}
fn load_lookahead(
    db: &Db,
    player: &Arc<Player>,
    id: Option<&str>,
    tx: &mpsc::Sender<ControllerMessage>,
) {
    let _ = load_track(db, player, id, tx);
}
fn find_playable_track(
    db: &Db,
    player: &Arc<Player>,
    queue: &mut PlaybackQueue,
    tx: &mpsc::Sender<ControllerMessage>,
) -> Option<String> {
    for _ in 0..queue.len() {
        if let Some(id) = load_track(db, player, queue.current_id(), tx) {
            return Some(id);
        }
        queue.advance();
    }
    None
}
fn persist_state(db: &Db, queue: &PlaybackQueue, filter: &FilterSpec, sort_key: Option<&SortKey>) {
    let mode = match queue.mode() {
        QueueMode::Shuffle => "shuffle",
        QueueMode::Ascending => "ascending",
        QueueMode::Descending => "descending",
    };
    let sort = sort_key.map(ToString::to_string);
    let json = serde_json::to_string(filter).unwrap_or_else(|_| "{}".to_string());
    let _ = db.upsert_player_state(mode, sort.as_deref(), &json, queue.seed(), queue.cursor());
}
fn seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}
