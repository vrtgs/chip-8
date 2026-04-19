use std::num::NonZero;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use clap::{Parser, ValueEnum};
use rand::SeedableRng;
use chip_8_core::{CycleEffect, Emulator, Fault, InputState};
use crate::renderers::{PacedRenderer, PollEventAction};
use crate::renderers::gui::GuiRenderer;
use crate::renderers::terminal::TerminalRenderer;
use crate::rgb::Rgb;

mod rgb;
mod renderers;

pub fn earlier_deadline(deadline1: Option<Instant>, deadline2: Option<Instant>) -> Option<Instant> {
    match (deadline1, deadline2) {
        (None, None) => None,
        (Some(one), None) | (None, Some(one)) => Some(one),
        (Some(one), Some(two)) => Some(core::cmp::min(one, two)),
    }
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum Platform {
    Chip8,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum UiType {
    TUI,
    GUI
}

const DEFAULT_CPU_HZ: NonZero<u32> = NonZero::new(700).unwrap();
const DEFAULT_CLOCK_HZ: NonZero<u32> = NonZero::new(60).unwrap();



#[derive(Debug, Parser)]
#[command(
    name = "chip8",
    version,
    about = "A CHIP-8 emulator",
    long_about = None
)]
pub struct Cli {
    /// Path to the ROM file
    #[arg(value_name = "ROM")]
    pub rom: PathBuf,

    /// Emulation target
    #[arg(long, value_enum, default_value_t = Platform::Chip8)]
    pub platform: Platform,

    /// display target
    #[arg(long, value_enum, default_value_t = UiType::GUI)]
    pub ui: UiType,

    /// CPU cycles per second
    #[arg(long, default_value_t = DEFAULT_CPU_HZ.get())]
    pub hz: u32,

    /// Delay ticks per second
    #[arg(long, default_value_t = DEFAULT_CLOCK_HZ.get())]
    pub clock: u32,

    /// target fps to update display
    #[arg(long, default_value_t = 24)]
    pub fps: u32,

    /// Foreground color as hex, e.g. C8C8C8
    #[arg(long, default_value = "FFFFFF")]
    pub fg: Rgb,

    /// Background color as hex, e.g. 050505
    #[arg(long, default_value = "000000")]
    pub bg: Rgb,

    /// Seed for deterministic randomness
    #[arg(long)]
    pub seed: Option<u64>,

    /// Start emulation in a paused state
    #[arg(long)]
    pub start_paused: bool
}

pub enum TimerState {
    Running {
        // None = WatingForInput
        next_cpu_cycle: Option<Instant>,
        // None = NoTimersActive
        next_clock_tick: Option<Instant>,
    },
    Paused {
        was_waiting_on_input: bool,
        clock_resume_offset: Option<Duration>,
    }
}

impl TimerState {
    pub fn pause(&mut self) {
        let Self::Running { next_cpu_cycle, next_clock_tick: next_timer_tick } = *self else {
            return;
        };

        let offset = match (next_cpu_cycle, next_timer_tick) {
            (_, None) => None,
            (None, Some(_)) => Some(Duration::ZERO),
            (Some(next_cycle), Some(next_tick)) => {
                Some(next_tick.saturating_duration_since(next_cycle))
            }
        };

        *self = Self::Paused {
            was_waiting_on_input: next_cpu_cycle.is_none(),
            clock_resume_offset: offset
        }
    }

    pub fn resume(&mut self, now: Instant, input: InputState) {
        match *self {
            TimerState::Running { ref mut next_cpu_cycle, next_clock_tick: _ } => {
                // if waiting on input; resume since input is available
                if next_cpu_cycle.is_none() && input.any() {
                    *next_cpu_cycle = Some(now);
                }
            },
            TimerState::Paused {
                was_waiting_on_input,
                clock_resume_offset
            } => {
                *self = TimerState::Running {
                    next_cpu_cycle: (!was_waiting_on_input).then_some(now),
                    next_clock_tick: clock_resume_offset.map(|offset| now + offset),
                }
            }
        }
    }
}

