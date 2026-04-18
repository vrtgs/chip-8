#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use core::hint::cold_path;
use bytemuck::Zeroable;
use crate::mem::{Addr, Memory, StackPointer};
use crate::niche_opt::Nibble;

#[derive(Debug, Copy, Clone)]
pub enum Fault {
    Memory,
    StackOverflow,
    StackUnderflow,
    InvalidInputIndex,
    InvalidInstruction,
}

impl core::fmt::Display for Fault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let str: &str = match *self {
            Fault::Memory => "memory fault",
            Fault::StackOverflow => "stack overflow",
            Fault::StackUnderflow => "stack underflow",
            Fault::InvalidInputIndex => "invalid input index requested",
            Fault::InvalidInstruction => "invalid instruction",
        };

        <str as core::fmt::Display>::fmt(str, f)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum CycleEffect {
    Executed,
    WaitForAnyKey,
    DisplayChanged,
    BeepStarted,
    DelayStarted,
}

mod niche_opt;
mod rng;
mod display;
mod input;
mod mem;


pub use rng::{Seeder, Seed};
pub use display::Display;
pub use input::{InputState, InputIndex};


const FONTSET_SIZE: usize = 80;

#[rustfmt::skip]
static FONTSET: [u8; FONTSET_SIZE] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

#[derive(Copy, Clone)]
#[repr(transparent)]
struct Register(Nibble);

impl Register {
    pub const V0: Self = Self(Nibble::_0);

    #[inline]
    pub fn from_opcode<const NIBBLE: u32>(opcode: u16) -> Self {
        Self(Nibble::select_nibble::<NIBBLE>(opcode))
    }
}

