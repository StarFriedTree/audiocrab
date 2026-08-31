use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "crabaudio", about = "Headless background music player")]
pub struct Cli {
    /// path to a single audio file
    #[arg(long, short, alias = "file", visible_alias = "af")]
    pub audio_file: Option<PathBuf>,

    /// path to a directory with multiple audio files
    #[arg(long, visible_alias = "playlist", alias = "directory")]
    pub dir: Option<PathBuf>,

    /// download link (must be supported by yt-dlp)
    #[arg(long, short, visible_alias = "URL", alias = "link")]
    pub download: Option<String>, 

    /// control commands sent to running player
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// browser to pull cookies from (for auth-gated downloads)
    #[arg(long, value_enum)]
    pub cookies_from_browser: Option<Browser>,
}

#[derive(Subcommand, Serialize, Deserialize, Debug, Clone)]
pub enum Commands {
    Play,
    Pause,
    SetVolume,
    VolumeUp,
    VolumeDown,
    Skip,
    Stop,
    Name,
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Serialize, Deserialize, Debug, Clone)]
pub enum ConfigAction {
    Get { key: String },
    Set { key: String, value: String },
    Path,
}

#[derive(clap::ValueEnum, Clone, Debug, Serialize, Deserialize)]
pub enum Browser {
    Brave,
    Chrome,
    Chromium,
    Edge,
    Firefox,
    Opera,
    Safari,
    Vivaldi,
    Whale,
}
