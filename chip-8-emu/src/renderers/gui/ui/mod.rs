use iced::futures::stream::StreamExt;
use std::thread::ScopedJoinHandle;
use iced::{event, keyboard, mouse, widget, window, Alignment, Background, Border, Color, Element, Event, Length, Point, Rectangle, Size, Subscription, Task, Theme};
use iced::keyboard::key;
use iced::keyboard::key::Code;
use iced::widget::{button, canvas, container, responsive, text, Container};
use chip_8_core::{Display, Fault, InputIndex};
use crate::renderers::gui::{GuiCommand, GuiEvent};
use crate::renderers::gui::ui::keymap::{Action, Keymap};
use crate::rgb::Rgb;

mod keymap;

#[derive(Clone)]
enum Message {
    Command(GuiCommand),
    KeyPressed(key::Physical),
    KeyReleased(key::Physical),
    ListenToRebindFor(Action),
    Quit,
}

#[allow(
    clippy::large_enum_variant,
    reason = "99.999% of the time, the render state is set to Display"
)]
pub enum GuiRenderState {
    Display(Display),
    Fault(Fault)
}

pub struct SettingsOverlayState {
    remap_listening: Option<Action>,
    escape_key_released: bool,
}

struct GuiApp {
    _event_tx: flume::Sender<GuiEvent>,
    render_state: GuiRenderState,
    settings_overlay: Option<SettingsOverlayState>,
    keymap: Keymap,
    fg: Color,
    bg: Color,
    border: Color
}


#[derive(Clone)]
struct Chip8Canvas<'a>{
    display: &'a Display,
    fg: Color,
    bg: Color,
    border: Color
}

impl<'a, Message> canvas::Program<Message> for Chip8Canvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let display = self.display;

        let src_w = f32::from(Display::VIDEO_WIDTH);
        let src_h = f32::from(Display::VIDEO_HEIGHT);

        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.border);

        // I use integer scale only, so the framebuffer stays crisp.
        // otherwise it gets blurry and starts looking not so chip-8-emu like

        let pixel = (bounds.width / src_w)
            .min(bounds.height / src_h)
            .floor()
            .max(1.0);

        let out_w = src_w * pixel;
        let out_h = src_h * pixel;

        // Center the scaled screen and snap to whole pixels.
        let origin = Point::new(
            ((bounds.width - out_w) * 0.5).floor(),
            ((bounds.height - out_h) * 0.5).floor(),
        );

        // Draw the screen background inside the border-filled canvas.
        frame.fill_rectangle(origin, Size::new(out_w, out_h), self.bg);

        // Draw lit pixels.
        for y in 0..Display::VIDEO_HEIGHT {
            for x in 0..Display::VIDEO_WIDTH {
                if display.get(x, y) {
                    frame.fill_rectangle(
                        Point::new(
                            origin.x + f32::from(x) * pixel,
                            origin.y + f32::from(y) * pixel,
                        ),
                        Size::new(pixel, pixel),
                        self.fg,
                    );
                }
            }
        }

        vec![frame.into_geometry()]
    }
}


