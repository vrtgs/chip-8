use std::io;
use std::io::{BufWriter, StdinLock, StdoutLock, Write};
use std::time::{Duration, Instant};
use crossterm::{cursor, execute, queue, terminal};
use crossterm::cursor::MoveTo;
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::style::{Color, Colors, Print, ResetColor, SetBackgroundColor, SetColors, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen};
use chip_8_core::{Display, Fault, InputIndex, InputState};
use crate::renderers::{PollEventActionRaw, Renderer};
use crate::rgb::Rgb;

#[derive(Debug, Copy, Clone)]
struct KeyLease(Option<Instant>);

impl KeyLease {
    // A key is considered released if we stop hearing about it for this long.
    // This is what makes the behavior portable across terminals that never send Release.
    const SYNTHETIC_RELEASE_AFTER: Duration = Duration::from_millis(100);

    fn up() -> Self {
        Self(None)
    }

    fn press(&mut self, now: Instant) {
        self.0 = Some(now + Self::SYNTHETIC_RELEASE_AFTER)
    }

    fn release(&mut self) {
        self.0 = None
    }

    fn expire(&mut self, now: Instant) -> bool {
        let expired = self.0.is_some_and(|expires_at| now >= expires_at);
        if expired {
            self.release()
        }
        expired
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct RenderMetadata {
    terminal_size: (u16, u16),
    origin_x: u16,
    origin_y: u16,
    out_w: u16,
    out_h: u16,
    draw_h_cells: u16,
    pixel: u16,
}

struct RenderData {
    metadata: RenderMetadata,
    display: Display,
}

pub struct TerminalRenderer {
    stdout: BufWriter<StdoutLock<'static>>,
    #[allow(dead_code)]
    stdin: StdinLock<'static>,
    fg: Color,
    bg: Color,
    terminal_size: (u16, u16),

    last_render: Option<RenderData>,

    input_state: InputState,
    key_leases: [KeyLease; InputIndex::TOTAL_INDICES],

    exited: bool,
}

impl TerminalRenderer {
    pub fn init(fg: Rgb, bg: Rgb) -> eyre::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = BufWriter::with_capacity(
            8 * 1024 * 1024,
            io::stdout().lock(),
        );
        let stdin = io::stdin().lock();
        execute!(stdout, EnterAlternateScreen, DisableLineWrap, cursor::Hide)?;
        stdout.flush()?;

        let terminal_size = match terminal::size()? {
            (0, 0) => (u16::from(Display::VIDEO_WIDTH), u16::from(Display::VIDEO_HEIGHT)),
            nz => nz
        };

        let make_color = |c: Rgb| Color::Rgb {
            r: c.r,
            g: c.g,
            b: c.b,
        };

        Ok(Self {
            stdout,
            stdin,
            fg: make_color(fg),
            bg: make_color(bg),
            terminal_size,
            last_render: None,
            input_state: InputState::new(),
            key_leases: [KeyLease::up(); InputIndex::TOTAL_INDICES],
            exited: false
        })
    }

    fn exit_inner(&mut self)  -> eyre::Result<()> {
        self.exited = true;
        execute!(
            self.stdout,
            cursor::Show,
            EnableLineWrap,
            LeaveAlternateScreen,
            Clear(ClearType::All),
            MoveTo(0, 0)
        )?;

        terminal::disable_raw_mode()?;

        self.stdout.flush()?;

        Ok(())
    }

    pub fn exit(mut self) -> eyre::Result<()> {
        self.exit_inner()
    }

    fn map_key(&self, key_code: KeyCode) -> Option<InputIndex> {
        Some(match key_code.as_char()? {
            '1' => InputIndex::_1,
            '2' => InputIndex::_2,
            '3' => InputIndex::_3,
            '4' => InputIndex::_C,



            'q' | 'Q' => InputIndex::_4,
            'w' | 'W' => InputIndex::_5,
            'e' | 'E' => InputIndex::_6,
            'r' | 'R' => InputIndex::_D,



            'a' | 'A' => InputIndex::_7,
            's' | 'S' => InputIndex::_8,
            'd' | 'D' => InputIndex::_9,
            'f' | 'F' => InputIndex::_E,



            'z' | 'Z' => InputIndex::_A,
            'x' | 'X' => InputIndex::_0,
            'c' | 'C' => InputIndex::_B,
            'v' | 'V' => InputIndex::_F,

            _ => return None
        })
    }

    fn press_key(&mut self, idx: InputIndex, now: Instant) {
        self.key_leases[idx.as_usize()].press(now);
        self.input_state.set(idx);
    }

    fn release_key(&mut self, idx: InputIndex) {
        self.key_leases[idx.as_usize()].release();
        self.input_state.unset(idx);
    }

    fn expire_keys(&mut self, now: Instant) {
        for (slot, lease) in self.key_leases.iter_mut().enumerate() {
            if lease.expire(now) {
                let idx = InputIndex::from_usize(slot).unwrap();
                self.input_state.unset(idx);
            }
        }
    }
}

impl Renderer for TerminalRenderer {
    fn exit(self: Box<Self>) -> eyre::Result<()> {
        let this: Self = *self;
        this.exit()
    }