#[derive(Copy, Clone)]
struct Timings {
    delta_time: Duration,
    resync_threshold: Duration
}

impl Timings {
    pub fn new(hz: u32, default: NonZero<u32>) -> Self {
        let hz = NonZero::new(hz).unwrap_or(default);
        let delta_time = Duration::from_secs(1) / hz.get();

        let floor = Duration::from_millis(2);
        let ceiling = Duration::from_millis(500);
        let scaled = delta_time.saturating_mul(12);
        let resync_threshold = scaled.clamp(floor, ceiling);

        Self {
            delta_time,
            resync_threshold
        }
    }
}

pub struct EmuTimers {
    state: TimerState,

    cpu: Timings,
    clock: Timings,
}

#[must_use]
enum TimerIterationResult<T> {
    Continue,
    HaltTimer,
    Break(T)
}

impl EmuTimers {
    pub fn new(start_paused: bool, cpu_hz: u32, clock_hz: u32) -> Self {
        Self {
            state: match start_paused {
                true => TimerState::Paused {
                    was_waiting_on_input: false,
                    clock_resume_offset: None,
                },
                false => TimerState::Running {
                    next_cpu_cycle: Some(Instant::now()),
                    next_clock_tick: None,
                }
            },
            cpu: Timings::new(cpu_hz, DEFAULT_CPU_HZ),
            clock: Timings::new(clock_hz, DEFAULT_CLOCK_HZ),
        }
    }

    fn run_timer_until<T>(
        now: Instant,
        timer: &mut Option<Instant>,
        timings: Timings,
        mut cycle: impl FnMut() -> TimerIterationResult<T>,
    ) -> Option<T> {
        let next_cycle: &mut Instant = timer.as_mut()?;

        if now.saturating_duration_since(*next_cycle) > timings.resync_threshold {
            *next_cycle = now
        }

        while now >= *next_cycle {
            match cycle() {
                TimerIterationResult::Continue => *next_cycle += timings.delta_time,
                TimerIterationResult::HaltTimer => {
                    *timer = None;
                    break;
                },
                TimerIterationResult::Break(ret) => return Some(ret)
            }
        }

        None
    }

    fn run_until<S, CPU1, CLOCK1>(
        &mut self,
        now: Instant,
        state: &mut S,
        mut clock: impl FnMut(&mut S) -> TimerIterationResult<CLOCK1>,
        mut cpu: impl FnMut(&mut S) -> TimerIterationResult<CPU1>,
    ) -> (Option<CLOCK1>, Option<CPU1>) {
        let TimerState::Running { next_cpu_cycle, next_clock_tick } = &mut self.state else {
            return (None, None)
        };

        let state1 = &mut *state;
        let clock = move || clock(state1);
        let clock_ret = Self::run_timer_until(now, next_clock_tick, self.clock, clock);

        let cpu = move || cpu(state);
        let cpu_ret = Self::run_timer_until(now, next_cpu_cycle, self.cpu, cpu);

        (clock_ret, cpu_ret)
    }

    fn ensure_clock_start(&mut self, now: Instant) {
        match self.state {
            TimerState::Running {
                next_clock_tick: ref mut clock @ None,
                ..
            } => {
                *clock = Some(now + self.clock.delta_time);
            }

            TimerState::Paused {
                clock_resume_offset: ref mut clock_offset @ None,
                ..
            } => {
                *clock_offset = Some(self.clock.delta_time)
            }
            _ => {}
        }
    }

    fn pause(&mut self) {
        self.state.pause()
    }

    fn resume(&mut self, now: Instant, input: InputState) {
        self.state.resume(now, input)
    }

    fn reschedule_deadline(&self) -> Option<Instant> {
        match self.state {
            TimerState::Running { next_cpu_cycle, next_clock_tick } => {
                earlier_deadline(next_cpu_cycle, next_clock_tick)
            },
            TimerState::Paused { .. } => None
        }
    }
}

