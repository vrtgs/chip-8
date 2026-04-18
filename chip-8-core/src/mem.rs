use core::hint::cold_path;
use bytemuck::Zeroable;
use crate::Fault;
use crate::niche_opt::Nibble;

/// first 16 words from address range 0x000 to 0x00F is used for the stack
/// the 17th word found at address 0x010 is used for the input
#[derive(Zeroable)]
#[repr(transparent)]
pub(crate) struct Memory([u16; 2048]);

impl Memory {
    pub(crate) const ROM_START: Addr = Addr(0x200);
    pub(crate) const FONTSET_START_ADDRESS: Addr = Addr(0x50);

    #[inline(always)]
    pub(crate) const fn as_bytes(&self) -> &[u8; 4096] {
        unsafe { &*((&raw const self.0) as *const [u8; 4096]) }
    }

    #[inline(always)]
    pub(crate) const fn as_bytes_mut(&mut self) -> &mut [u8; 4096] {
        unsafe { &mut *((&raw mut self.0) as *mut [u8; 4096]) }
    }

    #[inline(always)]
    pub(crate) const fn as_words(&self) -> &[u16; 2048] {
        &self.0
    }

    #[inline(always)]
    pub(crate) const fn as_words_mut(&mut self) -> &mut [u16; 2048] {
        &mut self.0
    }

    #[inline(always)]
    pub(crate) fn load_word(&self, addr: Addr) -> Result<u16, Fault> {
        let bytes = self.as_bytes();
        let i = addr.get();

        if i.checked_add(1).is_none_or(|i| i >= bytes.len()) {
            return Err(Fault::Memory)
        }

        let word = unsafe {
            core::ptr::read_unaligned::<u16>(bytes.as_ptr().cast::<u16>().byte_add(i))
        };

        Ok(word.to_be())
    }

    #[inline(always)]
    pub(crate) fn load_offset(&self, base_addr: Addr, offset: usize) -> Result<u8, Fault> {
        let i = base_addr.get().checked_add(offset);
        match i.and_then(|i| self.as_bytes().get(i).copied()) {
            Some(byte) => Ok(byte),
            None => {
                cold_path();
                Err(Fault::Memory)
            }
        }
    }

    #[inline(always)]
    pub(crate) fn store_slice(&mut self, addr: Addr, slice: &[u8]) -> Result<(), Fault> {
        let memory = self.as_bytes_mut();
        for (i, &byte) in slice.iter().enumerate() {
            let loc = addr
                .get()
                .checked_add(i)
                .and_then(|loc_address| memory.get_mut(loc_address));

            let Some(location) = loc else {
                return Err(Fault::Memory)
            };

            *location = byte
        }

        Ok(())
    }

    #[inline(always)]
    pub(crate) fn load_slice(&self, addr: Addr, slice: &mut [u8]) -> Result<(), Fault> {
        let memory = self.as_bytes();
        for (i, byte_location) in slice.iter_mut().enumerate() {
            let loc = addr
                .get()
                .checked_add(i)
                .and_then(|loc_address| memory.get(loc_address));

            let Some(&loaded_byte) = loc else {
                return Err(Fault::Memory)
            };

            *byte_location = loaded_byte
        }

        Ok(())
    }

    #[inline(always)]
    pub(crate) fn store_bytes<const N: usize>(&mut self, addr: Addr, bytes: [u8; N]) -> Result<(), Fault> {
        self.store_slice(addr, &bytes)
    }
}



#[derive(Zeroable, Copy, Clone)]
#[repr(transparent)]
pub(crate) struct StackPointer(Nibble);

impl StackPointer {
    pub(crate) const fn inc(self) -> Option<Self> {
        let new = unsafe { (self.0 as u8).unchecked_add(1) };
        if new >= 16 {
            cold_path();
            return None
        }

        Some(unsafe { core::mem::transmute::<u8, Self>(new) })
    }

    pub(crate) const fn dec(self) -> Option<Self> {
        match (self.0 as u8).checked_sub(1) {
            Some(new) => Some(unsafe { core::mem::transmute::<u8, Self>(new) }),
            None => {
                cold_path();
                None
            }
        }
    }

    pub(crate) const fn load_pc(self, mem: &Memory) -> Addr {
        Addr(mem.as_words()[self.0 as usize])
    }

    pub(crate) const fn store_pc(self, pc: Addr, mem: &mut Memory) {
        mem.as_words_mut()[self.0 as usize] = pc.0
    }
}

#[derive(Zeroable, Copy, Clone, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct Addr(u16);

impl Addr {
    #[inline(always)]
    #[cfg(test)]
    pub(crate) const fn test_addr(address: u16) -> Self {
        Self(address)
    }

    #[inline(always)]
    pub(crate) const fn add8(self, offset: u8) -> Self {
        Self(self.0.wrapping_add(offset as u16))
    }

    #[inline(always)]
    pub(crate) const fn add(self, offset: u16) -> Self {
        Self(self.0.wrapping_add(offset))
    }
    
    #[inline(always)]
    pub(crate) const fn sub(self, offset: u16) -> Self {
        Self(self.0.wrapping_sub(offset))
    }

    #[inline(always)]
    pub(crate) const fn from_opcode(opcode: u16) -> Self {
        Self(opcode & 0xFFF)
    }

    #[inline(always)]
    pub(crate) const fn get(self) -> usize {
        const {
            if usize::BITS < 16 {
                panic!("the rust spec guarentees usize is at least 16 bits")
            }
        }

        self.0 as usize
    }
}
