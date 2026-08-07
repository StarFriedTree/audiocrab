use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::Arc,
};

use rodio::{Decoder, DeviceSinkBuilder, Player};

pub fn start_player(audio_file: &Path) -> Arc<Player> {
    let handle = DeviceSinkBuilder::open_default_sink()
        .expect("Failed to open default audio stream");

    let player = Arc::new(Player::connect_new(handle.mixer()));

    let file = File::open(audio_file).expect("No audio file provided");
    let reader = BufReader::new(file);
    let source = Decoder::new(reader).expect("failed to decode audio");

    player.append(source);
    player
}