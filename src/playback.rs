use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{Arc, mpsc},
};

use cpal::traits::HostTrait;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};

use crate::server::ControllerMessage;

pub struct PlayerState {
    pub player: Arc<Player>,
    pub _sink: MixerDeviceSink,
    pub device_name: String,
}

fn default_device_name() -> anyhow::Result<String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no default audio output device"))?;
    Ok(device.to_string())
}

fn open_sink(error_tx: mpsc::Sender<ControllerMessage>) -> anyhow::Result<MixerDeviceSink> {
    let builder = DeviceSinkBuilder::from_default_device()
        .map_err(|error| anyhow::anyhow!("failed to find default audio device: {error}"))?;
    builder
        .with_error_callback(move |_| {
            let _ = error_tx.send(ControllerMessage::AudioStreamError);
        })
        .open_stream()
        .map_err(|error| anyhow::anyhow!("failed to open audio stream: {error}"))
}

pub fn start_player(error_tx: mpsc::Sender<ControllerMessage>) -> anyhow::Result<PlayerState> {
    let device_name = default_device_name()?;
    let sink = open_sink(error_tx)?;
    let player = Arc::new(Player::connect_new(sink.mixer()));

    Ok(PlayerState { player, _sink: sink, device_name })
}

pub fn replace_player(
    state: &mut PlayerState,
    error_tx: mpsc::Sender<ControllerMessage>,
) -> anyhow::Result<()> {
    let volume = state.player.volume();
    let paused = state.player.is_paused();
    let device_name = default_device_name()?;
    let sink = open_sink(error_tx)?;
    let player = Arc::new(Player::connect_new(sink.mixer()));
    player.set_volume(volume);
    if paused {
        player.pause();
    }
    *state = PlayerState { player, _sink: sink, device_name };
    Ok(())
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