enum Instruction {
    /// 00E0: CLS
    ///
    ///
    /// Clears the display.
    Cls,
    /// 00EE: RET
    ///
    ///
    /// Return from a subroutine.
    Ret,
    /// 1nnn: JP addr
    ///
    ///
    /// Jump to location nnn.
    /// The interpreter sets the program counter to nnn.
    Jump(Addr),
    /// 2nnn - CALL addr
    ///
    ///
    /// Call subroutine at nnn.
    Call(Addr),
    /// 3xkk - SE Vx, byte
    ///
    ///
    /// Skip next instruction if Vx == kk.
    ///
    ///
    /// 4xkk - SNE Vx, byte
    ///
    ///
    /// Skip next instruction if Vx != kk.
    SkipEqImm {
        eq: bool,
        reg: Register,
        value: u8,
    },
    /// 5xy0 - SE Vx, Vy
    ///
    ///
    /// Skip next instruction if Vx == Vy.
    ///
    ///
    /// 9xy0 - SNE Vx, Vy
    ///
    ///
    /// Skip next instruction if Vx != Vy.
    SkipEqReg {
        eq: bool,
        reg1: Register,
        reg2: Register,
    },
    /// 6xkk - LD Vx, byte
    ///
    ///
    /// Set Vx = kk.
    LoadImm {
        reg: Register,
        value: u8
    },
    /// 7xkk - ADD Vx, byte
    ///
    ///
    /// Set Vx = Vx + kk.
    AddImm {
        reg: Register,
        value: u8
    },
    /// 8xy0 - LD Vx, Vy
    ///
    ///
    /// Set Vx = Vy.
    LoadReg {
        dst: Register,
        src: Register,
    },
    /// 8xy1 - OR Vx, Vy
    ///
    ///
    /// Set Vx = Vx OR Vy.
    OrReg {
        x: Register,
        y: Register,
    },
    /// 8xy2 - AND Vx, Vy
    ///
    ///
    /// Set Vx = Vx AND Vy.
    AndReg {
        x: Register,
        y: Register,
    },
    /// 8xy3 - XOR Vx, Vy
    ///
    ///
    /// Set Vx = Vx XOR Vy.
    XorReg {
        x: Register,
        y: Register,
    },
    /// 8xy4 - ADD Vx, Vy
    ///
    ///
    /// Set Vx = Vx + Vy, set VF = carry.
    AddReg {
        x: Register,
        y: Register,
    },
    /// 8xy5 - SUB Vx, Vy
    ///
    ///
    /// Set Vx = Vx - Vy, set VF = NOT borrow.
    ///
    ///
    /// If Vx > Vy, then VF is set to 1, otherwise 0.
    /// Then Vy is subtracted from Vx, and the results stored in Vx.
    SubReg {
        x: Register,
        y: Register,
    },
    /// 8xy6 - SHR Vx
    ///
    ///
    /// Set Vx = Vx SHR 1.
    ///
    ///
    /// If the least-significant bit of Vx is 1,
    /// then VF is set to 1, otherwise 0. Then Vx is divided by 2.
    /// basically shift right, by one, and put the bit that got removed inside VF
    Shr(Register),
    /// 8xy7 - SUBN Vx, Vy
    ///
    ///
    /// Set Vx = Vy - Vx, set VF = NOT borrow.
    ///
    /// If Vy > Vx, then VF is set to 1, otherwise 0. (this is the oposite comparison of `SubReg`)
    /// Then Vx is subtracted from Vy, and the results stored in Vx.
    SubNReg {
        x: Register,
        y: Register,
    },
    /// 8xyE - SHL Vx
    ///
    ///
    /// Set Vx = Vx SHL 1.
    ///
    /// If the most-significant bit of Vx is 1, then VF is set to 1,
    /// otherwise to 0. Then Vx is multiplied by 2.
    /// basically shift left, by one, and then put the bit that got removed inside VF
    Shl(Register),
    /// Annn - LD I, addr
    ///
    ///
    /// Set I = nnn.
    LoadAddrImm(Addr),
    /// Bnnn - JP V0, addr
    ///
    ///
    /// Jump to location nnn + V0.
    JumpIndirect(Addr),
    /// Cxkk - RND Vx, byte
    ///
    /// Set Vx = random byte AND kk.
    GetRand {
        reg: Register,
        mask: u8
    },
    /// Dxyn - DRW Vx, Vy, nibble
    ///
    /// Display n-byte sprite starting at memory location I at (Vx, Vy), set VF = collision.
    ///
    ///
    /// We iterate over the sprite, row by row and column by column. We know there are eight columns because a sprite is guaranteed to be eight pixels wide.
    ///
    ///
    /// If a sprite pixel is on then there may be a collision with what’s already being displayed, so we check if our screen pixel in the same location is set. If so we must set the VF register to express collision.
    /// Then we can just XOR the screen pixel
    Draw {
        x: Register,
        y: Register,
        height: Nibble
    },
    /// Ex9E - SKP Vx
    ///
    ///
    /// Skip next instruction if key with the value of Vx is pressed.
    ///
    ///
    /// ExA1 - SKNP Vx
    ///
    ///
    /// Skip next instruction if key with the value of Vx is not pressed.
    SkipIfInput {
        /// when true; skip if pressed
        ///
        /// when false; skip if not pressed
        pressed: bool,
        key_reg: Register
    },
    /// Fx07 - LD Vx, DT
    ///
    ///
    /// Set Vx = delay timer value.
    LoadDelayTimer(Register),
    /// Fx0A - LD Vx, K
    ///
    /// Wait for a key press, store the value of the key in Vx.
    ///
    /// The easiest way to “wait” is to decrement the PC by 2 whenever a keypad value is not detected. This has the effect of running the same instruction repeatedly.
    LoadWaitKey(Register),
    /// Fx15 - LD DT, Vx
    ///
    ///
    /// Set delay timer = Vx.
    SetDelayTimer(Register),
    /// Fx18 - LD ST, Vx
    ///
    ///
    /// Set sound timer = Vx.
    SetSoundTimer(Register),
    /// Fx1E - ADD I, Vx
    ///
    /// Set I = I + Vx.
    AddToIndex(Register),
    /// Fx29 - LD F, Vx
    ///
    /// Set I = location of sprite for digit Vx.
    ///
    /// We know the font characters are located at 0x50, and we know they’re five bytes each,
    /// so we can get the address of the first byte
    /// of any character by taking an offset from the start address.
    LoadFontSpriteToIndex(Register),
    /// Fx33 - LD B, Vx
    ///
    /// Store BCD representation of Vx in memory locations I, I+1, and I+2.
    ///
    /// The interpreter takes the decimal value of Vx, and places
    /// the hundreds digit in memory at location in I,
    /// the tens digit at location I+1, and
    /// the ones digit at location I+2.
    ///
    /// We can use the modulus operator to get the right-most digit of a number,
    /// and then do a division to remove that digit.
    /// A division by ten will either completely remove the digit (340 / 10 = 34),
    /// or result in a float which will be truncated (345 / 10 = 34.5 = 34).
    StoreBcd(Register),
    /// Fx55 - LD \[I], Vx
    ///
    /// 
    /// Store registers V0 through Vx in memory starting at location I.
    StoreRegistersToMem(Register),
    /// Fx65 - LD Vx, \[I]
    /// 
    /// 
    /// Read registers V0 through Vx from memory starting at location I.
    LoadRegistersFromMem(Register),
}