const fn key_label(key: key::Physical) -> &'static str {
    macro_rules! super_key {
        ($(prefix: $prefix: literal)?) => {
            concat!($($prefix,)? cfg_select! {
                windows => "⊞",
                target_vendor = "apple" => "⌘",
                _ => "❖",
            })
        };
    }

    macro_rules! alt_key {
        ($(prefix: $prefix: literal)?) => {
            concat!($($prefix,)? cfg_select! {
                target_vendor = "apple" => "⌥",
                _ => "⎇",
            })
        };
    }


    match key {
        key::Physical::Code(code) => match code {
            Code::Backquote => "`",
            Code::Digit1 => "1",
            Code::Digit2 => "2",
            Code::Digit3 => "3",
            Code::Digit4 => "4",
            Code::Digit5 => "5",
            Code::Digit6 => "6",
            Code::Digit7 => "7",
            Code::Digit8 => "8",
            Code::Digit9 => "9",
            Code::Digit0 => "0",
            Code::Minus => "-",
            Code::Equal => "=",
            Code::Backspace => "⌫",

            Code::Tab => "⭾",
            Code::KeyQ => "Q",
            Code::KeyW => "W",
            Code::KeyE => "E",
            Code::KeyR => "R",
            Code::KeyT => "T",
            Code::KeyY => "Y",
            Code::KeyU => "U",
            Code::KeyI => "I",
            Code::KeyO => "O",
            Code::KeyP => "P",
            Code::BracketLeft => "[",
            Code::BracketRight => "]",
            Code::Backslash => "\\",

            Code::CapsLock => "⇪",
            Code::KeyA => "A",
            Code::KeyS => "S",
            Code::KeyD => "D",
            Code::KeyF => "F",
            Code::KeyG => "G",
            Code::KeyH => "H",
            Code::KeyJ => "J",
            Code::KeyK => "K",
            Code::KeyL => "L",
            Code::Semicolon => ";",
            Code::Quote => "'",
            Code::Enter => "⏎",

            Code::ShiftLeft => "L⇧",
            Code::KeyZ => "Z",
            Code::KeyX => "X",
            Code::KeyC => "C",
            Code::KeyV => "V",
            Code::KeyB => "B",
            Code::KeyN => "N",
            Code::KeyM => "M",
            Code::Comma => ",",
            Code::Period => ".",
            Code::Slash => "/",
            Code::ShiftRight => "R⇧",

            Code::ControlLeft => "L⌃",
            Code::SuperLeft => super_key!(prefix: "L"),
            Code::AltLeft => alt_key!(prefix: "L"),
            Code::Space => "␣",
            Code::Meta | Code::Hyper => super_key!(),
            Code::AltRight => alt_key!(prefix: "R"),
            Code::SuperRight => super_key!(prefix: "R"),
            Code::ContextMenu => "fn",
            Code::ControlRight => "R⌃",


            Code::F1 => "F1",
            Code::F2 => "F2",
            Code::F3 => "F3",
            Code::F4 => "F4",
            Code::F5 => "F5",
            Code::F6 => "F6",
            Code::F7 => "F7",
            Code::F8 => "F8",
            Code::F9 => "F9",
            Code::F10 => "F10",
            Code::F11 => "F11",
            Code::F12 => "F12",

            Code::Insert => "⎀",
            Code::Delete => "⌦",
            Code::Home => "⇱",
            Code::End => "⇲",
            Code::PageUp => "⇞",
            Code::PageDown => "⇟",

            Code::ArrowUp => "↑",
            Code::ArrowDown => "↓",
            Code::ArrowLeft => "←",
            Code::ArrowRight => "→",

            Code::NumLock => "N🔒",
            Code::NumpadDivide => "N/",
            Code::NumpadMultiply => "N*",
            Code::NumpadSubtract => "N-",
            Code::NumpadAdd => "N+",
            Code::NumpadEnter => "N⏎",
            Code::Numpad1 => "N1",
            Code::Numpad2 => "N2",
            Code::Numpad3 => "N3",
            Code::Numpad4 => "N4",
            Code::Numpad5 => "N5",
            Code::Numpad6 => "N6",
            Code::Numpad7 => "N7",
            Code::Numpad8 => "N8",
            Code::Numpad9 => "N9",
            Code::Numpad0 => "N0",
            Code::NumpadDecimal => "N.",

            _ => "?",
        },
        key::Physical::Unidentified(_) => "?",
    }
}


fn cell_text<'a>(label: impl text::IntoFragment<'a>, scale: f32) -> Container<'a, Message, Theme> {
    container(text(label).size(28.0 * scale).center())
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
}

fn cell_size(scale: f32) -> f32 {
    72.0 * scale
}

fn key_cell(
    label: &'static str,
    action: Action,
    actively_listening: Option<Action>,
    scale: f32
) -> Element<'static, Message> {
    let is_listening = actively_listening
        .is_some_and(|listening| listening == action);

    let size = cell_size(scale);

    button(cell_text(label, scale))
        .width(size)
        .height(size)
        .style(move |theme: &Theme, status| {
            let mut style = button::primary(theme, status);
            match is_listening {
                true => {
                    let palette = theme.extended_palette();
                    style = style.with_background(Background::Color(palette.success.strong.color));
                    style.border = Border {
                        width: 2.0,
                        color: palette.success.base.color,
                        radius: 6.0.into(),
                    };
                }
                false => {
                    style.border = Border {
                        width: 1.0,
                        color: theme.extended_palette().background.strong.color,
                        radius: 6.0.into(),
                    };
                }
            }

            style
        })
        .on_press(Message::ListenToRebindFor(action))
        .into()
}

