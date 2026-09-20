use crate::{
    cli::Commands,
    ipc,
    server::{ControllerMessage, ControllerRequest, ServerHandle},
};
use iced::{
    Element, Task,
    widget::{button, column, text},
};
use std::sync::mpsc;

#[derive(Debug, Clone)]
enum Message {
    Command(Commands),
}

struct App {
    controller_tx: mpsc::Sender<ControllerMessage>,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

pub fn run(handle: ServerHandle) -> anyhow::Result<()> {
    let controller_tx = handle.controller_tx;
    let shutdown = handle.shutdown;
    iced::application(
        move || {
            (
                App {
                    controller_tx: controller_tx.clone(),
                    shutdown: shutdown.clone(),
                },
                Task::none(),
            )
        },
        App::update,
        App::view,
    )
        .title("AudioCrab")
        .exit_on_close_request(false)
        .run()
        .map_err(|error| anyhow::anyhow!("iced application failed: {error}"))
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        let Message::Command(command) = message;
        let (response_tx, _response_rx) = mpsc::channel::<ipc::Response>();
        let _ = self.controller_tx.send(ControllerMessage::Ipc(ControllerRequest {
            command,
            response_tx,
        }));
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        column![
            text("AudioCrab"),
            button("Previous").on_press(Message::Command(Commands::Previous)),
            button("Play").on_press(Message::Command(Commands::Play)),
            button("Pause").on_press(Message::Command(Commands::Pause)),
            button("Next").on_press(Message::Command(Commands::Next)),
        ]
        .spacing(12)
        .padding(24)
        .into()
    }
}