impl Instruction {
    #[inline]
    fn decode_opcode(opcode: u16) -> Result<Instruction, Fault> {
        Ok(match opcode {
            0x00E0 => Instruction::Cls,
            0x00EE => Instruction::Ret,
            0x1000..0x2000 => Instruction::Jump(Addr::from_opcode(opcode)),
            0x2000..0x3000 => Instruction::Call(Addr::from_opcode(opcode)),
            0x3000..0x5000 => Instruction::SkipEqImm {
                eq: opcode < 0x4000,
                reg: Register::from_opcode::<1>(opcode),
                value: opcode as u8,
            },

            0x5000..0x6000 | 0x9000..0xA000 => {
                if Nibble::select_nibble::<3>(opcode) != Nibble::_0 {
                    cold_path();
                    return Err(Fault::InvalidInstruction)
                }

                Instruction::SkipEqReg {
                    eq: opcode < 0x9000,
                    reg1: Register::from_opcode::<1>(opcode),
                    reg2: Register::from_opcode::<2>(opcode),
                }
            },

            0x6000..0x7000 => Instruction::LoadImm {
                reg: Register::from_opcode::<1>(opcode),
                value: opcode as u8
            },
            0x7000..0x8000 => Instruction::AddImm {
                reg: Register::from_opcode::<1>(opcode),
                value: opcode as u8
            },
            0x8000..0x9000 => {
                let x = Register::from_opcode::<1>(opcode);
                let y = Register::from_opcode::<2>(opcode);
                match Nibble::select_nibble::<3>(opcode) {
                    Nibble::_0 => Instruction::LoadReg {
                        dst: x,
                        src: y,
                    },
                    Nibble::_1 => Instruction::OrReg {
                        x,
                        y,
                    },
                    Nibble::_2 => Instruction::AndReg {
                        x,
                        y,
                    },
                    Nibble::_3 => Instruction::XorReg {
                        x,
                        y,
                    },
                    Nibble::_4 => Instruction::AddReg {
                        x,
                        y,
                    },
                    Nibble::_5 => Instruction::SubReg {
                        x,
                        y,
                    },
                    Nibble::_6 => Instruction::Shr(x),
                    Nibble::_7 => Instruction::SubNReg {
                        x,
                        y,
                    },
                    Nibble::_E => Instruction::Shl(x),
                    _ => return Err(Fault::InvalidInstruction)
                }
            },
            0xA000..0xB000 => Instruction::LoadAddrImm(Addr::from_opcode(opcode)),
            0xB000..0xC000 => Instruction::JumpIndirect(Addr::from_opcode(opcode)),
            0xC000..0xD000 => Instruction::GetRand {
                reg: Register::from_opcode::<1>(opcode),
                mask: opcode as u8,
            },
            0xD000..0xE000 => Instruction::Draw {
                x: Register::from_opcode::<1>(opcode),
                y: Register::from_opcode::<2>(opcode),
                height: Nibble::select_nibble::<3>(opcode),
            },

            0xE000..0xF000 => {
                let key_reg = Register::from_opcode::<1>(opcode);
                let magic = opcode as u8;
                let pressed = match magic {
                    0x9E => true,
                    0xA1 => false,
                    _ => return Err(Fault::InvalidInstruction)
                };

                Instruction::SkipIfInput {
                    pressed,
                    key_reg
                }
            },

            0xF000..=0xFFFF => {
                let reg = Register::from_opcode::<1>(opcode);
                let magic = opcode as u8;
                match magic {
                    0x07 => Instruction::LoadDelayTimer(reg),
                    0x0A => Instruction::LoadWaitKey(reg),
                    0x15 => Instruction::SetDelayTimer(reg),
                    0x18 => Instruction::SetSoundTimer(reg),
                    0x1E => Instruction::AddToIndex(reg),
                    0x29 => Instruction::LoadFontSpriteToIndex(reg),
                    0x33 => Instruction::StoreBcd(reg),
                    0x55 => Instruction::StoreRegistersToMem(reg),
                    0x65 => Instruction::LoadRegistersFromMem(reg),
                    _ => return Err(Fault::InvalidInstruction)
                }
            },

            _ => return Err(Fault::InvalidInstruction)
        })
    }
}


