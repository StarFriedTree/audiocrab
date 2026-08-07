# TODO checklist

01. [x] Play a single local audio file (hardcoded path)
02. [x] Play a file passed as a CLI argument
03. [x] Pause / resume / stop the currently playing file
04. [x] Volume control
05. [ ] Structured CLI with subcommands (play, pause, stop, etc.)
06. [x] Download a single audio file from a YouTube link
07. [ ] Extract and store metadata for a downloaded file (title, artist/channel, thumbnail)
08. [ ] Combined download-then-play flow for a single link
09. [ ] Default tmp download folder
10. [ ] Configurable size cap + oldest-file cleanup for tmp folder
11. [ ] Persistent app config file (folder paths, cap sizes, defaults)
12. [ ] Local database setup for tracking files/metadata
13. [ ] Playlist represented as its own folder
14. [ ] Batch-download a YouTube playlist into a playlist folder
15. [ ] archive.txt-based duplicate-download prevention on batch update/append
16. [ ] Sequential playback of all files in a playlist folder
17. [ ] Rescan a playlist folder to register manually-added files without disrupting existing entries
18. [ ] Sorting by metadata (title, artist, date added)
19. [ ] Play-count / completed-play tracking per file
20. [ ] Time-played / last-played tracking per file
21. [ ] True random shuffle
22. [ ] Like/favorite marking per file
23. [ ] Weighted shuffle using likes + recency
24. [ ] Search within a playlist
25. [ ] Delete a file (with metadata cleanup)
26. [ ] Delete a whole playlist (with metadata cleanup)
27. [ ] Enforce single active playback session (new selection drops the old one)
28. [ ] Split playback engine into a long-running background process
29. [ ] Basic IPC protocol between a client and the background process (play/pause/stop/status/close)
30. [ ] Launch background process detached from the terminal
31. [ ] Explicit close/exit command for the background process
32. [ ] Wait-for-completion behavior when no close command is given
33. [ ] Streaming playback without downloading first (optional)
34. [ ] Audit and split responsibilities across modules/processes for minimal idle resource use
35. [ ] CLI polish (help text, config overrides, error messages)
36. [ ] Minimal GUI widget window showing current track + basic controls
37. [ ] GUI communicates with the background process instead of embedding playback itself
38. [ ] System tray icon with minimize-to-tray
39. [ ] Translucency, always-on-top, and default corner-docking for the widget
40. [ ] Keyboard shortcuts reachable within the GUI
41. [ ] Global (system-wide) keybindings for playback control
42. [ ] Single-instance enforcement for the GUI (focus existing window instead of relaunching)
43. [ ] Cross-platform audit pass (identify and abstract Windows-specific assumptions for future Linux/macOS support)
