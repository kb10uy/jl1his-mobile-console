pub(crate) const BLOCK_BITS: usize = 64;
pub(crate) const BLOCK_BYTES: usize = BLOCK_BITS / 8;
pub(crate) const BLOCK_COUNT: usize = 0x1_0000 / BLOCK_BITS;
pub(crate) const BITMAP_SIZE: usize = 0x1_0000 / 8;
pub const RANK_TABLE_SIZE: usize = BITMAP_SIZE + BLOCK_COUNT * 2;
pub const RANGE_ENTRY_SIZE: usize = 6;

fn bmp_code_point(c: char) -> Option<u16> {
    u16::try_from(u32::from(c)).ok()
}

#[derive(Debug, Clone, Copy)]
pub struct RankTable<'a> {
    data: &'a [u8; RANK_TABLE_SIZE],
}

impl<'a> RankTable<'a> {
    pub const fn new(data: &'a [u8; RANK_TABLE_SIZE]) -> Self {
        Self { data }
    }

    pub fn lookup(&self, c: char) -> Option<usize> {
        let code_point = usize::from(bmp_code_point(c)?);
        let block = code_point / BLOCK_BITS;
        let bit = code_point % BLOCK_BITS;

        let word = u64::from_le_bytes(*self.data[block * BLOCK_BYTES..].first_chunk()?);
        if word >> bit & 1 == 0 {
            return None;
        }

        let rank = u16::from_le_bytes(*self.data[BITMAP_SIZE + block * 2..].first_chunk()?);
        let preceding = word & ((1 << bit) - 1);
        Some(usize::from(rank) + preceding.count_ones() as usize)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RangeTable<'a> {
    data: &'a [u8],
}

impl<'a> RangeTable<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        assert!(data.len().is_multiple_of(RANGE_ENTRY_SIZE));
        Self { data }
    }

    pub fn lookup(&self, c: char) -> Option<usize> {
        let code_point = bmp_code_point(c)?;
        let (entries, _) = self.data.as_chunks::<RANGE_ENTRY_SIZE>();
        entries.iter().find_map(|&[f0, f1, l0, l1, b0, b1]| {
            let first = u16::from_le_bytes([f0, f1]);
            let last = u16::from_le_bytes([l0, l1]);
            let base = u16::from_le_bytes([b0, b1]);
            (first..=last)
                .contains(&code_point)
                .then(|| usize::from(base) + usize::from(code_point - first))
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Mapping<'a> {
    Ranges(RangeTable<'a>),
    Rank(RankTable<'a>),
}

impl Mapping<'_> {
    pub fn lookup(&self, c: char) -> Option<usize> {
        match self {
            Mapping::Ranges(table) => table.lookup(c),
            Mapping::Rank(table) => table.lookup(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{range_table, rank_table};

    #[test]
    fn rank_table_indexes_every_registered_code_point_in_order() {
        let code_points: Vec<u16> = (0..=u16::MAX)
            .filter(|cp| cp % 7 == 3 || (0x4E00..0x4F00).contains(cp) || *cp == u16::MAX)
            .collect();
        let data = rank_table(code_points.iter().copied());
        let table = RankTable::new(data.as_slice().try_into().unwrap());

        let mut expected = 0;
        for code_point in 0..=u16::MAX {
            let Some(c) = char::from_u32(u32::from(code_point)) else {
                continue;
            };
            if code_points.binary_search(&code_point).is_ok() {
                assert_eq!(table.lookup(c), Some(expected), "U+{code_point:04X}");
            } else {
                assert_eq!(table.lookup(c), None, "U+{code_point:04X}");
            }
            expected = code_points.partition_point(|&cp| cp <= code_point);
        }
    }

    #[test]
    fn rank_table_rejects_code_points_outside_the_bmp() {
        let data = rank_table([0x0041, 0xFF01]);
        let table = RankTable::new(data.as_slice().try_into().unwrap());

        assert_eq!(table.lookup('\u{FF01}'), Some(1));
        assert_eq!(table.lookup('\u{1FF01}'), None);
        assert_eq!(table.lookup('\u{10041}'), None);
    }

    #[test]
    fn range_table_assigns_indexes_sequentially_across_ranges() {
        let data = range_table(&[0x20..=0x7E, 0xA0..=0xFF, 0xFF61..=0xFF9F]);
        let table = RangeTable::new(&data);

        assert_eq!(table.lookup(' '), Some(0));
        assert_eq!(table.lookup('~'), Some(94));
        assert_eq!(table.lookup('\u{7F}'), None);
        assert_eq!(table.lookup('\u{A0}'), Some(95));
        assert_eq!(table.lookup('\u{FF}'), Some(190));
        assert_eq!(table.lookup('\u{FF61}'), Some(191));
        assert_eq!(table.lookup('\u{FF9F}'), Some(253));
        assert_eq!(table.lookup('\u{FFA0}'), None);
        assert_eq!(table.lookup('\u{10020}'), None);
    }
}
