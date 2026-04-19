use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};
use chip_8_core::{Display, Fault, InputState};
use crate::earlier_deadline;

pub mod terminal;
pub mod gui;

pub enum PollEventAction {
    Resume(Instant),
    Pause,
    Reload,
    Quit,
}

pub enum PollEventActionRaw {
    Resume {
        resume_at: Instant,
        needs_redraw: bool
    },
    Reload,
    Pause,
    Quit,
}

pub trait Renderer {
    fn exit(self: Box<Self>) -> eyre::Result<()>;

    fn submit_render(&mut self, display: &Display) -> eyre::Result<()>;

    fn show_fault(&mut self, fault: Fault) -> eyre::Result<()>;

    fn make_beep(&mut self) -> eyre::Result<()>;

    fn fetch_input_state(&self) -> InputState;

    fn poll_events_raw(&mut self, deadline: Option<Instant>) -> eyre::Result<PollEventActionRaw>;
}

pub struct PacedRenderer<'a> {
    renderer: Box<dyn Renderer + 'a>,
    target_time_per_frame: Duration,
    last_draw: Option<Instant>,
    display_dirty: bool
}

impl<'a> PacedRenderer<'a> {
    pub fn new(renderer: impl Renderer + 'a, fps: u32) -> Self {
        Self {
            renderer: Box::new(renderer),
            target_time_per_frame: Duration::from_secs(1)
                .checked_div(fps)
                .unwrap_or(Duration::ZERO),
            last_draw: None,
            display_dirty: true
        }
    }

    pub fn needs_redraw(&self) -> bool {
        self.display_dirty
    }

    pub fn force_needs_redraw(&mut self) {
        self.display_dirty = true
    }

    pub fn request_render(&mut self, now: Instant, display: &Display) -> eyre::Result<()> {
        // just so if anything here panics; then the display will be dirty
        // until the frame is submitted; the renderer is dirty
        self.display_dirty = true;

        let time_to_draw = self.last_draw.is_none_or(|last_draw| {
            self.target_time_per_frame.is_zero()
                || now.saturating_duration_since(last_draw) >= self.target_time_per_frame
        });

        if !time_to_draw {
            return Ok(());
        }

        self.submit_render(display)?;
        // finaly submitted, no longer dirty
        self.display_dirty = false;
        self.last_draw = Some(now);
        Ok(())
    }

    pub fn exit(self) -> eyre::Result<()> {
        self.renderer.exit()
    }

    pub fn poll_events(&mut self, deadline: Option<Instant>) -> eyre::Result<PollEventAction> {
        let re_render = self
            .display_dirty
            .then(|| Instant::now().checked_add(self.target_time_per_frame))
            .flatten();

        let deadline = earlier_deadline(re_render, deadline);
        let events_raw = self.renderer.poll_events_raw(deadline)?;

        Ok(match events_raw {
            PollEventActionRaw::Resume {
                resume_at,
                needs_redraw
            } => {
                self.display_dirty |= needs_redraw;
                PollEventAction::Resume(resume_at)
            },
            PollEventActionRaw::Reload => PollEventAction::Reload,
            PollEventActionRaw::Pause => PollEventAction::Pause,
            PollEventActionRaw::Quit => PollEventAction::Quit,
        })
    }
}


impl<'a> Deref for PacedRenderer<'a> {
    type Target = dyn Renderer + 'a;

    fn deref(&self) -> &Self::Target {
        &*self.renderer
    }
}

impl<'a> DerefMut for PacedRenderer<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.renderer
    }
}
