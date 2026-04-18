use core::fmt::Debug;
use bytemuck::Zeroable;

#[derive(Zeroable, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Nibble {
    _0, _1, _2, _3,
    _4, _5, _6, _7,
    _8, _9, _10, _11,
    _12, _13, _14, _15,
}

impl Debug for Nibble {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        <u8 as core::fmt::UpperHex>::fmt(&((*self) as u8), f)
    }
}

impl Nibble {
    pub(crate) const _A: Self = Self::_10;
    pub(crate) const _B: Self = Self::_11;
    pub(crate) const _C: Self = Self::_12;
    pub(crate) const _D: Self = Self::_13;
    pub(crate) const _E: Self = Self::_14;
    pub(crate) const _F: Self = Self::_15;


    #[inline(always)]
    pub(crate) const unsafe fn new_unchecked(x: u8) -> Self {
        unsafe {
            core::hint::assert_unchecked(x <= 0xF);
            core::mem::transmute::<u8, Self>(x)
        }
    }

    #[inline(always)]
    pub(crate) const fn new(x: u8) -> Option<Self> {
        if (x & const { !0xF }) != 0 {
            return None
        }

        Some(unsafe { Self::new_unchecked(x) })
    }

    #[inline]
    pub(crate) const fn select_nibble<const N: u32>(word: u16) -> Self {
        const { assert!(N < 4) }
        // in a word ABCD
        // A is index 0
        // B is index 1
        // C is index 2
        // D is index 3
        unsafe {
            let nibble_raw = word.unchecked_shr(const { 4 * (3 - N) }) as u8 & 0xF;
            core::mem::transmute::<u8, Self>(nibble_raw)
        }
    }
}