#[derive(Zeroable)]
struct InnerEmu {
    gp_registers: [u8; 16],
    index: Addr,
    pc: Addr,
    sp: StackPointer,
    delay_timer: u8,
    sound_timer: u8,
    mem: Memory,
    display: Display,
    rng: rng::SimpleRng
}



impl InnerEmu {
    fn load_reg(&self, reg: Register) -> u8 {
        self.gp_registers[reg.0 as usize]
    }
    
    fn store_reg(&mut self, reg: Register, value: u8) {
        self.gp_registers[reg.0 as usize] = value
    }

    fn set_flag_reg(&mut self, flag: u8) {
        let [.., ref mut vf] = self.gp_registers;
        *vf = flag;
    }

    fn set_flag(&mut self, flag: bool) {
        self.set_flag_reg(flag as u8)
    }

    fn rmw_reg_with_ret<T>(&mut self, reg: Register, op: impl FnOnce(u8) -> (u8, T)) -> T {
        let location = &mut self.gp_registers[reg.0 as usize];
        let value = *location;
        let (out, ret) = op(value);
        *location = out;
        ret
    }

    fn rmw_reg(&mut self, reg: Register, op: impl FnOnce(u8) -> u8) {
        self.rmw_reg_with_ret(reg, |x| (op(x), ()))
    }

    fn binop_bitwise(&mut self, reg1: Register, reg2: Register, op: impl FnOnce(u8, u8) -> u8) {
        let val2 = self.load_reg(reg2);
        let location = &mut self.gp_registers[reg1.0 as usize];
        let val1 = *location;
        *location = op(val1, val2)
    }

    fn binop_arith(&mut self, reg1: Register, reg2: Register, op: impl FnOnce(u8, u8) -> (u8, bool)) {
        let val2 = self.load_reg(reg2);
        let location = &mut self.gp_registers[reg1.0 as usize];
        let val1 = *location;
        let (out, flag) = op(val1, val2);
        *location = out;
        self.set_flag(flag)
    }

    #[inline(always)]
    fn skip_if(&mut self, cond: bool) {
        self.pc = core::hint::select_unpredictable(
            cond,
            self.pc.add(2),
            self.pc
        )
    }