    fn submit_render(&mut self, display: &Display) -> eyre::Result<()> {
        const UPPER_HALF_BLOCK: char = '▀';

        let (term_w, term_h) = self.terminal_size;

        let src_w = u16::from(Display::VIDEO_WIDTH);
        let src_h = u16::from(Display::VIDEO_HEIGHT);

        // One terminal cell = 1 column x 2 vertical logical pixels.
        let max_out_w = term_w;
        let max_out_h = term_h.saturating_mul(2);

        let pixel = (max_out_w / src_w).min(max_out_h / src_h).max(1);

        let out_w = src_w * pixel;
        let out_h = src_h * pixel;
        let draw_h_cells = out_h.div_ceil(2);

        let origin_x = term_w.saturating_sub(out_w) / 2;
        let origin_y = term_h.saturating_sub(draw_h_cells) / 2;

        let current_render_metadata = RenderMetadata {
            terminal_size: self.terminal_size,
            origin_x,
            origin_y,
            out_w,
            out_h,
            draw_h_cells,
            pixel,
        };

        let fg = self.fg;
        let bg = self.bg;

        let sample_bool = |frame: &Display, dx: u16, dy: u16| -> bool {
            let sx = dx / pixel;
            let sy = dy / pixel;
            frame.get(sx as u8, sy as u8)
        };

        let sample = |frame: &Display, dx: u16, dy: u16| -> Color {
            if sample_bool(frame, dx, dy) { fg } else { bg }
        };


        let different_metadata = self.last_render.as_mut().is_none_or(|last| {
            let is_different = last.metadata != current_render_metadata;
            if is_different {
                last.metadata = current_render_metadata;
            }

            is_different
        });


        if different_metadata {
            queue!(self.stdout, ResetColor, Clear(ClearType::All))?;
        }

        let last_correct_render = self
            .last_render
            .as_ref()
            .filter(|_| !different_metadata)
            .map(|last| &last.display);

        for cell_y in 0..draw_h_cells {
            let top_dy = cell_y * 2;
            let bottom_dy = top_dy + 1;

            let mut run_start: Option<u16> = None;

            let mut emit_run = |start: u16, end: u16| {
                queue!(self.stdout, MoveTo(origin_x + start, origin_y + cell_y))?;

                for draw_dx in start..end {
                    let top_color = sample(display, draw_dx, top_dy);

                    if bottom_dy < out_h {
                        let bottom_color = sample(display, draw_dx, bottom_dy);
                        queue!(
                            self.stdout,
                            SetForegroundColor(top_color),
                            SetBackgroundColor(bottom_color),
                            Print(UPPER_HALF_BLOCK),
                        )?;
                    } else {
                        queue!(
                            self.stdout,
                            SetForegroundColor(top_color),
                            SetBackgroundColor(Color::Reset),
                            Print(UPPER_HALF_BLOCK),
                        )?;
                    }
                }

                Ok::<_, io::Error>(())
            };

            for dx in 0..out_w {
                let changed = last_correct_render.is_none_or(|last_render| {
                    let top_now = sample_bool(display, dx, top_dy);
                    let top_prev = sample_bool(last_render, dx, top_dy);

                    let bottom_changed = (bottom_dy < out_h) && {
                        let bottom_now = sample_bool(display, dx, bottom_dy);
                        let bottom_prev = sample_bool(last_render, dx, bottom_dy);
                        bottom_now != bottom_prev
                    };

                    (top_now != top_prev) || bottom_changed
                });

                match (run_start, changed) {
                    (None, true) => run_start = Some(dx),
                    (Some(start), false) => {
                        emit_run(start, dx)?;
                        run_start = None;
                    }
                    _ => {}
                }
            }

            if let Some(start) = run_start {
                emit_run(start, out_w)?;
            }
        }

        queue!(self.stdout, ResetColor)?;
        self.stdout.flush()?;

        match self.last_render {
            Some(ref mut data) => {
                // metadata already changed when diffing
                data.display.clone_from(display)
            },
            None => self.last_render = Some(RenderData {
                metadata: current_render_metadata,
                display: display.clone()
            })
        };
        Ok(())
    }