fn keypad_cell(ch: char, scale: f32) -> Element<'static, Message> {
    let size = cell_size(scale);

    container(cell_text(ch, scale))
        .width(size)
        .height(size)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.background.weak.color)),
                border: Border {
                    width: 1.0,
                    color: palette.background.strong.color,
                    radius: 6.0.into(),
                },
                text_color: Some(palette.background.strong.text),
                ..Default::default()
            }
        })
        .into()
}

macro_rules! send_event {
    ($self: expr, $event: expr) => {
        if $self._event_tx.send($event).is_err() {
            return Task::done(Message::Quit)
        }
    };
}

impl GuiApp {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Command(GuiCommand::Quit) => return iced::exit(),
            Message::Command(GuiCommand::Render(display)) => {
                self.render_state = GuiRenderState::Display(*display)
            }
            Message::Command(GuiCommand::Fault(fault)) => {
                self.render_state = GuiRenderState::Fault(fault)
            }

            Message::KeyPressed(key) => {
                if let key::Physical::Code(Code::Escape) = key {
                    match self.settings_overlay {
                        Some(SettingsOverlayState { remap_listening: Some(_), .. }) => {},
                        Some(SettingsOverlayState { remap_listening: None, .. }) => {
                            self.settings_overlay = None;
                            send_event!(self, GuiEvent::Resume)
                        }

                        None => {
                            if self._event_tx.send(GuiEvent::Pause).is_err() {
                                return Task::done(Message::Quit)
                            }

                            self.settings_overlay = Some(SettingsOverlayState {
                                remap_listening: None,
                                escape_key_released: false
                            })
                        },
                    }
                } else if let Some(action) = self.keymap.get(key) && self.settings_overlay.is_none() {
                    match action {
                        Action::GameInput(idx) => send_event!(self, GuiEvent::KeyPressed(idx)),
                        // ReloadRom happens on press
                        Action::ReloadRom => send_event!(self, GuiEvent::ReloadRom),
                        Action::TogglePause => { /* TogglePause happens on release not press */ },
                    }
                }
            }

            Message::KeyReleased(key) => {
                if let Some(ref mut overlay) = self.settings_overlay {
                    if let key::Physical::Code(Code::Escape) = key && !overlay.escape_key_released {
                        overlay.escape_key_released = true
                    } else if let Some(listen_index) = overlay.remap_listening.take() {
                        self.keymap.remap(key, listen_index)
                    }
                } else if let Some(action) = self.keymap.get(key) {
                    match action {
                        Action::GameInput(idx) => send_event!(self, GuiEvent::KeyReleased(idx)),
                        Action::ReloadRom => { /* ReloadRom happens on press not release */ }
                        // TogglePause happens on release
                        Action::TogglePause => send_event!(self, GuiEvent::TogglePause),
                    }
                }
            }

            Message::Quit => {
                // quitting either way, no need to handle the case where the send fails
                let _ = self._event_tx.send(GuiEvent::Quit);
                return iced::exit();
            }

