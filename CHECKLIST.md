# TODO checklist

01. [x] Play a single local audio file (hardcoded path)
02. [x] Play a file passed as a CLI argument
03. [x] Pause / resume / stop the currently playing file
04. [x] Volume control
05. [-] Structured CLI with subcommands (play, pause, stop, etc.)
06. [x] Download a single audio file from a YouTube link
07. [-] Extract and store metadata for a downloaded file (title, artist/channel, thumbnail)
08. [ ] Combined download-then-play flow for a single link
09. [ ] Shared library folder for all downloaded files (flat layout for now)
10. [ ] Local database schema: tracks table (path, metadata, play stats, liked flag)
11. [ ] DB-based dedup check by source video ID before downloading (replaces archive.txt)
12. [ ] Persistent app config file (library path, DB path, defaults)
13. [ ] Playlists table + playlist_tracks join table (many-to-many, with position/order)
14. [ ] Batch-download a YouTube playlist: register each track in DB, link to playlist
15. [ ] Playback of a playlist by querying ordered tracks from DB
16. [ ] Rescan library folder to register manually-added files as tracks without disrupting existing DB entries
17. [ ] Sorting playlist/library by metadata via DB query (title, artist, date added)
18. [ ] Play-count / completed-play tracking per track
19. [ ] Time-played / last-played tracking per track
20. [ ] True random shuffle
21. [ ] Like/favorite marking per track
22. [ ] Weighted shuffle using likes + recency
23. [ ] Search within a playlist/library (DB query)
24. [ ] Delete a track (remove from library + DB, cascade from all playlists)
25. [ ] Remove a track from one playlist only (join-table row delete, file and other playlists untouched)
26. [ ] Delete a whole playlist (join-table cleanup, tracks themselves untouched)
27. [ ] Enforce single active playback session (new selection drops the old one)
28. [ ] Split playback engine into a long-running background server process
29. [ ] Client/server IPC via local socket (interprocess crate), connect-or-become-server logic
30. [ ] Basic line/text-based command protocol (play/pause/stop/status/skip/name/close)
31. [ ] Launch server detached from the terminal (Windows: respawn detached / CREATE_NO_WINDOW)
32. [ ] Explicit close/exit command for the server
33. [ ] Wait-for-completion behavior when no close command is given
34. [ ] Streaming playback without downloading first (optional)
35. [ ] Directory sharding for library folder if track count grows large (optional, later)
36. [ ] Audit and split responsibilities across modules/processes for minimal idle resource use
37. [ ] CLI polish (help text, config overrides, error messages)
38. [ ] Minimal GUI widget window showing current track + basic controls
39. [ ] GUI communicates with the background server instead of embedding playback itself
40. [ ] System tray icon with minimize-to-tray
41. [ ] Translucency, always-on-top, and default corner-docking for the widget
42. [ ] Keyboard shortcuts reachable within the GUI
43. [ ] Global (system-wide) keybindings for playback control
44. [ ] Single-instance enforcement for the GUI (focus existing window instead of relaunching)
45. [ ] Dependency-check logic for yt-dlp/ffmpeg with OS-specific install guidance (deferred from current PATH-assumption)
46. [ ] Cross-platform audit pass (identify and abstract Windows-specific assumptions for future Linux/macOS support)