    fn show_fault(&mut self, fault: Fault) -> eyre::Result<()> {
        let msg = fault.to_string();
        let lines: Vec<&str> = msg.lines().collect();

        let (term_w, term_h) = self.terminal_size;
        let block_h = lines.len() as u16;
        let origin_y = term_h.saturating_sub(block_h) / 2;

        execute!(
            self.stdout,
            SetColors(Colors::new(Color::White, Color::Black)),
            Clear(ClearType::All),
        )?;

        for (i, line) in lines.iter().enumerate() {
            let line_w = line.chars().count() as u16;
            let x = term_w.saturating_sub(line_w) / 2;
            let y = origin_y + i as u16;

            execute!(
                self.stdout,
                MoveTo(x, y),
                SetColors(Colors::new(Color::White, Color::DarkRed)),
                Print(*line),
                ResetColor
            )?;
        }

        self.stdout.flush()?;
        Ok(())
    }

    fn make_beep(&mut self) -> eyre::Result<()> {
        // terminal bell
        self.stdout.write_all(b"\x07")?;
        self.stdout.flush()?;
        Ok(())
    }

    fn fetch_input_state(&self) -> InputState {
        self.input_state.copy()
    }

    fn poll_events_raw(&mut self, deadline: Option<Instant>) -> eyre::Result<PollEventActionRaw> {
        let timeout = deadline.map_or(
            Duration::MAX,
            |deadline| deadline.saturating_duration_since(Instant::now())
        );

        let events_exist = crossterm::event::poll(timeout)?;

        let now = Instant::now();

        self.expire_keys(now);

        if !events_exist {
            return Ok(PollEventActionRaw::Resume {
                resume_at: now,
                needs_redraw: false
            })
        }

        let mut needs_redraw = false;
        loop {
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => {
                    if key.modifiers == KeyModifiers::CONTROL
                        && let Some('C' | 'c' | 'Q' | 'q') = key.code.as_char() {
                        return Ok(PollEventActionRaw::Quit)
                    }

                    if let Some(idx) = self.map_key(key.code) {
                        use crossterm::event::KeyEventKind as KEK;

                        match key.kind {
                            KEK::Press | KEK::Repeat => self.press_key(idx, now),
                            KEK::Release => self.release_key(idx),
                        }
                    }
                },
                crossterm::event::Event::Resize(width, height) => {
                    self.terminal_size = (width, height);
                    needs_redraw = true;
                },
                _ => {}
            }

            if !crossterm::event::poll(Duration::ZERO)? {
                break Ok(PollEventActionRaw::Resume {
                    resume_at: now,
                    needs_redraw
                })
            }
        }
    }
}

impl Drop for TerminalRenderer {
    fn drop(&mut self) {
        if !self.exited {
            let _ = self.exit_inner();
        }
    }
}
