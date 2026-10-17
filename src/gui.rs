use crate::{
    cli::Commands,
    ipc,
    server::{ControllerMessage, ControllerRequest, PlayerStatus, ServerHandle, StatusEvent},
};
use futures::StreamExt;
use futures::channel::mpsc as async_mpsc;
use iced::window::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use iced::{
    Element, Subscription, Task,
    widget::{button, column, row, text},
    window,
};
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuId, MenuItem},
};

#[derive(Debug, Clone)]
enum Message {
    Command(Commands),
    Status(StatusEvent),
    WindowId(Option<window::Id>),
    Hwnd(Option<isize>),
    CloseRequested(window::Id),
    Tray(TrayAction),
}

#[derive(Debug, Clone, Copy)]
enum TrayAction {
    Show,
    Quit,
}

struct App {
    controller_tx: mpsc::Sender<ControllerMessage>,
    #[allow(dead_code)]
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    status_rx: Arc<Mutex<Option<async_mpsc::UnboundedReceiver<StatusEvent>>>>,
    tray_rx: Arc<Mutex<Option<async_mpsc::UnboundedReceiver<TrayAction>>>>,
    status: PlayerStatus,
    window_id: Option<window::Id>,
    media: Option<MediaControls>,
    media_requested: bool,
    paused: Arc<AtomicBool>,
}

pub fn run(handle: ServerHandle) -> anyhow::Result<()> {
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Console::FreeConsole();
    }

    let controller_tx = handle.controller_tx;
    let shutdown = handle.shutdown;
    let status_rx = Arc::new(Mutex::new(Some(handle.status_rx)));

    let (tray_action_tx, tray_action_rx) = async_mpsc::unbounded::<TrayAction>();

    let show_item = MenuItem::new("Show", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let show_id: MenuId = show_item.id().clone();
    let quit_id: MenuId = quit_item.id().clone();
    let tray_menu = Menu::new();
    tray_menu
        .append(&show_item)
        .expect("failed to build tray menu");
    tray_menu
        .append(&quit_item)
        .expect("failed to build tray menu");
    let icon =
        Icon::from_rgba(vec![255u8; 32 * 32 * 4], 32, 32).expect("failed to build tray icon image");
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("AudioCrab")
        .with_icon(icon)
        .build()
        .expect("failed to create tray icon");

    std::thread::spawn(move || {
        while let Ok(event) = MenuEvent::receiver().recv() {
            let action = if event.id == show_id {
                Some(TrayAction::Show)
            } else if event.id == quit_id {
                Some(TrayAction::Quit)
            } else {
                None
            };
            if let Some(action) = action {
                if tray_action_tx.unbounded_send(action).is_err() {
                    break;
                }
            }
        }
    });
    let tray_rx = Arc::new(Mutex::new(Some(tray_action_rx)));

    let result = iced::application(
        move || {
            (
                App {
                    controller_tx: controller_tx.clone(),
                    shutdown: shutdown.clone(),
                    status_rx: Arc::clone(&status_rx),
                    tray_rx: Arc::clone(&tray_rx),
                    status: PlayerStatus::default(),
                    window_id: None,
                    media: None,
                    media_requested: false,
                    paused: Arc::new(AtomicBool::new(false)),
                },
                window::latest().map(Message::WindowId),
            )
        },
        App::update,
        App::view,
    )
    .title("AudioCrab")
    .exit_on_close_request(false)
    .subscription(App::subscription)
    .run()
    .map_err(|error| anyhow::anyhow!("iced application failed: {error}"));

    drop(tray_icon);
    result
}

impl App {
    fn send_command(&self, command: Commands) {
        send_to(&self.controller_tx, command);
    }

    fn sync_media(&mut self) {
        let Some(media) = self.media.as_mut() else {
            return;
        };
        let _ = media.set_metadata(MediaMetadata {
            title: self.status.title.as_deref(),
            artist: self.status.artist.as_deref(),
            ..Default::default()
        });
        let _ = media.set_playback(if self.status.is_paused {
            MediaPlayback::Paused { progress: None }
        } else {
            MediaPlayback::Playing { progress: None }
        });
    }

