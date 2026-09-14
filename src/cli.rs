use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(name = "crabaudio", about = "Headless background music player")]
pub struct Cli {
    /// download link (must be supported by yt-dlp)
    #[arg(long, short, visible_alias = "URL", alias = "link")]
    pub download: Option<String>,

    /// filter by source playlist name (repeatable)
    #[arg(long)]
    pub playlist: Option<String>,

    /// filter by tag name (repeatable)
    #[arg(long)]
    pub tag: Vec<String>,

    /// filter by artist name
    #[arg(long)]
    pub artist: Option<String>,

    /// filter by album name
    #[arg(long)]
    pub album: Option<String>,

    /// filter match mode: 'and' (all filters must match) or 'or' (any filter matches)
    #[arg(long, default_value = "and")]
    pub match_mode: String,

    /// continue previous playback session if available (default: true)
    #[arg(long, default_value = "true")]
    pub r#continue: bool,

    /// browser to pull cookies from (for auth-gated downloads)
    #[arg(long, value_enum)]
    pub cookies_from_browser: Option<Browser>,

    /// control commands sent to running player
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Serialize, Deserialize, Debug, Clone)]
pub enum Commands {
    Play,
    Pause,
    SetVolume {
        level: u8,
    },
    VolumeUp,
    VolumeDown,
    Skip,
    Quit,
    Next,
    Previous,
    Shuffle {
        #[arg(skip)]
        filter: FilterSpec,
    },
    Ascending {
        sort_by: SortKey,
        #[arg(skip)]
        filter: FilterSpec,
    },
    Descending {
        sort_by: SortKey,
        #[arg(skip)]
        filter: FilterSpec,
    },
    Name,
    Info,
    Like {
        weight: Option<u8>,
    },
    Tags {
        action: TagsAction,
    },
    Playlists,
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Serialize, Deserialize, Debug, Clone, clap::ValueEnum)]
pub enum TagsAction {
    List,
}

#[derive(Subcommand, Serialize, Deserialize, Debug, Clone)]
pub enum ConfigAction {
    Get { key: String },
    Set { key: String, value: String },
    Path,
}

#[derive(clap::ValueEnum, Clone, Debug, Serialize, Deserialize, Copy)]
pub enum SortKey {
    Title,
    Artist,
    Album,
    UploadDate,
    DateAdded,
    PlayCount,
}

impl std::fmt::Display for SortKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortKey::Title => write!(f, "title"),
            SortKey::Artist => write!(f, "artist"),
            SortKey::Album => write!(f, "album"),
            SortKey::UploadDate => write!(f, "upload_date"),
            SortKey::DateAdded => write!(f, "created_at"),
            SortKey::PlayCount => write!(f, "play_count"),
        }
    }
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilterSpec {
    pub playlist: Option<String>,
    pub tags: Vec<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub match_mode: String, // "and" or "or"
}

impl Default for FilterSpec {
    fn default() -> Self {
        Self {
            playlist: None,
            tags: Vec::new(),
            artist: None,
            album: None,
            match_mode: "and".to_string(),
        }
    }
}
