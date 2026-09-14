use std::{collections::HashSet, path::Path};

use anyhow::Result;
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

use crate::download::VideoMeta;

pub struct Db {
    conn: Connection,
}

#[derive(Debug)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub play_count: i64,
    pub weight: f64,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;",
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS source_playlists (
                id              TEXT PRIMARY KEY,
                title           TEXT,
                webpage_url     TEXT NOT NULL,
                created_at      TEXT NOT NULL,
                last_synced_at  TEXT
            );

            CREATE TABLE IF NOT EXISTS videos (
                id              TEXT PRIMARY KEY,
                extractor       TEXT NOT NULL,
                extractor_id    TEXT NOT NULL,
                title           TEXT,
                artist          TEXT,
                album           TEXT,
                duration        REAL,
                upload_date     TEXT,
                webpage_url     TEXT NOT NULL,
                thumbnail_url   TEXT,
                audio_path      TEXT,
                thumbnail_path  TEXT,
                status          TEXT NOT NULL DEFAULT 'pending',
                error           TEXT,
                created_at      TEXT NOT NULL,
                updated_at      TEXT NOT NULL,
                play_count      INTEGER NOT NULL DEFAULT 0,
                last_played_at  TEXT,
                weight          REAL NOT NULL DEFAULT 1.0
            );

            CREATE TABLE IF NOT EXISTS video_source_playlists (
                video_id            TEXT NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
                source_playlist_id  TEXT NOT NULL REFERENCES source_playlists(id) ON DELETE CASCADE,
                PRIMARY KEY (video_id, source_playlist_id)
            );

            CREATE TABLE IF NOT EXISTS tags (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL UNIQUE,
                created_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS video_tags (
                video_id    TEXT NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
                tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                added_at    TEXT NOT NULL,
                position    INTEGER,
                PRIMARY KEY (video_id, tag_id)
            );

            CREATE TABLE IF NOT EXISTS player_state (
                id            INTEGER PRIMARY KEY CHECK (id = 0),
                mode          TEXT NOT NULL,
                sort_key      TEXT,
                filter_json   TEXT NOT NULL,
                seed          INTEGER,
                cursor        INTEGER NOT NULL,
                updated_at    TEXT NOT NULL
            );",
        )?;

        Ok(Self { conn })
    }

    pub fn known_ids(&self, ids: &[String]) -> Result<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }

        let placeholders = vec!["?"; ids.len()].join(", ");
        // Only return videos that have successfully completed (status='complete')
        // Failed videos should be retried, not skipped
        let sql =
            format!("SELECT id FROM videos WHERE id IN ({placeholders}) AND status = 'complete'");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |row| row.get(0))?;

        let mut result = HashSet::new();
        for item in rows {
            result.insert(item?);
        }
        Ok(result)
    }

    pub fn insert_pending(&self, video: &VideoMeta) -> Result<bool> {
        let now = timestamp();
        let rows = self.conn.execute(
            "INSERT OR IGNORE INTO videos (
                id, extractor, extractor_id, title, artist, album, duration,
                upload_date, webpage_url, thumbnail_url, status, created_at, updated_at, weight
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, 1.0)",
            params![
                video.id,
                video.extractor,
                video.extractor_id,
                video.title,
                video.artist,
                video.album,
                video.duration,
                video.upload_date,
                video.webpage_url,
                video.thumbnail_url,
                now,
                now,
            ],
        )?;
        Ok(rows == 1)
    }

    pub fn mark_downloading(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE videos SET status = 'downloading', updated_at = ? WHERE id = ?",
            params![timestamp(), id],
        )?;
        Ok(())
    }

    pub fn mark_complete(
        &self,
        id: &str,
        audio_path: &std::path::Path,
        thumbnail_path: Option<&std::path::Path>,
    ) -> Result<()> {
        let now = timestamp();
        self.conn.execute(
            "UPDATE videos SET status = 'complete', audio_path = ?, thumbnail_path = ?, updated_at = ?, error = NULL WHERE id = ?",
            params![
                audio_path.to_string_lossy().to_string(),
                thumbnail_path.map(|p| p.to_string_lossy().to_string()),
                now,
                id,
            ],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, id: &str, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE videos SET status = 'failed', error = ?, updated_at = ? WHERE id = ?",
            params![error, timestamp(), id],
        )?;
        Ok(())
    }

    pub fn upsert_source_playlist(&self, id: &str, title: &str, url: &str) -> Result<()> {
        let now = timestamp();
        self.conn.execute(
            "INSERT INTO source_playlists (id, title, webpage_url, created_at, last_synced_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                webpage_url = excluded.webpage_url,
                last_synced_at = excluded.last_synced_at",
            params![id, title, url, now, now],
        )?;
        Ok(())
    }

    pub fn link_video_source_playlist(
        &self,
        video_id: &str,
        source_playlist_id: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO video_source_playlists (video_id, source_playlist_id) VALUES (?, ?)",
            params![video_id, source_playlist_id],
        )?;
        Ok(())
    }

    pub fn create_tag(&self, name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tags (name, created_at) VALUES (?, ?)",
            params![name, timestamp()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_video_tag(&self, video_id: &str, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO video_tags (video_id, tag_id, added_at) VALUES (?, ?, ?)",
            params![video_id, tag_id, timestamp()],
        )?;
        Ok(())
    }

    pub fn remove_video_tag(&self, video_id: &str, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM video_tags WHERE video_id = ? AND tag_id = ?",
            params![video_id, tag_id],
        )?;
        Ok(())
    }

    pub fn videos_orphaned_by_tag(&self, tag_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT vt.video_id
             FROM video_tags vt
             WHERE vt.tag_id = ?
               AND NOT EXISTS (
                   SELECT 1 FROM video_tags vt2
                   WHERE vt2.video_id = vt.video_id
                     AND vt2.tag_id != ?
               )",
        )?;

        let rows = stmt.query_map(params![tag_id, tag_id], |row| row.get(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn delete_tag(&self, tag_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM tags WHERE id = ?", params![tag_id])?;
        Ok(())
    }

    pub fn videos_by_tags(&self, tag_ids: &[i64]) -> Result<Vec<VideoMeta>> {
        if tag_ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = vec!["?"; tag_ids.len()].join(", ");
        let sql = format!(
            "SELECT v.id, v.extractor, v.extractor_id, v.title, v.artist, v.album, v.duration, v.upload_date, v.webpage_url, v.thumbnail_url
             FROM videos v
             JOIN video_tags vt ON vt.video_id = v.id
             WHERE vt.tag_id IN ({placeholders})
             GROUP BY v.id"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(tag_ids.iter()), |row| {
            Ok(VideoMeta {
                id: row.get("id")?,
                extractor: row.get("extractor")?,
                extractor_id: row.get("extractor_id")?,
                title: row.get("title")?,
                artist: row.get("artist")?,
                album: row.get("album")?,
                duration: row.get("duration")?,
                upload_date: row.get("upload_date")?,
                webpage_url: row.get("webpage_url")?,
                thumbnail_url: row.get("thumbnail_url")?,
                audio_path: None,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn record_play(&self, video_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE videos SET play_count = play_count + 1, last_played_at = ?, updated_at = ? WHERE id = ?",
            params![timestamp(), timestamp(), video_id],
        )?;
        Ok(())
    }

    pub fn set_weight(&self, video_id: &str, weight: f64) -> Result<()> {
        self.conn.execute(
            "UPDATE videos SET weight = ?, updated_at = ? WHERE id = ?",
            params![weight, timestamp(), video_id],
        )?;
        Ok(())
    }

    pub fn list_tags(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM tags ORDER BY name")?;
        let rows = stmt.query_map([], |row| Ok((row.get("id")?, row.get("name")?)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn list_source_playlists(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title FROM source_playlists ORDER BY title")?;
        let rows = stmt.query_map([], |row| Ok((row.get("id")?, row.get("title")?)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn get_video_meta(&self, id: &str) -> Result<VideoMeta> {
        let mut stmt = self.conn.prepare(
            "SELECT id, extractor, extractor_id, title, artist, album, duration, upload_date, webpage_url, thumbnail_url, audio_path
             FROM videos WHERE id = ?"
        )?;

        let meta = stmt.query_row(params![id], |row| {
            Ok(VideoMeta {
                id: row.get("id")?,
                extractor: row.get("extractor")?,
                extractor_id: row.get("extractor_id")?,
                title: row.get("title")?,
                artist: row.get("artist")?,
                album: row.get("album")?,
                duration: row.get("duration")?,
                upload_date: row.get("upload_date")?,
                webpage_url: row.get("webpage_url")?,
                thumbnail_url: row.get("thumbnail_url")?,
                audio_path: row
                    .get::<_, Option<String>>("audio_path")?
                    .map(std::path::PathBuf::from),
            })
        })?;

        Ok(meta)
    }

    pub fn get_video_info(&self, id: &str) -> Result<Option<VideoInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, artist, album, play_count, weight
             FROM videos WHERE id = ?",
        )?;
        let info = stmt
            .query_row(params![id], |row| {
                Ok(VideoInfo {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    artist: row.get("artist")?,
                    album: row.get("album")?,
                    play_count: row.get("play_count")?,
                    weight: row.get("weight")?,
                })
            })
            .optional()?;
        Ok(info)
    }

    pub fn upsert_player_state(
        &self,
        mode: &str,
        sort_key: Option<&str>,
        filter_json: &str,
        seed: Option<u64>,
        cursor: usize,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO player_state (id, mode, sort_key, filter_json, seed, cursor, updated_at)
             VALUES (0, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                mode = excluded.mode,
                sort_key = excluded.sort_key,
                filter_json = excluded.filter_json,
                seed = excluded.seed,
                cursor = excluded.cursor,
                updated_at = excluded.updated_at",
            params![
                mode,
                sort_key,
                filter_json,
                seed.map(|value| value as i64),
                cursor as i64,
                timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn load_player_state(
        &self,
    ) -> Result<Option<(String, Option<String>, String, Option<i64>, i32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT mode, sort_key, filter_json, seed, cursor FROM player_state WHERE id = 0",
        )?;

        let result = stmt
            .query_row([], |row| {
                Ok((
                    row.get("mode")?,
                    row.get("sort_key")?,
                    row.get("filter_json")?,
                    row.get("seed")?,
                    row.get("cursor")?,
                ))
            })
            .optional()?;

        Ok(result)
    }

    pub fn filtered_video_ids_and_weights(
        &self,
        filter: &crate::cli::FilterSpec,
    ) -> Result<Vec<(String, f64)>> {
        let mut sql = String::from("SELECT v.id, v.weight FROM videos v");

        let mut join_tag = false;
        let mut join_playlist = false;

        // Collect conditions for WHERE clause
        let mut conditions: Vec<String> = vec![];

        let mut params: Vec<String> = vec![];

        if let Some(ref artist) = filter.artist {
            conditions.push("v.artist = ?".to_string());
            params.push(artist.clone());
        }

        if let Some(ref album) = filter.album {
            conditions.push("v.album = ?".to_string());
            params.push(album.clone());
        }

        if let Some(ref playlist) = filter.playlist {
            join_playlist = true;
            conditions.push("sp.title = ?".to_string());
            params.push(playlist.clone());
        }

        if !filter.tags.is_empty() {
            if filter.match_mode == "or" {
                join_tag = true;
                let placeholders = vec!["?"; filter.tags.len()].join(", ");
                conditions.push(format!("t.name IN ({placeholders})"));
                params.extend(filter.tags.iter().cloned());
            } else {
                for tag in &filter.tags {
                    conditions.push(
                        "EXISTS (SELECT 1 FROM video_tags evt JOIN tags et ON et.id = evt.tag_id WHERE evt.video_id = v.id AND et.name = ?)"
                            .to_string(),
                    );
                    params.push(tag.clone());
                }
            }
        }

        // Add JOINs
        if join_playlist {
            sql.push_str(" JOIN video_source_playlists vsp ON v.id = vsp.video_id JOIN source_playlists sp ON vsp.source_playlist_id = sp.id");
        }
        if join_tag {
            sql.push_str(
                " JOIN video_tags vt ON v.id = vt.video_id JOIN tags t ON vt.tag_id = t.id",
            );
        }

        sql.push_str(" WHERE v.status = 'complete'");

        // Combine conditions with match_mode
        if !conditions.is_empty() {
            let where_clause = if filter.match_mode == "or" {
                format!(" AND ({})", conditions.join(" OR "))
            } else {
                format!(" AND ({})", conditions.join(" AND "))
            };
            sql.push_str(&where_clause);
        }

        sql.push_str(" GROUP BY v.id ORDER BY v.weight DESC, v.id");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get("id")?, row.get("weight")?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn filtered_video_ids_sorted(
        &self,
        filter: &crate::cli::FilterSpec,
        sort_key: &str,
        descending: bool,
    ) -> Result<Vec<String>> {
        let mut sql = String::from("SELECT v.id FROM videos v");

        let mut join_tag = false;
        let mut join_playlist = false;
        let mut conditions: Vec<String> = vec![];

        let mut params: Vec<String> = vec![];

        if let Some(ref artist) = filter.artist {
            conditions.push("v.artist = ?".to_string());
            params.push(artist.clone());
        }

        if let Some(ref album) = filter.album {
            conditions.push("v.album = ?".to_string());
            params.push(album.clone());
        }

        if let Some(ref playlist) = filter.playlist {
            join_playlist = true;
            conditions.push("sp.title = ?".to_string());
            params.push(playlist.clone());
        }

        if !filter.tags.is_empty() {
            if filter.match_mode == "or" {
                join_tag = true;
                let placeholders = vec!["?"; filter.tags.len()].join(", ");
                conditions.push(format!("t.name IN ({placeholders})"));
                params.extend(filter.tags.iter().cloned());
            } else {
                for tag in &filter.tags {
                    conditions.push(
                        "EXISTS (SELECT 1 FROM video_tags evt JOIN tags et ON et.id = evt.tag_id WHERE evt.video_id = v.id AND et.name = ?)"
                            .to_string(),
                    );
                    params.push(tag.clone());
                }
            }
        }

        if join_playlist {
            sql.push_str(" JOIN video_source_playlists vsp ON v.id = vsp.video_id JOIN source_playlists sp ON vsp.source_playlist_id = sp.id");
        }
        if join_tag {
            sql.push_str(
                " JOIN video_tags vt ON v.id = vt.video_id JOIN tags t ON vt.tag_id = t.id",
            );
        }

        sql.push_str(" WHERE v.status = 'complete'");

        if !conditions.is_empty() {
            let where_clause = if filter.match_mode == "or" {
                format!(" AND ({})", conditions.join(" OR "))
            } else {
                format!(" AND ({})", conditions.join(" AND "))
            };
            sql.push_str(&where_clause);
        }

        let direction = if descending { "DESC" } else { "ASC" };
        sql.push_str(&format!(
            " GROUP BY v.id ORDER BY v.{} {}, v.id",
            sort_key, direction
        ));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            row.get("id")
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