    fn init_media(&mut self, hwnd: isize) {
        let config = PlatformConfig {
            dbus_name: "audiocrab",
            display_name: "AudioCrab",
            hwnd: Some(hwnd as *mut std::ffi::c_void),
        };
        let Ok(mut media) = MediaControls::new(config) else {
            return;
        };
        let tx = self.controller_tx.clone();
        let paused = Arc::clone(&self.paused);
        let _ = media.attach(move |event| {
            let command = match event {
                MediaControlEvent::Play => Commands::Play,
                MediaControlEvent::Pause | MediaControlEvent::Stop => Commands::Pause,
                MediaControlEvent::Toggle => {
                    if paused.load(Ordering::Relaxed) {
                        Commands::Play
                    } else {
                        Commands::Pause
                    }
                }
                MediaControlEvent::Next => Commands::Next,
                MediaControlEvent::Previous => Commands::Previous,
                MediaControlEvent::Quit => Commands::Quit,
                _ => return,
            };
            send_to(&tx, command);
        });
        self.media = Some(media);
        self.sync_media();
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Command(command) => {
                self.send_command(command);
                Task::none()
            }
            Message::Status(StatusEvent::Update(status)) => {
                let changed = self.status.title != status.title
                    || self.status.artist != status.artist
                    || self.status.is_paused != status.is_paused;
                self.status = status;
                self.paused.store(self.status.is_paused, Ordering::Relaxed);
                if changed {
                    self.sync_media();
                }
                Task::none()
            }
            Message::Status(StatusEvent::ShuttingDown) => iced::exit(),
            Message::WindowId(id) => {
                self.window_id = id;
                match id {
                    Some(id) if !self.media_requested => {
                        self.media_requested = true;
                        window::run(id, hwnd_of).map(Message::Hwnd)
                    }
                    _ => Task::none(),
                }
            }
            Message::Hwnd(hwnd) => {
                if let Some(hwnd) = hwnd {
                    self.init_media(hwnd);
                }
                Task::none()
            }
            Message::CloseRequested(id) => {
                self.window_id = Some(id);
                window::set_mode(id, window::Mode::Hidden)
            }
            Message::Tray(TrayAction::Show) => match self.window_id {
                Some(id) => window::set_mode(id, window::Mode::Windowed),
                None => Task::none(),
            },
            Message::Tray(TrayAction::Quit) => {
                self.send_command(Commands::Quit);
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            Subscription::run_with(StatusRx(Arc::clone(&self.status_rx)), |data| {
                futures::stream::iter(data.0.lock().unwrap().take())
                    .flatten()
                    .map(Message::Status)
            }),
            Subscription::run_with(TrayRx(Arc::clone(&self.tray_rx)), |data| {
                futures::stream::iter(data.0.lock().unwrap().take())
                    .flatten()
                    .map(Message::Tray)
            }),
            window::open_events().map(|id| Message::WindowId(Some(id))),
            window::close_requests().map(Message::CloseRequested),
        ])
    }

    fn view(&self) -> Element<'_, Message> {
        let now_playing = match (&self.status.title, &self.status.artist) {
            (Some(title), Some(artist)) => format!("{title} — {artist}"),
            (Some(title), None) => title.clone(),
            _ => "Nothing playing".to_string(),
        };
        let status_line = format!(
            "{} | {} / {} | vol {:.0}%",
            if self.status.is_paused {
                "Paused"
            } else {
                "Playing"
            },
            self.status.queue_position + 1,
            self.status.queue_length.max(1),
            self.status.volume * 100.0,
        );
        column![
            text("AudioCrab"),
            text(now_playing),
            text(status_line),
            row![
                button("Previous").on_press(Message::Command(Commands::Previous)),
                button("Play").on_press(Message::Command(Commands::Play)),
                button("Pause").on_press(Message::Command(Commands::Pause)),
                button("Next").on_press(Message::Command(Commands::Next)),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .padding(24)
        .into()
    }
}

fn send_to(tx: &mpsc::Sender<ControllerMessage>, command: Commands) {
    let (response_tx, _response_rx) = mpsc::channel::<ipc::Response>();
    let _ = tx.send(ControllerMessage::Ipc(ControllerRequest {
        command,
        response_tx,
    }));
}

fn hwnd_of(window: &dyn window::Window) -> Option<isize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

struct StatusRx(Arc<Mutex<Option<async_mpsc::UnboundedReceiver<StatusEvent>>>>);

impl Hash for StatusRx {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

struct TrayRx(Arc<Mutex<Option<async_mpsc::UnboundedReceiver<TrayAction>>>>);

impl Hash for TrayRx {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}