    fn run_cycle(&mut self, input: InputState) -> Result<CycleEffect, Fault> {
        let opcode = self.mem.load_word(self.pc)?;
        self.pc = self.pc.add(2);

        match Instruction::decode_opcode(opcode)? {
            Instruction::Cls => {
                self.display.clear();
                return Ok(CycleEffect::DisplayChanged)
            },
            Instruction::Ret => {
                self.sp = self.sp.dec().ok_or(Fault::StackUnderflow)?;
                self.pc = self.sp.load_pc(&self.mem)
            },
            Instruction::Jump(addr) => self.pc = addr,
            Instruction::Call(new_pc) => {
                self.sp.store_pc(self.pc, &mut self.mem);
                self.sp = self.sp.inc().ok_or(Fault::StackOverflow)?;
                self.pc = new_pc
            }
            Instruction::SkipEqImm {
                eq,
                reg,
                value
            } => {
                let actual_value = self.load_reg(reg);
                self.skip_if((actual_value != value) ^ eq)
            }
            Instruction::SkipEqReg { 
                eq,
                reg1,
                reg2,
            } => {
                let reg1_val = self.load_reg(reg1);
                let reg2_val = self.load_reg(reg2);
                self.skip_if((reg1_val != reg2_val) ^ eq);
            }
            Instruction::LoadImm { reg, value } => self.store_reg(reg, value),
            Instruction::AddImm { reg, value: immediate } => {
                self.rmw_reg(reg, |value| value.wrapping_add(immediate))
            }
            Instruction::LoadReg { src, dst } => {
                let src_val = self.load_reg(src);
                self.store_reg(dst, src_val)
            }
            Instruction::OrReg { x, y } => {
                self.binop_bitwise(x, y, |x, y| x | y)
            }
            Instruction::AndReg { x, y } => {
                self.binop_bitwise(x, y, |x, y| x & y)
            }
            Instruction::XorReg { x, y } => {
                self.binop_bitwise(x, y, |x, y| x ^ y)
            }
            Instruction::AddReg { x, y } => {
                self.binop_arith(x, y, u8::overflowing_add)
            }
            Instruction::SubReg { x, y } => {
                self.binop_arith(x, y, |x, y| {
                    let (result, borrow) = u8::overflowing_sub(x, y);
                    (result, !borrow)
                })
            }
            Instruction::Shr(reg) => {
                let flag = self.rmw_reg_with_ret(reg, |x| (x >> 1, x & 1));
                self.set_flag_reg(flag);
            }
            Instruction::SubNReg { x, y } => {
                self.binop_arith(x, y, |x, y| {
                    let (result, borrow) = u8::overflowing_sub(y, x);
                    (result, !borrow)
                })
            }
            Instruction::Shl(reg) => {
                let flag = self.rmw_reg_with_ret(reg, |x| (x << 1, (x & 0x80) >> 7));
                self.set_flag_reg(flag);
            }
            Instruction::LoadAddrImm(imm) => self.index = imm,
            Instruction::JumpIndirect(addr) => {
                let offset = self.load_reg(Register::V0);
                self.pc = addr.add8(offset);
            }
            Instruction::GetRand { reg, mask } => {
                let random_val = self.rng.next_u8() & mask;
                self.store_reg(reg, random_val)
            }
            Instruction::Draw {
                x,
                y,
                height
            } => {
                let x = self.load_reg(x);
                let y = self.load_reg(y);
                let colided = self.display.draw(x, y, height, self.index, &self.mem)?;
                self.set_flag(colided);
                return Ok(CycleEffect::DisplayChanged)
            }
            Instruction::SkipIfInput { pressed, key_reg } => {
                let index = self.load_reg(key_reg);
                let skip = input.check(InputIndex::new(index)?) ^ !pressed;
                self.skip_if(skip)
            }
            Instruction::LoadDelayTimer(reg) => self.store_reg(reg, self.delay_timer),
            Instruction::LoadWaitKey(reg) => {
                match input.find_first_keypress() {
                    Some(index) => self.store_reg(reg, index.get() as u8),
                    None => {
                        self.pc = self.pc.sub(2);
                        return Ok(CycleEffect::WaitForAnyKey)
                    }
                }
            },
            Instruction::SetDelayTimer(delay) => {
                self.delay_timer = self.load_reg(delay);
                if self.delay_timer != 0 {
                    return Ok(CycleEffect::DelayStarted)
                }
            },
            Instruction::SetSoundTimer(delay) => {
                self.sound_timer = self.load_reg(delay);
                if self.sound_timer != 0 {
                    return Ok(CycleEffect::BeepStarted)
                }
            },
            Instruction::AddToIndex(x) => self.index = self.index.add8(self.load_reg(x)),
            Instruction::LoadFontSpriteToIndex(digit_reg) => {
                let digit = u16::from(self.load_reg(digit_reg)) * 5;
                self.index = Memory::FONTSET_START_ADDRESS.add(digit);
            }
            Instruction::StoreBcd(number) => {
                let value = self.load_reg(number);

                let ones = value % 10;
                let tens = (value / 10) % 10;
                let hundreds = (value / 100) % 100;

                self.mem.store_bytes(self.index, [hundreds, tens, ones])?
            },
            Instruction::StoreRegistersToMem(vx) => {
                self.mem.store_slice(self.index, &self.gp_registers[..=vx.0 as usize])?
            },
            Instruction::LoadRegistersFromMem(vx) => {
                self.mem.load_slice(self.index, &mut self.gp_registers[..=vx.0 as usize])?
            },
        }

        Ok(CycleEffect::Executed)
    }

    pub fn tick_timers(&mut self) {
        self.delay_timer = self.delay_timer.saturating_sub(1);
        self.sound_timer = self.sound_timer.saturating_sub(1);
    }

