#![cfg_attr(not(test), no_std)]

use textdraw::{Font, FontSet, Mapping, RangeTable, RankTable};

pub const BASELINE_12: u32 = 9;
pub const REPLACEMENT: char = '〓';

pub static HALF_12: Font<'static> = Font::new(
    include_bytes!(concat!(env!("OUT_DIR"), "/half12.glyphs")),
    6,
    12,
    Mapping::Ranges(RangeTable::new(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/half12.ranges"
    )))),
);

pub static FULL_12: Font<'static> = Font::new(
    include_bytes!(concat!(env!("OUT_DIR"), "/full12.glyphs")),
    12,
    12,
    Mapping::Rank(RankTable::new(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/full12.rank"
    )))),
);

static FONTS_12: [Font<'static>; 2] = [HALF_12, FULL_12];

pub static SHINONOME_12: FontSet<'static> = FontSet::new(&FONTS_12, BASELINE_12, REPLACEMENT);

#[cfg(test)]
mod tests {
    use embedded_graphics::{
        Drawable,
        geometry::{Point, Size},
        mock_display::MockDisplay,
        pixelcolor::BinaryColor,
        text::{Baseline, Text},
    };
    use textdraw::{Glyph, TextStyle};

    use super::*;

    fn art(glyph: Glyph<'_>) -> Vec<String> {
        let pixels: Vec<char> = glyph
            .pixels()
            .map(|lit| if lit { '@' } else { '.' })
            .collect();
        pixels
            .chunks(glyph.size().width as usize)
            .map(|row| row.iter().collect())
            .collect()
    }

    #[test]
    fn glyph_counts_match_the_source_fonts() {
        assert_eq!(HALF_12.glyph_count(), 95 + 96 + 63);
        assert_eq!(FULL_12.glyph_count(), 6879 + 7);
    }

    #[test]
    fn ascii_glyph_matches_the_source_bitmap() {
        assert_eq!(
            art(SHINONOME_12.glyph('A').unwrap()),
            [
                "......", "..@...", "..@...", ".@.@..", ".@.@..", ".@.@..", "@@@@@.", "@...@.",
                "@...@.", "@...@.", "......", "......",
            ]
        );
    }

    #[test]
    fn kanji_glyph_matches_the_source_bitmap() {
        assert_eq!(
            art(SHINONOME_12.glyph('日').unwrap()),
            [
                "............",
                "..@@@@@@@...",
                "..@.....@...",
                "..@.....@...",
                "..@.....@...",
                "..@.....@...",
                "..@@@@@@@...",
                "..@.....@...",
                "..@.....@...",
                "..@.....@...",
                "..@.....@...",
                "..@@@@@@@...",
            ]
        );
    }

    #[test]
    fn latin1_and_halfwidth_kana_use_the_half_width_font() {
        for c in ['A', '~', ' ', '\u{A0}', '°', '±', 'ÿ', '｡', 'ｱ', 'ﾟ'] {
            assert_eq!(
                SHINONOME_12.glyph(c).unwrap().size(),
                Size::new(6, 12),
                "{c:?}"
            );
        }
    }

    #[test]
    fn jis_x0208_characters_use_the_full_width_font() {
        for c in ['　', '、', 'あ', 'ア', 'Ａ', '漢', '熙', '￥', '〓'] {
            assert_eq!(
                SHINONOME_12.glyph(c).unwrap().size(),
                Size::new(12, 12),
                "{c:?}"
            );
        }
    }

    #[test]
    fn half_width_glyphs_differ_from_their_full_width_counterparts() {
        assert_ne!(
            SHINONOME_12.glyph('ｱ').unwrap().size(),
            SHINONOME_12.glyph('ア').unwrap().size()
        );
        assert!(HALF_12.glyph('ｱ').unwrap().pixels().any(|lit| lit));
        assert!(HALF_12.glyph('ア').is_none());
        assert!(FULL_12.glyph('ｱ').is_none());
    }

    #[test]
    fn divergent_symbol_mappings_share_one_glyph() {
        let pairs = [
            ('〜', '～'),
            ('‖', '∥'),
            ('−', '－'),
            ('―', '—'),
            ('¢', '￠'),
            ('£', '￡'),
            ('¬', '￢'),
        ];
        for (first, second) in pairs {
            let glyph = FULL_12.glyph(first).unwrap();
            assert_eq!(Some(glyph), FULL_12.glyph(second), "{first:?} / {second:?}");
            assert!(glyph.pixels().any(|lit| lit), "{first:?}");
        }
    }

    #[test]
    fn unsupported_characters_fall_back_to_the_replacement_glyph() {
        let replacement = SHINONOME_12.glyph(REPLACEMENT).unwrap();
        for c in ['😀', '𠮷', '\u{1FF01}', '한', '\t'] {
            assert_eq!(SHINONOME_12.glyph(c), None, "{c:?}");
            assert_eq!(
                SHINONOME_12.glyph_or_replacement(c),
                Some(replacement),
                "{c:?}"
            );
        }
    }

    #[test]
    fn every_bmp_lookup_stays_inside_the_glyph_data() {
        let found = (0..=0xFFFF)
            .filter_map(char::from_u32)
            .filter(|&c| FULL_12.glyph(c).is_some())
            .count();
        assert_eq!(found, FULL_12.glyph_count());
    }

    #[test]
    fn mixed_text_renders_with_half_and_full_width_advances() {
        let style = TextStyle::new(SHINONOME_12, BinaryColor::On);
        let mut display = MockDisplay::new();

        let next = Text::with_baseline("A日ｱ", Point::zero(), style, Baseline::Top)
            .draw(&mut display)
            .unwrap();

        assert_eq!(next, Point::new(6 + 12 + 6, 0));
        display.assert_pattern(&[
            "                        ",
            "  #     #######         ",
            "  #     #     #   ##### ",
            " # #    #     #       # ",
            " # #    #     #     # # ",
            " # #    #     #     ##  ",
            "#####   #######     ##  ",
            "#   #   #     #     #   ",
            "#   #   #     #    #    ",
            "#   #   #     #    #    ",
            "        #     #   #     ",
            "        #######         ",
        ]);
    }
}
