use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{Arc, mpsc},
};

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};

use crate::server::ControllerMessage;

pub fn start_player() -> (Arc<Player>, MixerDeviceSink) {
    let handle =
        DeviceSinkBuilder::open_default_sink().expect("Failed to open default audio stream");

    let player = Arc::new(Player::connect_new(handle.mixer()));

    (player, handle)
}

/// Try to load and append a track to the player
/// Returns true if successful, false if file not found or decode error
pub fn append_track(player: &Arc<Player>, audio_path: &Path) -> bool {
    match File::open(audio_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            match Decoder::new(reader) {
                Ok(source) => {
                    player.append(source);
                    true
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

pub fn append_track_with_finish_signal(
    player: &Arc<Player>,
    audio_path: &Path,
    id: String,
    tx: mpsc::Sender<ControllerMessage>,
) -> bool {
    let file = match File::open(audio_path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let source = match Decoder::new(BufReader::new(file)) {
        Ok(source) => source,
        Err(_) => return false,
    };
    player.append(source);
    player.append(rodio::source::EmptyCallback::new(Box::new(move || {
        let _ = tx.send(ControllerMessage::TrackFinished(id.clone()));
    })));
    true
}