    fn load_new_rom<T, E>(
        &mut self,
        rom_loader: impl FnOnce(&mut [u8]) -> Result<T, E>,
        seeder: impl Seeder,
    ) -> Result<T, E> {
        let memory = self.mem.as_bytes_mut();
        let rom_buffer = &mut memory[Memory::ROM_START.get()..];

        let res = rom_loader(rom_buffer);

        if res.is_err() {
            return res
        }

        let fontset_start = Memory::FONTSET_START_ADDRESS.get();
        memory[fontset_start..(fontset_start + FONTSET_SIZE)].copy_from_slice(&FONTSET);

        self.pc = Memory::ROM_START;
        self.rng.reseed(seeder);
        self.display.clear();

        res
    }
}

#[repr(transparent)]
pub struct Emulator(InnerEmu);


impl Emulator {
    fn with_rom_pre_zeroed(&mut self, rom: &[u8], seeder: impl Seeder) {
        let Ok(()) = self.0.load_new_rom(
            |buffer| {
                if buffer.len() < rom.len() {
                    panic!("rom too big, rom must fit in {} bytes", buffer.len())
                }

                buffer[..rom.len()].copy_from_slice(rom);
                Ok::<(), core::convert::Infallible>(())
            },
            seeder
        );
    }

    pub fn with_rom(&mut self, rom: &[u8], seeder: impl Seeder) {
        bytemuck::write_zeroes::<InnerEmu>(&mut self.0);
        self.with_rom_pre_zeroed(rom, seeder)
    }

    #[cfg(feature = "std")]
    fn read_rom_pre_zeroed(
        &mut self,
        mut rom: impl std::io::Read,
        seeder: impl Seeder
    ) -> std::io::Result<()> {
        self.0.load_new_rom(
            move |mut buffer| {
                let mut read_rom = move |buffer: &mut [u8]| {
                    use std::io::ErrorKind::Interrupted;

                    loop {
                        match rom.read(buffer) {
                            Err(err) if err.kind() == Interrupted => continue,
                            res => break res
                        }
                    }
                };

                loop {
                    if buffer.is_empty() {
                        match read_rom(&mut [0])? {
                            0 => break, // the buffer is empty and the rom hit EOF; OK.
                            1.. => return Err(std::io::Error::new(
                                std::io::ErrorKind::FileTooLarge,
                                "emulator rom file too large"
                            ))
                        }
                    }

                    let Some(read) = core::num::NonZero::new(read_rom(buffer)?) else {
                        break;
                    };

                    buffer = &mut buffer[read.get()..];
                }

                Ok(())
            },
            seeder
        )
    }

    #[cfg(feature = "std")]
    pub fn read_rom(
        &mut self,
        rom: impl std::io::Read,
        seeder: impl Seeder
    ) -> std::io::Result<()> {
        bytemuck::write_zeroes::<InnerEmu>(&mut self.0);
        self.read_rom_pre_zeroed(rom, seeder)
    }

    #[cfg(feature = "alloc")]
    fn zeroed_and_boxed() -> Result<alloc::boxed::Box<Self>, ()> {
        use alloc::boxed::Box;

        let emu = bytemuck::try_zeroed_box::<InnerEmu>()?;
        // Safety: Emulator is repr(transparent) around InnerEmu
        Ok(unsafe { Box::from_raw(Box::into_raw(emu) as *mut Self) })
    }

    #[inline(always)]
    fn zeroed() -> Self {
        Self(bytemuck::zeroed::<InnerEmu>())
    }

    #[cfg(feature = "std")]
    pub fn read_new_rom_boxed(
        rom: impl std::io::Read,
        seeder: impl Seeder,
    ) -> std::io::Result<std::boxed::Box<Self>> {
        let mut this = Self::zeroed_and_boxed()
            .map_err(|()| std::io::ErrorKind::OutOfMemory)?;
        this.read_rom_pre_zeroed(rom, seeder).map(|()| this)
    }

    #[cfg(feature = "std")]
    pub fn read_new_rom(
        rom: impl std::io::Read,
        seeder: impl Seeder,
    ) -> std::io::Result<Self> {
        let mut this = Self::zeroed();
        this.read_rom_pre_zeroed(rom, seeder).map(|()| this)
    }

