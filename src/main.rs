// SPDX-License-Identifier: MIT

mod osd;

use cosmic::{
    app::Task,
    iced::{
        stream, Alignment, Length, Subscription,
    },
    iced_widget::row,
    widget::{autosize::autosize, vertical_space, Id},
};
use inotify::{EventMask, WatchMask};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const CAPS_LOCK_ICON: &[u8] =
    include_bytes!("../resources/icons/hicolor/scalable/status/caps-lock-symbolic.svg");

fn main() -> cosmic::iced::Result {
    if std::env::args().any(|arg| arg == "--osd") {
        return osd::run();
    }
    cosmic::applet::run::<CapsLockIndicator>(())
}

pub struct CapsLockIndicator {
    core: cosmic::Core,
    caps_active: Result<bool, String>,
    /// The OSD process; it exits when this handle (and its stdin pipe) is dropped.
    _osd: Option<Child>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Update(Result<bool, String>),
    HideOsd(u64),
}

/// Starts the OSD connected straight to the compositor instead of through the panel.
fn spawn_osd() -> Option<Child> {
    let exe = std::env::current_exe().ok()?;
    Command::new(exe)
        .arg("--osd")
        .env_remove("WAYLAND_SOCKET")
        .stdin(Stdio::piped())
        .spawn()
        .inspect_err(|e| eprintln!("Failed to start OSD: {e}"))
        .ok()
}

impl cosmic::Application for CapsLockIndicator {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "dev.tking.caps-lock-indicator";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: (),
    ) -> (CapsLockIndicator, cosmic::Task<cosmic::Action<Message>>) {
        let applet = CapsLockIndicator {
            core,
            caps_active: Ok(false),
            _osd: spawn_osd(),
        };

        (applet, Task::none())
    }

    fn view(&self) -> cosmic::Element<Message> {
        match &self.caps_active {
            Ok(true) => {
                let icon = cosmic::widget::icon::from_svg_bytes(CAPS_LOCK_ICON).symbolic(true);
                autosize(
                    self.core.applet.icon_button_from_handle(icon),
                    Id::new("autosize-main"),
                )
                .into()
            }
            Ok(false) => cosmic::widget::Space::new(0, 0).into(),
            Err(e) => {
                let content = row!(
                    self.core.applet.text(e.clone()),
                    vertical_space().height(Length::Fixed(
                        (self.core.applet.suggested_size(true).1
                            + 2 * self.core.applet.suggested_padding(true))
                            as f32
                    ))
                )
                .align_y(Alignment::Center);
                let button = cosmic::widget::button::custom(content)
                    .padding([0, self.core.applet.suggested_padding(true)])
                    .class(cosmic::theme::Button::AppletIcon);

                autosize(button, Id::new("autosize-main")).into()
            }
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Update(caps_active) => {
                self.caps_active = caps_active;
            }
            Message::HideOsd(_) => {}
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::run(|| stream::channel(100, poll_method))
    }
}

pub(crate) async fn poll_method(mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>) {
    // Poll on a plain thread with blocking reads: sysfs reads are tiny, and this
    // keeps the UI thread asleep until the state actually changes.
    std::thread::spawn(move || {
        let mut leds = Vec::new();
        let mut last_scan = None::<Instant>;
        let mut last_sent = None;
        loop {
            // Keyboards can be plugged in or out, so rescan the LEDs every second.
            if last_scan.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                last_scan = Some(Instant::now());
                leds = capslock_leds();
            }

            let caps_active = leds.iter().any(|led| {
                std::fs::read_to_string(led).is_ok_and(|brightness| brightness.starts_with('1'))
            });

            if last_sent != Some(caps_active) {
                last_sent = Some(caps_active);
                if output.try_send(Message::Update(Ok(caps_active))).is_err() {
                    // The subscription is gone, so nobody is listening anymore.
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    std::future::pending::<()>().await;
}

/// Brightness files of every Caps Lock LED, one per keyboard.
fn capslock_leds() -> Vec<PathBuf> {
    let Ok(read_dir) = std::fs::read_dir("/sys/class/leds") else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .filter(|dir| dir.file_name().to_string_lossy().ends_with("::capslock"))
        .map(|dir| dir.path().join("brightness"))
        .collect()
}
// async fn inotify_method(mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>) {
//     let inotify_getter = async || {
//         let inotify = inotify::Inotify::init().map_err(|e| e.to_string())?;
//         inotify
//             .watches()
//             .add(
//                 // "/sys/class/leds/input",
//                 Path::new("/sys/class/leds/input11::capslock")
//                     .canonicalize()
//                     .unwrap(),
//                 WatchMask::MODIFY | WatchMask::CREATE | WatchMask::DELETE,
//             )
//             .map_err(|e| e.to_string())?;

//         let event_stream = inotify
//             .into_event_stream([0_u8; 4096])
//             .map_err(|e| e.to_string())?;

//         Ok::<_, String>(event_stream)
//     };
//     let mut event_stream = match inotify_getter().await {
//         Ok(r) => r,
//         Err(e) => {
//             let _ = output.send(Message::Update(Err(e.to_string()))).await;
//             eprintln!("Failed to get event stream: {e}");
//             return;
//         }
//     };

//     let mut led_states: HashMap<PathBuf, bool> = Default::default();

//     loop {
//         while let Some(event) = event_stream.next().await {
//             dbg!(&event);
//             let Ok(event) = event else {
//                 eprintln!("Failed to get event");
//                 continue;
//             };
//             let Some(name) = event.name.as_ref().and_then(|s| s.to_str()) else {
//                 eprintln!("Couldn't get event name");
//                 continue;
//             };
//             if !name.ends_with("::capslock/brightness") {
//                 eprintln!("Name not caps lock brightness, it was: {name}");
//                 continue;
//             }

//             if event.mask.contains(EventMask::CREATE) || event.mask.contains(EventMask::MODIFY) {
//                 let Ok(led_active) = tokio::fs::read_to_string(name).await else {
//                     eprintln!("Couldn't read file: {name}");
//                     continue;
//                 };
//                 let entry = led_states
//                     .entry(PathBuf::from(name.to_string()))
//                     .or_default();
//                 *entry = led_active == "1";
//             } else if event.mask.contains(EventMask::DELETE) {
//                 led_states.remove(Path::new(name));
//             }

//             let any_led_active = led_states.values().any(|v| *v);

//             let _ = output.send(Message::Update(Ok(any_led_active))).await;
//         }
//     }
// }
