use embedded_graphics::geometry::Size;

use crate::table::Mapping;

pub(crate) const fn row_len(width: u32) -> usize {
    width.div_ceil(8) as usize
}

pub(crate) const fn glyph_len(width: u32, height: u32) -> usize {
    row_len(width) * height as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph<'a> {
    data: &'a [u8],
    size: Size,
}

impl<'a> Glyph<'a> {
    pub const fn size(&self) -> Size {
        self.size
    }

    pub const fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn pixels(&self) -> impl Iterator<Item = bool> + use<'a> {
        let width = self.size.width as usize;
        self.data
            .chunks_exact(row_len(self.size.width))
            .flat_map(move |row| (0..width).map(move |x| row[x / 8] & (0x80 >> (x % 8)) != 0))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Font<'a> {
    glyphs: &'a [u8],
    size: Size,
    mapping: Mapping<'a>,
}

impl<'a> Font<'a> {
    pub const fn new(glyphs: &'a [u8], width: u32, height: u32, mapping: Mapping<'a>) -> Self {
        assert!(width > 0 && height > 0);
        assert!(glyphs.len().is_multiple_of(glyph_len(width, height)));
        Self {
            glyphs,
            size: Size::new(width, height),
            mapping,
        }
    }

    pub const fn size(&self) -> Size {
        self.size
    }

    pub const fn glyph_count(&self) -> usize {
        self.glyphs.len() / glyph_len(self.size.width, self.size.height)
    }

    pub fn glyph(&self, c: char) -> Option<Glyph<'a>> {
        let len = glyph_len(self.size.width, self.size.height);
        let start = self.mapping.lookup(c)? * len;
        let data = self.glyphs.get(start..start + len)?;
        Some(Glyph {
            data,
            size: self.size,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FontSet<'a> {
    fonts: &'a [Font<'a>],
    line_height: u32,
    baseline: u32,
    replacement: char,
}

impl<'a> FontSet<'a> {
    pub const fn new(fonts: &'a [Font<'a>], baseline: u32, replacement: char) -> Self {
        assert!(!fonts.is_empty());
        let line_height = fonts[0].size.height;
        let mut index = 1;
        while index < fonts.len() {
            assert!(fonts[index].size.height == line_height);
            index += 1;
        }
        assert!(baseline < line_height);
        Self {
            fonts,
            line_height,
            baseline,
            replacement,
        }
    }

    pub const fn line_height(&self) -> u32 {
        self.line_height
    }

    pub const fn baseline(&self) -> u32 {
        self.baseline
    }

    pub fn glyph(&self, c: char) -> Option<Glyph<'a>> {
        self.fonts.iter().find_map(|font| font.glyph(c))
    }

    pub fn glyph_or_replacement(&self, c: char) -> Option<Glyph<'a>> {
        self.glyph(c).or_else(|| self.glyph(self.replacement))
    }
}

#[cfg(test)]
mod tests {
    use core::ops::RangeInclusive;

    use super::*;
    use crate::{
        RangeTable,
        build::{pack_glyph, range_table},
    };

    fn glyphs(width: u32, height: u32, patterns: &[&str]) -> Vec<u8> {
        patterns
            .iter()
            .flat_map(|pattern| pack_glyph(width, height, pattern.chars().map(|c| c == '#')))
            .collect()
    }

    fn single_range(range: RangeInclusive<u16>) -> Vec<u8> {
        range_table(core::slice::from_ref(&range))
    }

    #[test]
    fn glyph_pixels_round_trip_through_packing() {
        let pattern = "#..#.#..##.#\
                       .##.#.##..#.";
        let data = glyphs(12, 2, &[pattern]);
        let ranges = single_range(0x3042..=0x3042);
        let font = Font::new(&data, 12, 2, Mapping::Ranges(RangeTable::new(&ranges)));

        let glyph = font.glyph('あ').unwrap();
        let restored: String = glyph
            .pixels()
            .map(|lit| if lit { '#' } else { '.' })
            .collect();
        assert_eq!(restored, pattern);
        assert_eq!(glyph.size(), Size::new(12, 2));
        assert_eq!(font.glyph_count(), 1);
    }

    #[test]
    fn font_set_prefers_earlier_fonts_and_falls_back_to_replacement() {
        let narrow_data = glyphs(2, 1, &["#.", ".#"]);
        let narrow_ranges = single_range(0x41..=0x42);
        let wide_data = glyphs(4, 1, &["####", "#..#"]);
        let wide_ranges = single_range(0x42..=0x43);
        let fonts = [
            Font::new(
                &narrow_data,
                2,
                1,
                Mapping::Ranges(RangeTable::new(&narrow_ranges)),
            ),
            Font::new(
                &wide_data,
                4,
                1,
                Mapping::Ranges(RangeTable::new(&wide_ranges)),
            ),
        ];
        let set = FontSet::new(&fonts, 0, 'C');

        assert_eq!(set.glyph('A').unwrap().size().width, 2);
        assert_eq!(set.glyph('B').unwrap().size().width, 2);
        assert_eq!(set.glyph('C').unwrap().size().width, 4);
        assert_eq!(set.glyph('D'), None);
        assert_eq!(set.glyph_or_replacement('D'), set.glyph('C'));
    }
}