    #[cfg(feature = "alloc")]
    pub fn new_with_rom_boxed(rom: &[u8], seeder: impl Seeder) -> alloc::boxed::Box<Self> {
        let mut this = match Self::zeroed_and_boxed() {
            Ok(boxed) => boxed,
            Err(()) => alloc::alloc::handle_alloc_error(alloc::alloc::Layout::new::<Self>())
        };

        this.with_rom_pre_zeroed(rom, seeder);
        this
    }

    pub fn new_with_rom(rom: &[u8], seeder: impl Seeder) -> Self {
        let mut this = Self::zeroed();
        this.with_rom_pre_zeroed(rom, seeder);
        this
    }

    pub fn current_display(&self) -> &Display {
        &self.0.display
    }

    pub fn delay_timer(&self) -> u8 {
        self.0.delay_timer
    }

    pub fn sound_timer(&self) -> u8 {
        self.0.sound_timer
    }

    pub fn tick_timers(&mut self) {
        self.0.tick_timers()
    }

    pub fn run_cycle(&mut self, input: InputState) -> Result<CycleEffect, Fault> {
        self.0.run_cycle(input)
    }
}


#[cfg(test)]
mod tests {
    use crate::{Emulator, InputState, Seed, Seeder};
    use crate::mem::Addr;

    struct DummySeeder;

    impl Seeder for DummySeeder {
        fn seed(self, seed: &mut Seed) {
            *seed = std::array::from_fn(|i| i as u32 ^ 0x5F);
        }
    }

    #[test]
    fn test_opcodes() {
        let rom = include_bytes!("../test-roms/test_opcode.ch8").as_slice();
        let mut emu = Emulator::new_with_rom(rom, DummySeeder);

        static TEST_SUCCESS: [u64; 32] = [
            0b0000000000000000000000000000000000000000000000000000000000000000,
            0b0111010100111010100000011101110011101010000011100110111010100000,
            0b0011001000101011000000010101100010101100000011100100101011000000,
            0b0001010100101010100000010101000010101010000010100010101010100000,
            0b0111010100111010100000011101110011101010000011100100111010100000,
            0b0000000000000000000000000000000000000000000000000000000000000000,
            0b0101010100111010100000011101110011101010000011101110111010100000,
            0b0111001000101011000000011101010010101100000011101000101011000000,
            0b0001010100101010100000010101010010101010000010101110101010100000,
            0b0001010100111010100000011101110011101010000011101110111010100000,
            0b0000000000000000000000000000000000000000000000000000000000000000,
            0b0011010100111010100000011101100011101010000011101110111010100000,
            0b0010001000101011000000011100100010101100000011101100101011000000,
            0b0001010100101010100000010100100010101010000010101000101010100000,
            0b0010010100111010100000011101110011101010000011101110111010100000,
            0b0000000000000000000000000000000000000000000000000000000000000000,
            0b0111010100111010100000011101110011101010000011100110111010100000,
            0b0001001000101011000000011100010010101100000010000100101011000000,
            0b0001010100101010100000010101100010101010000011000010101010100000,
            0b0001010100111010100000011101110011101010000010000100111010100000,
            0b0000000000000000000000000000000000000000000000000000000000000000,
            0b0111010100111010100000011101110011101010000011101110111010100000,
            0b0111001000101011000000011100110010101100000010000110101011000000,
            0b0001010100101010100000010100010010101010000011000010101010100000,
            0b0111010100111010100000011101110011101010000010001110111010100000,
            0b0000000000000000000000000000000000000000000000000000000000000000,
            0b0010010100111010100000011101010011101010000011001010111010100000,
            0b0101001000101011000000011101110010101100000001000100101011000000,
            0b0111010100101010100000010100010010101010000001001010101010100000,
            0b0101010100111010100000011100010011101010000011101010111010100000,
            0b0000000000000000000000000000000000000000000000000000000000000000,
            0b0000000000000000000000000000000000000000000000000000000000000000,
        ];

        let mut cycles_executed: u32 = 0;
        loop {
            emu.run_cycle(InputState::new()).unwrap();
            cycles_executed += 1;
            if emu.0.pc == Addr::test_addr(0x3DC) {
                let test_success = *emu.current_display().as_board() == TEST_SUCCESS;
                assert!(test_success, "some opcode tests failed");
                break
            }

            if cycles_executed == 202 {
                panic!("executed too many times and didn't reach success loop")
            }
        }
    }
}