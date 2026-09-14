use crate::{
    cli::{Commands, FilterSpec, SortKey},
    db::Db,
    ipc, playback,
    queue::{PlaybackQueue, QueueMode},
};
use cpal::traits::HostTrait;
use interprocess::local_socket::{ListenerNonblockingMode, ListenerOptions, Name, prelude::*};
use rodio::Player;
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc},
    time::Duration,
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
    AudioStreamError,
    AudioDeviceChanged,
}

type SharedPlayer = Arc<Mutex<playback::PlayerState>>;


pub fn run(
    sock_name: Name<'static>, 
    db_path: PathBuf, 
    initial_queue: PlaybackQueue,
    initial_filter: FilterSpec,
    initial_sort: Option<SortKey>,
) {
    let (tx, rx) = mpsc::channel();
    let output = match playback::start_player(tx.clone()) {
        Ok(state) => Arc::new(Mutex::new(state)),
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    let controller_tx = tx.clone();
    let output_clone = Arc::clone(&output);
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);
    std::thread::spawn(move || {
        controller_loop(rx, output_clone, db_path, initial_queue, initial_filter, initial_sort, controller_tx, shutdown_clone)
    });
    let monitor_shutdown = Arc::clone(&shutdown);
    let monitor_output = Arc::clone(&output);
    let monitor_tx = tx.clone();
    std::thread::spawn(move || monitor_default_audio_device(monitor_tx, monitor_output, monitor_shutdown));
    let listener = match ListenerOptions::new().name(sock_name).create_sync() {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            panic!("Error: socket {} is already in use", ipc::SOCKET_NAME)
        }
        result => result.expect("Failed to create socket listener"),
    };
    listener.set_nonblocking(ListenerNonblockingMode::Accept).expect("Failed to configure socket listener");
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok(stream) => {
                let tx_clone = tx.clone();
                std::thread::spawn(move || handle_connection(stream, tx_clone));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                eprintln!("socket listener failed: {error}");
                break;
            }
        }
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
    output: SharedPlayer,
    db_path: PathBuf,
    mut queue: PlaybackQueue,
    mut current_filter: FilterSpec,
    mut current_sort: Option<SortKey>,
    self_tx: mpsc::Sender<ControllerMessage>,
    shutdown: Arc<AtomicBool>,
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
    let mut expected = load_track(&db, &output, queue.current_id(), &self_tx);
    
    if expected.is_none() {
        expected = find_playable_track(&db, &output, &mut queue, &self_tx);
    }
    if expected.is_none() {
        eprintln!("no playable matching tracks");
        return;
    }
    
    load_lookahead(&db, &output, &queue, &self_tx);
    persist_state(&db, &queue, &current_filter, current_sort.as_ref());

    while let Ok(message) = rx.recv() {
        match message {
            ControllerMessage::TrackFinished(id) => {
                if expected.as_deref() != Some(id.as_str()) {
                    continue;
                }
                let _ = db.record_play(&id);
                queue.advance();
                expected = queue.current_id().map(str::to_string);
                load_lookahead(&db, &output, &queue, &self_tx);
                persist_state(&db, &queue, &current_filter, current_sort.as_ref());
            }
            ControllerMessage::AudioStreamError => {
                if recover_audio_output(&output, &self_tx) {
                    if let Some(id) = expected.as_deref() {
                        let _ = load_track(&db, &output, Some(id), &self_tx);
                    }
                    load_lookahead(&db, &output, &queue, &self_tx);
                }
            }
            ControllerMessage::AudioDeviceChanged => {
                if recover_audio_output(&output, &self_tx) {
                    if let Some(id) = expected.as_deref() {
                        let _ = load_track(&db, &output, Some(id), &self_tx);
                    }
                    load_lookahead(&db, &output, &queue, &self_tx);
                }
            }
            ControllerMessage::Ipc(request) => {
                let quit = matches!(request.command, Commands::Quit);
                let response = handle_command(
                    &db,
                    &output,
                    &mut queue,
                    &mut expected,
                    &mut current_filter,
                    &mut current_sort,
                    &self_tx,
                    request.command,
                );
                let _ = request.response_tx.send(response);
                if quit {
                    shutdown.store(true, Ordering::Release);
                    break;
                }
            }
        }
    }
}

