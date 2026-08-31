use std::{
    fs,
    io,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use url::Url;

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatEntry {
    pub id: String,
    pub extractor: String,
    pub extractor_id: String,
    pub title: String,
    pub url: String,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMeta {
    pub id: String,
    pub extractor: String,
    pub extractor_id: String,
    pub title: String,
    #[serde(default, deserialize_with = "opt_string")] 
    pub artist: Option<String>,
    #[serde(default, deserialize_with = "opt_string")] 
    pub album: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")] 
    pub upload_date: Option<String>,
    pub webpage_url: String,
    #[serde(default, deserialize_with = "opt_string")] 
    pub thumbnail_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DownloadedFile {
    pub audio_path: PathBuf,
    pub thumbnail_path: Option<PathBuf>,
}

pub fn download_from_link(link: &str, _cfg: &Config) -> Result<String> {
    let flat = resolve_flat(link, _cfg)?;
    Ok(serde_json::to_string_pretty(&flat)?)
}

pub fn resolve_flat(link: &str, _cfg: &Config) -> Result<Vec<FlatEntry>> {
    let parsed = Url::parse(link).context("invalid URL")?;
    let output = Command::new("yt-dlp")
        .args(["--flat-playlist", "-j", "--no-warnings", "--ignore-errors", parsed.as_str()])
        .output()
        .with_context(|| format!("failed to run yt-dlp for {link}"))?;

    // Don't bail on non-zero exit; yt-dlp exits non-zero if any item failed.
    // We accept partial success as long as we got some JSON lines.
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && !stderr.is_empty() {
        eprintln!("yt-dlp warning: {}", stderr);
    }

    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid yt-dlp flat JSON: {line}"))?;

        // Prefer ie_key, but fall back to extractor field (which is present for single videos)
        let extractor = value
            .get("ie_key")
            .and_then(Value::as_str)
            .or_else(|| value.get("extractor").and_then(Value::as_str))
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| "unknown".to_string());
        let extractor_id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default();

        let title = value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();

        let url = value
            .get("webpage_url")
            .or_else(|| value.get("url"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| parsed.to_string());

        entries.push(FlatEntry {
            id: format!("{extractor}:{extractor_id}"),
            extractor: extractor.clone(),
            extractor_id: extractor_id.clone(),
            title,
            url,
            playlist_id: value
                .get("playlist_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            playlist_title: value
                .get("playlist_title")
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
    }

    Ok(entries)
}

pub fn resolve_full(urls: &[String], _cfg: &Config) -> Result<Vec<VideoMeta>> {
    if urls.is_empty() {
        return Ok(Vec::new());
    }

    let mut cmd = Command::new("yt-dlp");
    cmd.args(["-j", "--no-warnings", "--ignore-errors"]);
    for url in urls {
        cmd.arg(url);
    }

    let output = cmd.output().with_context(|| "failed to run yt-dlp detailed metadata lookup")?;
    // Don't bail on non-zero exit; yt-dlp exits non-zero if any item failed.
    // We accept partial success as long as we got some JSON lines.
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && !stderr.is_empty() {
        eprintln!("yt-dlp warning: {}", stderr);
    }

    let mut metas = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid yt-dlp full JSON: {line}"))?;
        let extractor = value
            .get("extractor")
            .and_then(Value::as_str)
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_else(|| "unknown".to_string());
        let extractor_id = value
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default();

        metas.push(VideoMeta {
            id: format!("{extractor}:{extractor_id}"),
            extractor: extractor.clone(),
            extractor_id: extractor_id.clone(),
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            artist: value.get("artist").and_then(Value::as_str).map(str::to_owned),
            album: value.get("album").and_then(Value::as_str).map(str::to_owned),
            duration: value.get("duration").and_then(Value::as_f64),
            upload_date: value.get("upload_date").and_then(Value::as_str).map(str::to_owned),
            webpage_url: value
                .get("webpage_url")
                .or_else(|| value.get("url"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_default(),
            thumbnail_url: value.get("thumbnail").and_then(Value::as_str).map(str::to_owned),
        });
    }

    Ok(metas)
}

pub fn download_one(meta: &VideoMeta, cfg: &Config) -> Result<DownloadedFile> {
    // Use a per-video subdirectory in temp to avoid parallel download conflicts
    let video_temp_dir = cfg.temp_dir.join(&meta.extractor_id);
    fs::create_dir_all(&video_temp_dir)?;

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-f")
        .arg("bestaudio/best")
        .arg("--extract-audio")
        .arg("--audio-format")
        .arg("vorbis")
        .arg("-P")
        .arg(&video_temp_dir)
        .arg("--retries")
        .arg(cfg.retries.to_string())
        .arg("--continue")
        .arg("--ignore-errors")
        .arg("--embed-metadata")
        .arg("--write-thumbnail")
        .arg("--convert-thumbnail")
        .arg("jpg")
        .arg("--no-warnings");

    if let Some(browser) = &cfg.cookies_from_browser {
        let browser_str = format!("{browser:?}").to_ascii_lowercase();
        cmd.arg("--cookies-from-browser").arg(browser_str);
    }

    cmd.arg(&meta.webpage_url);

    let status = cmd.status().with_context(|| format!("failed to download {}", meta.title))?;
    if !status.success() {
        anyhow::bail!("yt-dlp download failed for {}", meta.title);
    }

    let (audio_path, thumbnail_path) = find_downloaded_files(&video_temp_dir)?;
    let bucket_dir = bucket_dir_for_id(&cfg.download_dir, &meta.id);
    fs::create_dir_all(&bucket_dir)?;

    let final_audio = move_to_bucket(&audio_path, &bucket_dir)?;
    let final_thumb = if let Some(path) = thumbnail_path {
        Some(move_to_bucket(&path, &bucket_dir)?)
    } else {
        None
    };

    // Clean up the per-video temp directory
    let _ = fs::remove_dir_all(&video_temp_dir);

    Ok(DownloadedFile {
        audio_path: final_audio,
        thumbnail_path: final_thumb,
    })
}

fn find_downloaded_files(temp_dir: &Path) -> Result<(PathBuf, Option<PathBuf>)> {
    let mut audio_path = None;
    let mut thumbnail_path = None;

    for entry in fs::read_dir(temp_dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("")
            .to_ascii_lowercase();

        if audio_path.is_none() && is_audio_ext(&ext) {
            audio_path = Some(path);
            continue;
        }

        if thumbnail_path.is_none() && is_image_ext(&ext) {
            thumbnail_path = Some(path);
        }
    }

    Ok((
        audio_path.context("yt-dlp did not place an audio file in the temp directory")?,
        thumbnail_path,
    ))
}

fn bucket_dir_for_id(download_dir: &Path, video_id: &str) -> PathBuf {
    let bucket_name = video_id
        .chars()
        .last()
        .unwrap_or('x')
        .to_ascii_lowercase()
        .to_string();
    download_dir.join(bucket_name)
}

fn move_to_bucket(src: &Path, bucket_dir: &Path) -> Result<PathBuf> {
    let name = src
        .file_name()
        .context("file has no filename")?
        .to_os_string();

    let mut candidate = bucket_dir.join(&name);
    let mut index = 1;
    while candidate.exists() {
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("download");
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("");
        candidate = bucket_dir.join(format!("{stem}_{index}.{ext}"));
        index += 1;
    }

    if let Err(err) = fs::rename(src, &candidate) {
        if err.kind() == io::ErrorKind::CrossesDevices {
            fs::copy(src, &candidate)?;
            fs::remove_file(src)?;
        } else {
            return Err(err).context("failed to move downloaded file into bucket directory");
        }
    }

    Ok(candidate)
}

fn is_audio_ext(ext: &str) -> bool {
    matches!(
        ext,
        "mp3" | "m4a" | "aac" | "opus" | "ogg" | "flac" | "wav" | "webm"
    )
}

fn is_image_ext(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "webp" | "bmp")
}

fn opt_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<String> = Option::deserialize(deserializer)?;
    Ok(value)
}
