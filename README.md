# chip-8

A small CHIP-8 project with two crates:

- **`chip-8-emu`**: a runnable CHIP-8 emulator application
- **`chip-8-core`**: a `#![no_std]`, sans-IO CHIP-8 emulator core library

This split keeps the core logic portable and embeddable, while the app crate provides a ready-to-run emulator.

## Crates

### `chip-8-emu`

A desktop emulator binary for running CHIP-8 ROMs.

#### Usage
```ansi
A CHIP-8 emulator

Usage: chip-8-emu [OPTIONS] <ROM>

Arguments:
  <ROM>  Path to the ROM file

Options:
      --platform <PLATFORM>  Emulation target [default: chip8] [possible values: chip8]
      --ui <UI>              display target [default: gui] [possible values: tui, gui]
      --hz <HZ>              CPU cycles per second [default: 700]
      --clock <CLOCK>        Delay ticks per second [default: 60]
      --fps <FPS>            target fps to update display [default: 24]
      --fg <FG>              Foreground color as hex, e.g. C8C8C8 [default: FFFFFF]
      --bg <BG>              Background color as hex, e.g. 050505 [default: 000000]
      --seed <SEED>          Seed for deterministic randomness
      --start-paused         Start emulation in a paused state
  -h, --help                 Print help
  -V, --version              Print version
```

---

### `chip-8-core`

A portable CHIP-8 emulator core designed for embedding into other projects.

It is:

* `#![no_std]`
* sans-IO
* suitable for native apps, terminals, game engines, WASM hosts, and embedded experiments
* separated from rendering, input, audio, and file loading policy

## Why two crates?

`chip-8-core` handles emulation state and instruction execution.

`chip-8-emu` is one possible frontend on top of that core.

That means you can:

* use the included emulator app as-is
* build your own frontend around the core
* keep deterministic tests and host-specific code separate

## `chip-8-core` API overview

### Random seeding

```rust
pub type Seed = [u32; 4];

pub trait Seeder {
    fn seed(self, seed: &mut Seed);
}
```

Anything implementing `rand_core::Rng` can be used as a `Seeder`.

### Display

```rust
#[derive(Zeroable, Clone)]
#[repr(transparent)]
pub struct Display(...);
```

`Display` exposes a 64×32 monochrome framebuffer:

* `Display::VIDEO_WIDTH = 64`
* `Display::VIDEO_HEIGHT = 32`

Available methods:

* `Display::new()`
* `Display::clear(&mut self)`
* `Display::as_board(&self) -> &[u64; 32]`
* `Display::get(&self, x: u8, y: u8) -> bool`

The board layout is row-major, with each `u64` representing one row and the leftmost pixel stored in the most significant bit.

### Faults

```rust
pub enum Fault {
    Memory,
    StackOverflow,
    StackUnderflow,
    InvalidInputIndex,
    InvalidInstruction,
}
```

These represent execution-time failures from invalid ROM behavior or illegal machine state.

### Cycle effects

```rust
pub enum CycleEffect {
    Executed,
    WaitForAnyKey,
    DisplayChanged,
    BeepStarted,
    DelayStarted,
}
```

This allows the host to react to meaningful emulator events without hard-wiring IO into the core.

### Emulator

```rust
pub struct Emulator(...);
```

Core entry points include:

* loading ROMs from bytes
* optionally reading ROMs via `std::io::Read` when `std` is enabled
* reading current display state
* reading and ticking timers
* executing one emulation cycle at a time

Key methods:

```rust
pub fn with_rom(&mut self, rom: &[u8], seeder: impl Seeder);
pub fn new_with_rom(rom: &[u8], seeder: impl Seeder) -> Self;
pub fn current_display(&self) -> &Display;
pub fn delay_timer(&self) -> u8;
pub fn sound_timer(&self) -> u8;
pub fn tick_timers(&mut self);
pub fn run_cycle(&mut self, input: InputState) -> Result<CycleEffect, Fault>;
```

Additional constructors and ROM readers are available behind `alloc` and `std` feature gates.

## Using `chip-8-core`

A host application is expected to provide:

* ROM bytes
* input state each cycle
* a timing loop for CPU execution
* timer ticks, usually at 60 Hz
* rendering from `Display`
* sound behavior when the sound timer is active or a beep-related effect occurs

A typical integration loop looks like this:

1. Load a ROM into `Emulator`
2. Call `run_cycle(input)` at your chosen CPU rate
3. Call `tick_timers()` at 60 Hz
4. Render `current_display()` whenever needed
5. React to `CycleEffect` values such as `DisplayChanged` or `BeepStarted`

### Sketch

```rust
use chip_8_core::Emulator;

// Pseudocode-ish: depends on how your InputState is constructed.
let rom = /* ROM bytes */;
let mut emu = Emulator::new_with_rom(rom, rand::thread_rng());

loop {
    let input = /* build input state from your platform */;
    let effect = emu.run_cycle(input)?;

    match effect {
        _ => {}
    }

    // Tick timers at 60 Hz on a separate schedule.
    // emu.tick_timers();

    // Render from emu.current_display()
}
```

## Features

### `chip-8-core`

Feature-gated functionality includes:

* `alloc`: boxed constructors
* `std`: `Read`-based ROM loading helpers

This keeps the library lightweight in constrained environments while still offering ergonomic APIs on full `std` targets.

## Project layout

```text
.
├── chip-8-core   # no_std sans-IO emulator core
└── chip-8-emu    # runnable emulator application
```

## Design goals

* small, focused CHIP-8 implementation
* portable core with no baked-in frontend assumptions
* deterministic behavior when seeded
* ergonomic binary for quickly running ROMs
* reusable library for custom hosts

