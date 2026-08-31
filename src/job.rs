use std::sync::{Arc, Mutex};
use std::fs;

use anyhow::{Result, Context};
use rayon::prelude::*;

use crate::{
    config::Config,
    db::Db,
    download::{self, FlatEntry},
};

#[derive(Debug, Default)]
pub struct JobSummary {
    pub total_in_link: usize,
    pub skipped_duplicate: usize,
    pub downloaded: usize,
    pub failed: usize,
}

pub fn acquire_download_lock(cfg: &Config) -> Result<fslock::LockFile> {
    let download_dir = &cfg.download_dir;

    fs::create_dir_all(download_dir)
        .with_context(|| format!("failed to creaete download directory at {}", download_dir.display()))?;

    let path = download_dir.join("download.lock");

    let mut lock = fslock::LockFile::open(&path)
        .with_context(|| format!("failed to open lock file at {}", path.display()))?;
    if !lock.try_lock().context("error while checking lock status")? {
        anyhow::bail!("A download is already in progress");
    }

    eprintln!("download lock acquired");

    Ok(lock)
}

pub fn run_download_job(link: &str, cfg: &Config) -> Result<JobSummary> {
    let _lock = acquire_download_lock(cfg)?;
    let db = Arc::new(Mutex::new(Db::open(&cfg.db_path)?));
    
    eprintln! ("db connection made");

    let flat = download::resolve_flat(link, cfg)?;
    let source_playlist_id = flat.first().and_then(|entry| entry.playlist_id.clone());

    if let Some(pid) = &source_playlist_id {
        let title = flat.first().and_then(|entry| entry.playlist_title.as_deref()).unwrap_or("");
        let handle = db.lock().unwrap();
        handle.upsert_source_playlist(pid, title, link)?;
    }

    let ids: Vec<String> = flat.iter().map(|entry| entry.id.clone()).collect();
    let known = {
        let handle = db.lock().unwrap();
        handle.known_ids(&ids)?
    };

    let survivors: Vec<&FlatEntry> = flat.iter().filter(|entry| !known.contains(&entry.id)).collect();
    let skipped = flat.len() - survivors.len();

    if let Some(pid) = &source_playlist_id {
        for entry in flat.iter().filter(|entry| known.contains(&entry.id)) {
            let handle = db.lock().unwrap();
            handle.link_video_source_playlist(&entry.id, pid)?;
        }
    }

    let urls: Vec<String> = survivors.iter().map(|entry| entry.url.clone()).collect();
    let full_meta = download::resolve_full(&urls, cfg)?;

    for meta in &full_meta {
        let handle = db.lock().unwrap();
        handle.insert_pending(meta)?;
        if let Some(pid) = &source_playlist_id {
            handle.link_video_source_playlist(&meta.id, pid)?;
        }
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.concurrency.max(1))
        .build()?;

    let results: Vec<anyhow::Result<()>> = pool.install(|| {
        full_meta.par_iter().map(|meta| {
            {
                let handle = db.lock().unwrap();
                handle.mark_downloading(&meta.id)?;
            }

            match download::download_one(meta, cfg) {
                Ok(file) => {
                    let handle = db.lock().unwrap();
                    handle.mark_complete(&meta.id, &file.audio_path, file.thumbnail_path.as_deref())?;
                    Ok(())
                }
                Err(err) => {
                    let handle = db.lock().unwrap();
                    handle.mark_failed(&meta.id, &err.to_string())?;
                    // Propagate the error so it shows up in results.is_err()
                    Err(err)
                }
            }
        }).collect()
    });

    let downloaded = results.iter().filter(|result| result.is_ok()).count();
    let failed = results.iter().filter(|result| result.is_err()).count();

    Ok(JobSummary {
        total_in_link: flat.len(),
        skipped_duplicate: skipped,
        downloaded,
        failed,
    })
}