fn handle_command(
    db: &Db,
    output: &SharedPlayer,
    queue: &mut PlaybackQueue,
    expected: &mut Option<String>,
    current_filter: &mut FilterSpec,
    current_sort: &mut Option<SortKey>,
    self_tx: &mpsc::Sender<ControllerMessage>,
    command: Commands,
) -> ipc::Response {
    match command {
        Commands::Play => {
            let player = current_player(output);
            if player.empty() {
                *expected = load_track(db, output, queue.current_id(), self_tx);
                if expected.is_none() {
                    *expected = find_playable_track(db, output, queue, self_tx);
                }
                load_lookahead(db, output, queue, self_tx);
            }
            player.play();
            ipc::Response::Ack
        }
        Commands::Pause => {
            current_player(output).pause();
            ipc::Response::Ack
        }
        Commands::Quit => {
            ipc::Response::Ack
        }
        Commands::SetVolume { level } => {
            current_player(output).set_volume((level as f32 / 100.0).clamp(0.0, 1.0));
            ipc::Response::Ack
        }
        Commands::VolumeUp => {
            let player = current_player(output);
            player.set_volume((player.volume() + 0.05).min(1.0));
            ipc::Response::Ack
        }
        Commands::VolumeDown => {
            let player = current_player(output);
            player.set_volume((player.volume() - 0.05).max(0.0));
            ipc::Response::Ack
        }
        Commands::Next | Commands::Skip => {
            current_player(output).skip_one();
            queue.advance();
            *expected = queue.current_id().map(str::to_string);
            load_lookahead(db, output, queue, self_tx);
            persist_state(db, queue, current_filter, current_sort.as_ref());
            ipc::Response::Ack
        }
        Commands::Previous => {
            current_player(output).stop();
            queue.previous();
            *expected = queue.current_id().map(str::to_string);
            let _ = load_track(db, output, queue.current_id(), self_tx);
            load_lookahead(db, output, queue, self_tx);
            persist_state(db, queue, current_filter, current_sort.as_ref());
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
                *current_filter = filter;
                *current_sort = None;
                persist_state(db, queue, current_filter, current_sort.as_ref());
                ipc::Response::Ack
            }
            Err(e) => ipc::Response::Err(e.to_string()),
        },
        Commands::Ascending { sort_by, filter } => {
            rebuild_sorted(db, queue, current_filter, current_sort, sort_by, filter, false)
        }
        Commands::Descending { sort_by, filter } => {
            rebuild_sorted(db, queue, current_filter, current_sort, sort_by, filter, true)
        }
        Commands::Info => {
            let player = current_player(output);
            let currently_playing = queue
                .current_id()
                .and_then(|id| db.get_video_info(id).ok().flatten())
                .map(|info| serde_json::json!({
                    "ID": info.id,
                    "title": info.title,
                    "artist": info.artist,
                    "album": info.album,
                    "play_count": info.play_count,
                    "weight": info.weight,
                }));
            ipc::Response::Data(serde_json::json!({
                "currently_playing": currently_playing,
                "queue_position": queue.cursor(),
                "queue_length": queue.len(),
                "is_paused": player.is_paused(),
                "volume": player.volume(),
            }))
        }
        Commands::Name => ipc::Response::Data(serde_json::json!("audiocrab")),
        _ => ipc::Response::Err("command is not handled by the player".to_string()),
    }
}

fn rebuild_sorted(
    db: &Db,
    queue: &mut PlaybackQueue,
    current_filter: &mut FilterSpec,
    current_sort: &mut Option<SortKey>,
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
            *current_filter = filter;
            *current_sort = Some(sort_by);
            persist_state(db, queue, current_filter, current_sort.as_ref());
            ipc::Response::Ack
        }
        Err(e) => ipc::Response::Err(e.to_string()),
    }
}

fn load_track(
    db: &Db,
    output: &SharedPlayer,
    id: Option<&str>,
    tx: &mpsc::Sender<ControllerMessage>,
) -> Option<String> {
    let id = id?;
    let meta = db.get_video_meta(id).ok()?;
    let path = meta.audio_path.as_deref()?;
    let player = current_player(output);
    playback::append_track_with_finish_signal(&player, path, id.to_string(), tx.clone())
        .then(|| id.to_string())
}

fn load_lookahead(
    db: &Db,
    output: &SharedPlayer,
    queue: &PlaybackQueue,
    tx: &mpsc::Sender<ControllerMessage>,
) {
    for offset in 1..=queue.len().max(1) {
        if let Some(id) = queue.peek_ahead(offset) {
            if load_track(db, output, Some(id), tx).is_some() {
                return;
            }
        }
    }
}

fn find_playable_track(
    db: &Db,
    output: &SharedPlayer,
    queue: &mut PlaybackQueue,
    tx: &mpsc::Sender<ControllerMessage>,
) -> Option<String> {
    for _ in 0..queue.len() {
        if let Some(id) = load_track(db, output, queue.current_id(), tx) {
            return Some(id);
        }
        queue.advance();
    }
    None
}

fn current_player(output: &SharedPlayer) -> Arc<Player> {
    Arc::clone(&output.lock().expect("player state lock poisoned").player)
}

fn recover_audio_output(output: &SharedPlayer, tx: &mpsc::Sender<ControllerMessage>) -> bool {
    let mut state = match output.lock() {
        Ok(state) => state,
        Err(_) => return false,
    };
    match playback::replace_player(&mut state, tx.clone()) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("{error}");
            false
        }
    }
}

fn monitor_default_audio_device(
    tx: mpsc::Sender<ControllerMessage>,
    output: SharedPlayer,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(250));
        let current_device = cpal::default_host()
            .default_output_device()
            .map(|device| device.to_string());
        let player_device = output
            .lock()
            .ok()
            .map(|state| state.device_name.clone());
        if current_device.as_deref() != player_device.as_deref() {
            if tx.send(ControllerMessage::AudioDeviceChanged).is_err() {
                break;
            }
        }
    }
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
