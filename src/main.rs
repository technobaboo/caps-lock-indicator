// SPDX-License-Identifier: MIT

use cosmic::{
    app::Task,
    iced::{
        futures::{SinkExt, StreamExt},
        stream, Alignment, Length, Subscription,
    },
    iced_widget::row,
    widget::{autosize::autosize, vertical_space, Id},
};
use inotify::{EventMask, WatchMask};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{sync::watch, task::JoinSet};

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<CapsLockIndicator>(())
}

pub struct CapsLockIndicator {
    core: cosmic::Core,
    caps_active: Result<bool, String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Update(Result<bool, String>),
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
        };

        (applet, Task::none())
    }

    fn view(&self) -> cosmic::Element<Message> {
        let text = self.core.applet.text(
            match &self.caps_active {
                Ok(true) => "CAPS",
                Ok(false) => "",
                Err(e) => e.as_str(),
            }
            .to_string(),
        );

        let content = row!(
            text,
            vertical_space().height(Length::Fixed(
                (self.core.applet.suggested_size(true).1
                    + 2 * self.core.applet.suggested_padding(true)) as f32
            ))
        )
        .align_y(Alignment::Center);
        let button = cosmic::widget::button::custom(content)
            .padding([0, self.core.applet.suggested_padding(true)])
            .class(cosmic::theme::Button::AppletIcon);

        autosize(button, Id::new("autosize-main")).into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Update(caps_active) => {
                self.caps_active = caps_active;
            }
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

async fn poll_method(mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>) {
    let (folders_tx, folders) = watch::channel(HashSet::<PathBuf>::default());

    let folders_watch_loop = tokio::task::spawn(async move {
        loop {
            let Ok(mut read_dir) = tokio::fs::read_dir("/sys/class/leds").await else {
                continue;
            };

            let mut folders = HashSet::<PathBuf>::default();
            while let Ok(Some(dir)) = read_dir.next_entry().await {
                if dir.file_name().to_str().unwrap().ends_with("::capslock") {
                    folders.insert(dir.path().join("brightness"));
                }
            }

            let _ = folders_tx.send(folders);
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });

    loop {
        let mut join_set = JoinSet::new();

        for folder in folders.borrow().iter().cloned() {
            join_set.spawn(tokio::fs::read_to_string(folder));
        }

        let mut caps_brightness = 0_usize;
        while let Some(folder_read) = join_set.join_next().await {
            dbg!(&folder_read);
            let Ok(Ok(brightness)) = folder_read else {
                continue;
            };
            if brightness.starts_with('1') {
                caps_brightness += 1;
                break;
            }
        }

        let _ = output.send(Message::Update(Ok(caps_brightness > 0))).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
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
