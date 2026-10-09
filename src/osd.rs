// SPDX-License-Identifier: MIT

//! Bottom-center on-screen display shown when Caps Lock is toggled.
//!
//! Applets talk to the compositor through cosmic-panel, which doesn't forward
//! layer surfaces, so the OSD runs as a separate process connected directly to
//! the compositor. It exits when the applet that spawned it closes its stdin.

use cosmic::{
    app::Task,
    iced::{
        platform_specific::{
            runtime::wayland::layer_surface::{IcedMargin, SctkLayerSurfaceSettings},
            shell::wayland::commands::layer_surface::{
                destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity, Layer,
            },
        },
        stream, window, Alignment, Subscription,
    },
};
use cosmic::widget::autosize::autosize;
use std::{io::Read, time::Duration};

use crate::{poll_method, Message, CAPS_LOCK_ICON};

/// How long the on-screen display stays visible after Caps Lock is toggled.
const OSD_TIMEOUT: Duration = Duration::from_millis(1500);

pub fn run() -> cosmic::iced::Result {
    // Exit together with the applet: its end of our stdin closes when it dies.
    std::thread::spawn(|| {
        let _ = std::io::stdin().read_to_end(&mut Vec::new());
        std::process::exit(0);
    });

    let settings = cosmic::app::Settings::default()
        .no_main_window(true)
        .exit_on_close(false);
    cosmic::app::run::<CapsLockOsd>(settings, ())
}

pub struct CapsLockOsd {
    core: cosmic::Core,
    caps_active: Option<bool>,
    osd: Option<window::Id>,
    /// Bumped on every toggle so only the latest timeout closes the OSD.
    osd_generation: u64,
}

impl cosmic::Application for CapsLockOsd {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "dev.tking.caps-lock-indicator.osd";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: ()) -> (Self, Task<Message>) {
        let osd = CapsLockOsd {
            core,
            caps_active: None,
            osd: None,
            osd_generation: 0,
        };
        (osd, Task::none())
    }

    fn view(&self) -> cosmic::Element<Message> {
        cosmic::widget::Space::new(0, 0).into()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Update(Ok(caps_active)) => {
                // The first reading only records the state, so nothing pops up at login.
                let previous = self.caps_active.replace(caps_active);
                if previous.is_some_and(|previous| previous != caps_active) {
                    return self.show();
                }
            }
            Message::Update(Err(_)) => {}
            Message::HideOsd(generation) => {
                if generation == self.osd_generation {
                    if let Some(id) = self.osd.take() {
                        return destroy_layer_surface(id);
                    }
                }
            }
        }
        Task::none()
    }

    fn view_window(&self, id: window::Id) -> cosmic::Element<Message> {
        if self.osd != Some(id) {
            return cosmic::widget::Space::new(0, 0).into();
        }

        let icon = cosmic::widget::icon(
            cosmic::widget::icon::from_svg_bytes(CAPS_LOCK_ICON)
                .symbolic(true)
                .into(),
        )
        .size(48);
        let label = cosmic::widget::text::title4(if self.caps_active == Some(true) {
            "Caps Lock ON"
        } else {
            "Caps Lock OFF"
        });

        let content = cosmic::widget::column::with_children(vec![icon.into(), label.into()])
            .spacing(12)
            .align_x(Alignment::Center);

        let osd = cosmic::widget::container(content)
            .padding([24, 32])
            .class(cosmic::theme::Container::custom(|theme| {
                let cosmic = theme.cosmic();
                cosmic::iced::widget::container::Style {
                    text_color: Some(cosmic.background.on.into()),
                    icon_color: Some(cosmic.background.on.into()),
                    background: Some(cosmic::iced::Color::from(cosmic.background.base).into()),
                    border: cosmic::iced::Border {
                        radius: cosmic.corner_radii.radius_m.into(),
                        width: 1.0,
                        color: cosmic.background.divider.into(),
                    },
                    shadow: Default::default(),
                }
            }));

        // Sizes the layer surface to its contents; without it the surface stays 1x1.
        autosize(osd, cosmic::widget::Id::new("osd-autosize")).into()
    }

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run(|| stream::channel(100, poll_method))
    }
}

impl CapsLockOsd {
    /// Opens (or keeps open) the OSD and schedules it to close.
    fn show(&mut self) -> Task<Message> {
        self.osd_generation += 1;
        let generation = self.osd_generation;
        let hide = Task::perform(tokio::time::sleep(OSD_TIMEOUT), move |_| {
            cosmic::Action::App(Message::HideOsd(generation))
        });

        if self.osd.is_some() {
            return hide;
        }

        let id = window::Id::unique();
        self.osd = Some(id);
        let open = get_layer_surface(SctkLayerSurfaceSettings {
            id,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::None,
            pointer_interactivity: false,
            anchor: Anchor::BOTTOM,
            namespace: "caps-lock-osd".into(),
            margin: IcedMargin {
                bottom: 48,
                ..Default::default()
            },
            size: None,
            ..Default::default()
        });

        Task::batch([open, hide])
    }
}
