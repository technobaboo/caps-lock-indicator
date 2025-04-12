// SPDX-License-Identifier: MIT

use cosmic::{
    app::Task,
    iced::{futures::SinkExt, stream, Alignment, Length, Subscription},
    iced_widget::row,
    widget::{autosize::autosize, vertical_space, Id},
};
use std::{sync::Arc, time::Duration};
use x11rb::protocol::{xkb::ConnectionExt, xproto::ModMask};

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
        Subscription::run(|| {
            stream::channel(10, |mut output| async move {
                let (x_connection, _) = match x11rb::connect(None) {
                    Ok(conn) => conn,
                    Err(e) => {
                        let _ = output.send(Message::Update(Err(e.to_string())));
                        return;
                    }
                };

                let xkb_extension_cookie = match x_connection.xkb_use_extension(1, 0) {
                    Ok(cookie) => cookie,
                    Err(e) => {
                        let _ = output.send(Message::Update(Err(e.to_string())));
                        return;
                    }
                };
                let _xkb_extension_accepted = match xkb_extension_cookie.reply() {
                    Ok(ext) => ext,
                    Err(e) => {
                        let _ = output.send(Message::Update(Err(e.to_string())));
                        return;
                    }
                };

                let x_connection = Arc::new(x_connection);

                loop {
                    let x_connection = x_connection.clone();
                    let caps_active = tokio::task::spawn_blocking(move || {
                        let state_cookie = match x_connection.xkb_get_state(0x100) {
                            Ok(cookie) => cookie,
                            Err(e) => {
                                return Err(e.to_string());
                            }
                        };
                        let state = match state_cookie.reply() {
                            Ok(state) => state,
                            Err(e) => {
                                return Err(e.to_string());
                            }
                        };
                        let caps_active = state.locked_mods.contains(ModMask::LOCK);
                        // println!("updated caps to {caps_active}");
                        Ok(caps_active)
                    })
                    .await
                    .map_err(|e| e.to_string());
                    let _ = output
                        .send(Message::Update(match caps_active {
                            Ok(Ok(c)) => Ok(c),
                            Ok(Err(e)) => Err(e),
                            Err(e) => Err(e),
                        }))
                        .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
        })
    }
}