            Message::ListenToRebindFor(input) => {
                if let Some(ref mut overlay) = self.settings_overlay {
                    overlay.remap_listening = Some(input)
                }
            }
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if let Some(ref overlay) = self.settings_overlay {
            let actively_listening = overlay.remap_listening;

            const { assert!(InputIndex::TOTAL_INDICES == 16) }

            const KEYBOARD_LAYOUT: [[InputIndex; 4]; 4] = {
                macro_rules! i {
                    ($lit: literal) => { InputIndex::from_usize($lit).unwrap() };
                }

                [
                    [i!(0x0), i!(0x1), i!(0x2), i!(0x3)],
                    [i!(0x4), i!(0x5), i!(0x6), i!(0x7)],
                    [i!(0x8), i!(0x9), i!(0xA), i!(0xB)],
                    [i!(0xC), i!(0xD), i!(0xE), i!(0xF)],
                ]
            };

            let make_key_cell = move |action: Action, scale: f32| {
                let label = self
                    .keymap
                    .key_for(action)
                    .map(key_label)
                    .unwrap_or("*");

                key_cell(label, action, actively_listening, scale)
            };

            let settings_page = responsive(move |size| {
                let base_width = 960.0;
                let base_height = 425.0;

                let scale_x = size.width / base_width;
                let scale_y = size.height / base_height;

                let scale = scale_x.min(scale_y).clamp(0.3, 3.0);


                let title_size = 28.0 * scale;
                let arrow_size = 40.0 * scale;
                let inner_spacing = 12.0 * scale;
                let outer_spacing = 32.0 * scale;
                let button_spacing = 8.0 * scale;
                let emulator_control_label = 14.0 * scale;
                let no_head_emulator_controls_spacing = 1.0 * scale;
                let section_spacing = 16.0 * scale;

                let no_head_emulator_controls = widget::column![
                    widget::column![
                        make_key_cell(Action::ReloadRom, scale),
                        text("reload-rom").size(emulator_control_label),
                    ]
                    .spacing(no_head_emulator_controls_spacing)
                    .align_x(Alignment::Center),
                    widget::column![
                        make_key_cell(Action::TogglePause, scale),
                        text("toggle-pause").size(emulator_control_label),
                    ]
                    .spacing(no_head_emulator_controls_spacing)
                    .align_x(Alignment::Center),
                ]
                    .spacing(inner_spacing)
                    .align_x(Alignment::Center);

                let emulator_controls = widget::column![
                    text("Emulator\nControls").size(title_size),
                    no_head_emulator_controls
                ]
                    .spacing(inner_spacing)
                    .align_x(Alignment::Center);

                let vertical_bar = container(widget::row![
                    widget::Space::new().height(Length::FillPortion(1)),
                    container(widget::Space::new().width(6.0 * scale))
                        .height(Length::FillPortion(2))
                        .center_y(Length::FillPortion(2))
                        .style(|theme: &Theme| {
                            container::Style {
                                background: Some(Background::Color(
                                    theme.extended_palette().background.strongest.color
                                )),
                                border: Border {
                                    // pill shape, rounded top + bottom
                                    radius: 999.0.into(),
                                    width: 0.0,
                                    color: Color::TRANSPARENT,
                                },
                                ..Default::default()
                            }
                        }),
                    widget::Space::new().height(Length::FillPortion(1)),
                ])
                    .center_y(Length::Fill);

                let keyboard_grid = widget::column(
                    KEYBOARD_LAYOUT.into_iter().map(|row_indices| {
                        widget::row(row_indices.into_iter().map(|input| {
                            make_key_cell(Action::GameInput(input), scale)
                        }))
                            .spacing(button_spacing)
                            .into()
                    }),
                )
                    .spacing(button_spacing)
                    .align_x(Alignment::Center);

                let keypad_grid = widget::column(
                    KEYBOARD_LAYOUT.into_iter().map(|row_chars| {
                        widget::row(row_chars.into_iter().map(|idx| {
                            keypad_cell(idx.as_char(), scale)
                        }))
                            .spacing(button_spacing)
                            .into()
                    }),
                )
                    .spacing(button_spacing)
                    .align_x(Alignment::Center);

                let keyboard_to_keypad = widget::row![
                    widget::column![
                        text("Keyboard").size(title_size),
                        keyboard_grid,
                    ]
                    .spacing(inner_spacing)
                    .align_x(Alignment::Center),

                    container(text("⇒").size(arrow_size))
                        .width(Length::Shrink)
                        .center_x(Length::Shrink)
                        .center_y(Length::Shrink),

                    widget::column![
                        text("Keypad").size(title_size),
                        keypad_grid,
                    ]
                    .spacing(inner_spacing)
                    .align_x(Alignment::Center),
                ]
                    .spacing(outer_spacing)
                    .align_y(Alignment::Center);

                let content = widget::row![
                    emulator_controls,
                    vertical_bar, widget::Space::new().width(section_spacing), keyboard_to_keypad
                ]
                    .spacing(section_spacing)
                    .align_y(Alignment::Center);

                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .into()
            });

            return settings_page.into();
        }

