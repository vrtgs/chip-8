use bytemuck::Zeroable;
use crate::Fault;
use crate::mem::{Addr, Memory};
use crate::niche_opt::Nibble;

#[derive(Zeroable, Clone)]
#[repr(transparent)]
pub struct Display([u64; 32]);

impl Default for Display {
    fn default() -> Self {
        Self::new()
    }
}

impl Display {
    pub const VIDEO_WIDTH: u8 = 64;
    pub const VIDEO_HEIGHT: u8 = 32;

    pub const fn new() -> Self {
        Self([0_u64; 32])
    }

    pub const fn clear(&mut self) {
        unsafe { core::ptr::write_bytes::<Self>(self, 0u8, 1) }
    }

    /// the board is laid out such that each u64 is a row
    /// and each row, the leftmost pixel is the MSB
    pub fn as_board(&self) -> &[u64; 32] {
        &self.0
    }

    /// the board is laid out such that each u64 is a row
    /// and each row, the leftmost pixel is the MSB
    pub fn as_board_mut(&mut self) -> &mut [u64; 32] {
        &mut self.0
    }

    pub fn get(&self, x: u8, y: u8) -> bool {
        let row = self.0[usize::from(y % Self::VIDEO_HEIGHT)];
        ((row.wrapping_shl(u32::from(x))) & const { 1_u64 << (64-1) }) != 0
    }

    pub(crate) fn draw(
        &mut self,
        x: u8,
        y: u8,
        height: Nibble,
        index: Addr,
        memory: &Memory,
    ) -> Result<bool, Fault> {
        let height = height as u8;
        let x = x % Self::VIDEO_WIDTH;
        let y = y % Self::VIDEO_HEIGHT;

        let mut collisions: u64 = 0;

        let screen = &mut self.0;

        let rows_to_end = Self::VIDEO_HEIGHT - y;

        let front_len = core::hint::select_unpredictable(
            rows_to_end < height,
            rows_to_end,
            height
        );
        let back_len = height.saturating_sub(front_len);

        let (before_y, rows_front) = screen.split_at_mut(y as usize);
        let rows_front = &mut rows_front[..front_len as usize];
        let rows_back = &mut before_y[..back_len as usize];

        let rows_iter = rows_front.iter_mut().chain(rows_back);
        for (row_i, row) in rows_iter.enumerate() {
            let sprite_byte = memory.load_offset(index, row_i)?;

            let sprite_row = ((sprite_byte as u64) << (const { 64 - 8 })).rotate_right(u32::from(x));
            let old_row = *row;
            collisions |= old_row & sprite_row;
            *row = old_row ^ sprite_row
        }

        Ok(collisions != 0)
    }
}