fn main() -> eyre::Result<()> {
    let cli = Cli::parse();
    let rng = match cli.seed {
        None => &mut rand::rng() as &mut dyn rand::Rng,
        Some(x) => &mut rand::rngs::ChaCha20Rng::seed_from_u64(x),
    };

    let file = std::fs::File::open(cli.rom.as_path())?;
    let mut emu = Emulator::read_new_rom_boxed(file, rng)?;



    let mut timers = EmuTimers::new(
        /* start_paused */ false,
        cli.hz,
        cli.clock,
    );

    let mut run_main = move |mut renderer: PacedRenderer| -> eyre::Result<()> {
        let result = 'emu_loop: loop {
            let now = Instant::now();
            let input = renderer.fetch_input_state();

            let mut should_render = false;
            let mut should_init_timers = false;
            let mut should_beep_timer = false;
            let mut should_beep_instruction = false;

            let (None, cpu_fault) = timers.run_until(
                now,
                &mut *emu,
                |emu| -> TimerIterationResult<core::convert::Infallible> {
                    should_beep_timer |= emu.sound_timer() > 0;
                    emu.tick_timers();
                    if emu.sound_timer() == 0 && emu.delay_timer() == 0 {
                        return TimerIterationResult::HaltTimer
                    }

                    TimerIterationResult::Continue
                },
                |emu| -> TimerIterationResult<Fault> {
                    match emu.run_cycle(input.copy()) {
                        Ok(CycleEffect::DisplayChanged) => should_render = true,
                        Ok(CycleEffect::BeepStarted) => {
                            should_init_timers = true;
                            should_beep_instruction = true;
                        },
                        Ok(CycleEffect::DelayStarted) => should_init_timers = true,
                        Ok(CycleEffect::Executed) => {}
                        Ok(CycleEffect::WaitForAnyKey) => return TimerIterationResult::HaltTimer,
                        Err(fault) => return TimerIterationResult::Break(fault)
                    }

                    TimerIterationResult::Continue
                }
            );


            if let Some(fault) = cpu_fault {
                renderer.show_fault(fault)?;
                break 'emu_loop Err(fault)
            }


            if should_beep_timer || should_beep_instruction {
                renderer.make_beep()?
            }

            if should_render || renderer.needs_redraw() {
                renderer.request_render(now, emu.current_display())?;
            }

            if should_init_timers {
                timers.ensure_clock_start(now);
            }


            let resumed_now = match renderer.poll_events(timers.reschedule_deadline())? {
                PollEventAction::Resume(now) => now,
                PollEventAction::Pause => {
                    timers.pause();
                    continue
                },
                PollEventAction::Reload => {
                    emu.read_rom(std::fs::File::open(cli.rom.as_path())?, rand::rng())?;
                    renderer.force_needs_redraw();
                    timers = EmuTimers::new(
                        /* start_paused */ false,
                        cli.hz,
                        cli.clock,
                    );

                    continue
                },
                PollEventAction::Quit => break Ok(()),
            };

            timers.resume(resumed_now, renderer.fetch_input_state());
        };

        if result.is_err() {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let now = match renderer.poll_events(Some(deadline))? {
                    PollEventAction::Resume(now) => now,
                    PollEventAction::Reload | PollEventAction::Pause =>  Instant::now(),
                    PollEventAction::Quit => break
                };

                if deadline.saturating_duration_since(now) <= Duration::from_millis(50) {
                    break
                }
            }
        }

        renderer.exit()?;

        match result {
            Ok(()) => Ok(()),
            Err(fault) => Err(eyre::eyre!("emulator fault: {}", fault))
        }
    };


    let Cli { fg, bg, fps, .. } = cli;
    match cli.ui {
        UiType::TUI => run_main(PacedRenderer::new(TerminalRenderer::init(fg, bg)?, fps)),
        UiType::GUI => GuiRenderer::run(
            fg,
            bg,
            |renderer| run_main(PacedRenderer::new(renderer, fps))
        ),
    }
}
