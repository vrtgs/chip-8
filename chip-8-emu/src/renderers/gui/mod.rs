use std::thread;
use std::time::Instant;
use flume::RecvTimeoutError;
use chip_8_core::{Display, Fault, InputIndex, InputState};
use crate::renderers::{PollEventActionRaw, Renderer};
use crate::rgb::Rgb;

mod ui;

#[derive(Clone)]
enum GuiCommand {
    Render(Box<Display>),
    Fault(Fault),
    Quit,
}

#[derive(Debug, Copy, Clone)]
enum GuiEvent {
    KeyPressed(InputIndex),
    KeyReleased(InputIndex),
    RequestRedraw,
    ReloadRom,
    TogglePause,
    Pause,
    Resume,
    Quit,
}

pub struct GuiRenderer {
    tx: flume::Sender<GuiCommand>,
    rx: flume::Receiver<GuiEvent>,
    input_state: InputState,
    is_paused: bool
}

impl GuiRenderer {
    pub fn run(
        fg: Rgb,
        bg: Rgb,
        driver: impl FnOnce(Self) -> eyre::Result<()> + Send
    ) -> eyre::Result<()> {
        const DEFAULT_CAPACITY: usize = 4096;

        let (command_tx, command_rx) = flume::bounded(DEFAULT_CAPACITY);
        let (event_tx, event_rx) = flume::bounded(DEFAULT_CAPACITY);

        let this = Self {
            tx: command_tx,
            rx: event_rx,
            input_state: InputState::new(),
            is_paused: false,
        };

        thread::scope(move |s| {
            let thread = thread::Builder::new()
                .name("chip-8-emu".to_owned())
                .spawn_scoped(s, move || driver(this))?;

            ui::run_gui(command_rx, event_tx, fg, bg, thread)
        })
    }

    fn exit_inner(&mut self) -> eyre::Result<()> {
        let _ = self.tx.send(GuiCommand::Quit);
        Ok(())
    }
}

impl Drop for GuiRenderer {
    fn drop(&mut self) {
        let _ = self.exit_inner();
    }
}


impl Renderer for GuiRenderer {
    fn exit(mut self: Box<Self>) -> eyre::Result<()> {
        self.exit_inner()
    }

    fn submit_render(&mut self, display: &Display) -> eyre::Result<()> {
        self.tx.send(GuiCommand::Render(Box::new(display.clone())))?;
        Ok(())
    }

    fn show_fault(&mut self, fault: Fault) -> eyre::Result<()> {
        self.tx.send(GuiCommand::Fault(fault))?;
        Ok(())
    }

    fn make_beep(&mut self) -> eyre::Result<()> {
        eprint!("\x07");
        Ok(())
    }

    fn fetch_input_state(&self) -> InputState {
        self.input_state.copy()
    }

    fn poll_events_raw(&mut self, deadline: Option<Instant>) -> eyre::Result<PollEventActionRaw> {
        let result = match deadline {
            Some(deadline) => self.rx.recv_deadline(deadline),
            None => self.rx.recv().map_err(|_err| RecvTimeoutError::Disconnected),
        };

        match result {
            Ok(event) => {
                let extra = self.rx.drain();

                // this iter **must** be ordered correctly
                // and thankfully this is correct; the first event we got
                // then we include whatever was still in the channel
                let iter = std::iter::once(event).chain(extra);

                let mut resume_at = (!self.is_paused).then(Instant::now);

                let mut needs_redraw = false;
                let mut quit = false;

                enum StateChange {
                    Pause,
                    Resume,
                    Reload,
                    None
                }

                impl StateChange {
                    fn reload(&mut self) {
                        *self = Self::Reload
                    }

                    fn pause(&mut self) {
                        if !matches!(self, Self::Reload) {
                            *self = Self::Pause
                        }
                    }


                    fn resume(&mut self, was_paused: bool, resume_at: &mut Option<Instant>) {
                        if !matches!(self, Self::Reload) {
                            *self = Self::Resume;

                            // if paused previously; then set the resume time to be now
                            if was_paused {
                                *resume_at = Some(Instant::now())
                            }
                        }
                    }

                    fn toggle_pause(&mut self, was_paused: bool, resume_at: &mut Option<Instant>) {
                        if let Self::None = self {
                            *self = match was_paused {
                                true => Self::Pause,
                                false => Self::Resume
                            }
                        }

                        match self {
                            Self::Resume => self.pause(),
                            Self::Pause => self.resume(was_paused, resume_at),
                            _ => {}
                        }
                    }
                }

                let mut change_state = StateChange::None;

                for event in iter {
                    match event {
                        GuiEvent::KeyPressed(idx) => self.input_state.set(idx),
                        GuiEvent::KeyReleased(idx) => self.input_state.unset(idx),
                        GuiEvent::RequestRedraw => needs_redraw = true,

                        GuiEvent::Pause => change_state.pause(),
                        GuiEvent::Resume => change_state.resume(self.is_paused, &mut resume_at),
                        GuiEvent::TogglePause => {
                            change_state.toggle_pause(self.is_paused, &mut resume_at)
                        },

                        GuiEvent::ReloadRom => change_state.reload(),
                        GuiEvent::Quit => quit = true,
                    }
                }

                if quit {
                    return Ok(PollEventActionRaw::Quit)
                }

                match change_state {
                    StateChange::Reload => {
                        self.is_paused = false;
                        return Ok(PollEventActionRaw::Reload)
                    }
                    StateChange::Pause => {
                        self.is_paused = true;
                        return Ok(PollEventActionRaw::Pause)
                    },
                    StateChange::Resume => self.is_paused = false,
                    StateChange::None => {}
                }

                // if we started off paused and there were no state changes;
                // trivially resume_at is none
                //
                // if we started off paused,
                // then got resumed resume_at will contain the time it got resumed
                //
                // if we started off paused then got resumed then paused; in the same time
                // there would be a changed_state = Paused; which would be handled previously
                match resume_at {
                    None => Ok(PollEventActionRaw::Pause),
                    Some(resume_at) => Ok(PollEventActionRaw::Resume {
                        resume_at,
                        needs_redraw
                    })
                }
            }
            Err(RecvTimeoutError::Timeout) => Ok(PollEventActionRaw::Resume {
                resume_at: Instant::now(),
                needs_redraw: false
            }),
            Err(RecvTimeoutError::Disconnected) => Ok(PollEventActionRaw::Quit),
        }
    }
}