        match self.render_state {
            GuiRenderState::Display(ref display) => {
                canvas::Canvas::new(Chip8Canvas {
                    display,
                    fg: self.fg,
                    bg: self.bg,
                    border: self.border
                })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            GuiRenderState::Fault(fault) => {
                container(text(format!("{fault}")).size(20).style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.danger.strong.color),
                    }
                }))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center(Length::Fill)
                    .into()
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen().filter_map(|event| Some(match event {
            Event::Keyboard(keyboard::Event::KeyPressed { physical_key, .. }) => {
                Message::KeyPressed(physical_key)
            },
            Event::Keyboard(keyboard::Event::KeyReleased { physical_key, .. }) => {
                Message::KeyReleased(physical_key)
            }
            Event::Window(window::Event::CloseRequested) => Message::Quit,

            _ => return None,
        }))
    }
}

pub(super) fn run_gui(
    command_rx: flume::Receiver<GuiCommand>,
    event_tx: flume::Sender<GuiEvent>,
    fg: Rgb,
    bg: Rgb,
    thread: ScopedJoinHandle<'_, eyre::Result<()>>,
) -> eyre::Result<()> {
    fn rgb_to_iced(rgb: Rgb) -> Color {
        Color::from_rgb8(rgb.r, rgb.g, rgb.b)
    }

    fn relative_luminance(c: Color) -> f32 {
        fn channel(x: f32) -> f32 {
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        }

        let r = channel(c.r);
        let g = channel(c.g);
        let b = channel(c.b);

        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fn contrast_ratio(a: Color, b: Color) -> f32 {
        let la = relative_luminance(a);
        let lb = relative_luminance(b);
        let (bright, dark) = if la >= lb { (la, lb) } else { (lb, la) };
        (bright + 0.05) / (dark + 0.05)
    }

    fn pick_high_contrast_border(fg: Color, bg: Color) -> Color {
        // A small palette of strong candidates. We pick the one with the best
        // worst-case contrast against both fg and bg.
        const CANDIDATES: &[(f32, f32, f32)] = &[
            (0.0, 0.0, 0.0), // black
            (1.0, 1.0, 1.0), // white
            (1.0, 0.0, 0.0), // red
            (0.0, 1.0, 0.0), // green
            (0.0, 0.0, 1.0), // blue
            (1.0, 1.0, 0.0), // yellow
            (1.0, 0.0, 1.0), // magenta
            (0.0, 1.0, 1.0), // cyan
            (1.0, 0.5, 0.0), // orange
        ];

        CANDIDATES
            .iter()
            .copied()
            .map(|(r, g, b)| Color::from_rgb(r, g, b))
            .max_by(|a, b| {
                let a_score = contrast_ratio(*a, fg).min(contrast_ratio(*a, bg));
                let b_score = contrast_ratio(*b, fg).min(contrast_ratio(*b, bg));
                a_score
                    .partial_cmp(&b_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(Color::WHITE)
    }

    let fg = rgb_to_iced(fg);
    let bg = rgb_to_iced(bg);
    let border = pick_high_contrast_border(fg, bg);

    iced::application(
        move || {
            let command_rx = command_rx.clone();
            let res = event_tx.send(GuiEvent::RequestRedraw);

            let initial_task = match res {
                Ok(()) => Task::stream(command_rx.into_stream().map(Message::Command)),
                Err(_) => Task::done(Message::Command(GuiCommand::Quit)),
            };

            (
                GuiApp {
                    _event_tx: event_tx.clone(),
                    render_state: GuiRenderState::Display(Display::new()),
                    keymap: Keymap::default(),
                    settings_overlay: None,
                    fg,
                    bg,
                    border,
                },
                initial_task,
            )
        },
        GuiApp::update,
        GuiApp::view,
    )
        .title("CHIP-8")
        .subscription(GuiApp::subscription)
        .window_size(Size::new(960.0, 480.0))
        .run()?;

    thread.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}
