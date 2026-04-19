use core::num::NonZero;
use crate::Fault;
use crate::niche_opt::Nibble;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InputIndex(Nibble);

impl InputIndex {
    pub const _1: Self = Self(Nibble::_0);
    pub const _2: Self = Self(Nibble::_1);
    pub const _3: Self = Self(Nibble::_2);
    pub const _C: Self = Self(Nibble::_3);



    pub const _4: Self = Self(Nibble::_4);
    pub const _5: Self = Self(Nibble::_5);
    pub const _6: Self = Self(Nibble::_6);
    pub const _D: Self = Self(Nibble::_7);



    pub const _7: Self = Self(Nibble::_8);
    pub const _8: Self = Self(Nibble::_9);
    pub const _9: Self = Self(Nibble::_10);
    pub const _E: Self = Self(Nibble::_11);



    pub const _A: Self = Self(Nibble::_12);
    pub const _0: Self = Self(Nibble::_13);
    pub const _B: Self = Self(Nibble::_14);
    pub const _F: Self = Self(Nibble::_15);


    pub const TOTAL_INDICES: usize = 16;

    pub const fn from_usize(index: usize) -> Option<Self> {
        if index > u8::MAX as usize {
            return None
        }

        match Nibble::new(index as u8) {
            Some(index) => Some(Self(index)),
            None => None
        }
    }

    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
    
    pub const fn as_char(self) -> char {
        match self {
            Self::_1 => '1',
            Self::_2 => '2',
            Self::_3 => '3',
            Self::_C => 'C',



            Self::_4 => '4',
            Self::_5 => '5',
            Self::_6 => '6',
            Self::_D => 'D',



            Self::_7 => '7',
            Self::_8 => '8',
            Self::_9 => '9',
            Self::_E => 'E',



            Self::_A => 'A',
            Self::_0 => '0',
            Self::_B => 'B',
            Self::_F => 'F',
        }
    }


    pub fn all_iter() -> impl DoubleEndedIterator<Item=Self> {
        (0..Self::TOTAL_INDICES as u8).map(|i| Self(unsafe { Nibble::new_unchecked(i) }))
    }
}

impl InputIndex {

    pub(crate) fn new(x: u8) -> Result<Self, Fault> {
        Nibble::new(x).map(Self).ok_or(Fault::InvalidInputIndex)
    }


    pub(crate) fn get(self) -> Nibble {
        self.0
    }

    const fn mask(self) -> u16 {
        1_u16 << self.0 as u8
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct InputState(u16);

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub const fn new() -> Self {
        Self(0)
    }

    pub(crate) const fn find_first_keypress(self) -> Option<InputIndex> {
        match NonZero::new(self.0) {
            Some(nz_input) => {
                Some(InputIndex(unsafe { Nibble::new_unchecked(nz_input.leading_zeros() as u8) }))
            },
            None => None
        }
    }

    pub const fn check(self, index: InputIndex) -> bool {
        (self.0 & index.mask()) != 0
    }

    pub const fn with_set(self, index: InputIndex) -> Self {
        Self(self.0 | index.mask())
    }

    pub const fn set(&mut self, index: InputIndex) {
        *self = self.copy().with_set(index);
    }

    pub const fn with_unset(self, index: InputIndex) -> Self {
        Self(self.0 & !index.mask())
    }

    pub const fn unset(&mut self, index: InputIndex) {
        *self = self.copy().with_unset(index);
    }

    pub const fn with_toggled(self, index: InputIndex) -> Self {
        Self(self.0 ^ index.mask())
    }

    pub const fn toggle(&mut self, index: InputIndex) {
        *self = self.copy().with_toggled(index);
    }

    /// this is clone but explicitly documents that this is a very cheap copy,
    /// and can run in const contexts
    pub const fn copy(&self) -> Self {
        Self(self.0)
    }

    /// checks if any input buttons are pressed
    pub const fn any(self) -> bool {
        self.0 != 0
    }
}

impl core::fmt::Display for InputState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("{:016b}", self.0))
    }
}