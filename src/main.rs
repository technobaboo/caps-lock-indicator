// SPDX-License-Identifier: MIT

use std::sync::{Arc, Mutex};

use cosmic::{
    app::Task,
    iced::{Alignment, Length},
    iced_widget::row,
    widget::{autosize::autosize, vertical_space, Id},
    Action,
};
use zbus::Connection;
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<CapsLockIndicator>(())
}

async fn polkit_authorize() -> Result<evdev::Device, String> {
    let connection = Connection::session().await.map_err(|e| e.to_string())?;
    let proxy = AuthorityProxy::new(&connection)
        .await
        .map_err(|e| e.to_string())?;
    let subject =
        Subject::new_for_owner(std::process::id(), None, None).map_err(|e| e.to_string())?;

    let result = proxy
        .check_authorization(
            &subject,
            "dev.tking.CapsLockCheck",
            &std::collections::HashMap::new(),
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        )
        .await
        .map_err(|e| e.to_string())?;

    if !result.is_authorized {
        return Err(format!("{:?}", result.details));
    }

    let evdev = evdev::Device::open("/dev/input/event11").map_err(|e| e.to_string())?;
    Ok(evdev)
}

pub struct CapsLockIndicator {
    core: cosmic::Core,
    evdev: Result<evdev::Device, String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    PolkitResult(Arc<Mutex<Option<Result<evdev::Device, String>>>>),
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
            evdev: Err("Not yet authorized".to_string()),
        };

        (
            applet,
            Task::future(async move {
                Action::App(Message::PolkitResult(Arc::new(Mutex::new(Some(
                    polkit_authorize().await,
                )))))
            }),
        )
    }

    fn view(&self) -> cosmic::Element<Message> {
        let text = self.core.applet.text(match &self.evdev {
            Ok(_) => "Scanning...",
            Err(e) => e.as_str(),
        });

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
            Message::PolkitResult(device) => self.evdev = device.lock().unwrap().take().unwrap(),
        };

        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
