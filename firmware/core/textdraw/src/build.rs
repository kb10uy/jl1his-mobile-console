use alloc::{vec, vec::Vec};
use core::ops::RangeInclusive;

use crate::{
    font::{glyph_len, row_len},
    table::{BITMAP_SIZE, BLOCK_BYTES, BLOCK_COUNT, RANGE_ENTRY_SIZE, RANK_TABLE_SIZE},
};

pub fn rank_table(code_points: impl IntoIterator<Item = u16>) -> Vec<u8> {
    let mut table = vec![0u8; RANK_TABLE_SIZE];
    for code_point in code_points {
        table[usize::from(code_point / 8)] |= 1 << (code_point % 8);
    }

    let mut rank: u16 = 0;
    for block in 0..BLOCK_COUNT {
        let ones: u32 = table[block * BLOCK_BYTES..][..BLOCK_BYTES]
            .iter()
            .map(|byte| byte.count_ones())
            .sum();
        table[BITMAP_SIZE + block * 2..][..2].copy_from_slice(&rank.to_le_bytes());
        rank = rank.wrapping_add(ones as u16);
    }
    table
}

pub fn range_table(ranges: &[RangeInclusive<u16>]) -> Vec<u8> {
    let mut table = Vec::with_capacity(ranges.len() * RANGE_ENTRY_SIZE);
    let mut base: u16 = 0;
    for range in ranges {
        assert!(range.start() <= range.end(), "empty range {range:?}");
        table.extend_from_slice(&range.start().to_le_bytes());
        table.extend_from_slice(&range.end().to_le_bytes());
        table.extend_from_slice(&base.to_le_bytes());
        base += range.end() - range.start() + 1;
    }
    table
}

pub fn pack_glyph(width: u32, height: u32, pixels: impl IntoIterator<Item = bool>) -> Vec<u8> {
    let mut data = vec![0; glyph_len(width, height)];
    let mut count = 0;
    for (index, lit) in pixels.into_iter().enumerate() {
        let x = index % width as usize;
        let y = index / width as usize;
        assert!(y < height as usize, "more than {width}x{height} pixels");
        if lit {
            data[y * row_len(width) + x / 8] |= 0x80 >> (x % 8);
        }
        count = index + 1;
    }
    assert_eq!(
        count,
        (width * height) as usize,
        "fewer than {width}x{height} pixels"
    );
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_glyph_pads_each_row_to_a_byte_boundary() {
        let pixels = "#....#....##\
                      .#########.#"
            .chars()
            .map(|c| c == '#');

        assert_eq!(pack_glyph(12, 2, pixels), [0x84, 0x30, 0x7F, 0xD0]);
    }

    #[test]
    #[should_panic(expected = "fewer than")]
    fn pack_glyph_rejects_missing_pixels() {
        pack_glyph(6, 12, [false; 71]);
    }

    #[test]
    #[should_panic(expected = "more than")]
    fn pack_glyph_rejects_extra_pixels() {
        pack_glyph(6, 12, [false; 73]);
    }
}
