use clap::{ Parser, Subcommand };
use serde::{ Deserialize, Serialize };
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
}

