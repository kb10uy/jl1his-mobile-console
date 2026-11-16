use crate::font::FontSet;

const NO_LINE_START: &str = "、。，．・：；？！ー…‥）〕］｝〉》」』】’”｡｣､･,.:;?!)]}";
const NO_LINE_END: &str = "（〔［｛〈《「『【‘“｢([{";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineBreak {
    pub end: usize,
    pub next: usize,
}

impl<'a> FontSet<'a> {
    pub fn advance(&self, c: char) -> u32 {
        self.glyph_or_replacement(c)
            .map_or(0, |glyph| glyph.size().width)
    }

    pub fn break_line(&self, text: &str, max_width: u32) -> LineBreak {
        let mut width = 0;
        for (index, c) in text.char_indices() {
            if c == '\n' {
                return LineBreak {
                    end: index,
                    next: index + 1,
                };
            }
            width += self.advance(c);
            if width > max_width && index > 0 {
                return keep_words_whole(text, apply_kinsoku(text, index));
            }
        }
        LineBreak {
            end: text.len(),
            next: text.len(),
        }
    }

    pub fn wrap<'t>(&self, text: &'t str, max_width: u32) -> WrappedLines<'a, 't> {
        WrappedLines {
            fonts: *self,
            text,
            max_width,
        }
    }
}

fn apply_kinsoku(text: &str, mut cut: usize) -> usize {
    while let Some((previous, last)) = text[..cut].char_indices().next_back() {
        let starts_badly = text[cut..]
            .chars()
            .next()
            .is_some_and(|c| NO_LINE_START.contains(c));
        let ends_badly = NO_LINE_END.contains(last);
        if previous == 0 || !(starts_badly || ends_badly) {
            break;
        }
        cut = previous;
    }
    cut
}

fn keep_words_whole(text: &str, cut: usize) -> LineBreak {
    let (line, rest) = text.split_at(cut);
    let splits_word = line.chars().next_back().is_some_and(is_word_char)
        && rest.chars().next().is_some_and(is_word_char);
    if splits_word && let Some(space) = line.rfind(' ').filter(|&space| space > 0) {
        return LineBreak {
            end: line[..space].trim_end_matches(' ').len(),
            next: space + 1,
        };
    }
    LineBreak {
        end: line.trim_end_matches(' ').len(),
        next: if rest.starts_with(' ') { cut + 1 } else { cut },
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_graphic()
}

#[derive(Debug, Clone)]
pub struct WrappedLines<'a, 't> {
    fonts: FontSet<'a>,
    text: &'t str,
    max_width: u32,
}

impl<'t> Iterator for WrappedLines<'_, 't> {
    type Item = &'t str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.text.is_empty() {
            return None;
        }
        let LineBreak { end, next } = self.fonts.break_line(self.text, self.max_width);
        let line = &self.text[..end];
        self.text = &self.text[next..];
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Font, Mapping, RangeTable,
        build::{pack_glyph, range_table},
    };

    struct Fixture {
        narrow_glyphs: Vec<u8>,
        narrow_ranges: Vec<u8>,
        wide_glyphs: Vec<u8>,
        wide_ranges: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            let blank = |width, count| -> Vec<u8> {
                (0..count)
                    .flat_map(|_| pack_glyph(width, 1, (0..width).map(|_| false)))
                    .collect()
            };
            Self {
                narrow_glyphs: blank(2, 95),
                narrow_ranges: range_table(core::slice::from_ref(&(0x20..=0x7E))),
                wide_glyphs: blank(4, 256),
                wide_ranges: range_table(core::slice::from_ref(&(0x3000..=0x30FF))),
            }
        }

        fn fonts(&self) -> [Font<'_>; 2] {
            [
                Font::new(
                    &self.narrow_glyphs,
                    2,
                    1,
                    Mapping::Ranges(RangeTable::new(&self.narrow_ranges)),
                ),
                Font::new(
                    &self.wide_glyphs,
                    4,
                    1,
                    Mapping::Ranges(RangeTable::new(&self.wide_ranges)),
                ),
            ]
        }
    }

    fn wrap(text: &str, max_width: u32) -> Vec<String> {
        let fixture = Fixture::new();
        let fonts = fixture.fonts();
        FontSet::new(&fonts, 0, '〓')
            .wrap(text, max_width)
            .map(String::from)
            .collect()
    }

    #[test]
    fn lines_break_when_the_next_glyph_does_not_fit() {
        assert_eq!(wrap("あいうえお", 12), ["あいう", "えお"]);
        assert_eq!(wrap("abあc", 6), ["ab", "あc"]);
        assert_eq!(wrap("abcdef", 6), ["abc", "def"]);
    }

    #[test]
    fn newlines_end_lines_and_blank_lines_are_kept() {
        assert_eq!(wrap("あい\n\nう\n", 12), ["あい", "", "う"]);
    }

    #[test]
    fn closing_punctuation_pulls_the_previous_glyph_to_the_next_line() {
        assert_eq!(wrap("あいう。え", 12), ["あい", "う。え"]);
        assert_eq!(wrap("あいう」。", 12), ["あい", "う」。"]);
    }

    #[test]
    fn opening_brackets_move_to_the_next_line() {
        assert_eq!(wrap("あい「うえ", 12), ["あい", "「うえ"]);
    }

    #[test]
    fn every_line_keeps_at_least_one_glyph() {
        assert_eq!(wrap("。。。", 4), ["。", "。", "。"]);
        assert_eq!(wrap("ああ", 3), ["あ", "あ"]);
    }

    #[test]
    fn latin_words_wrap_at_spaces() {
        assert_eq!(wrap("aaa bbbb", 10), ["aaa", "bbbb"]);
        assert_eq!(wrap("aa bb, cc", 10), ["aa", "bb,", "cc"]);
        assert_eq!(wrap("Lorem ipsum dolor", 12), ["Lorem", "ipsum", "dolor"]);
    }

    #[test]
    fn spaces_at_the_break_are_dropped() {
        assert_eq!(wrap("aa bb cc", 10), ["aa bb", "cc"]);
        assert_eq!(wrap("aaaa  bb", 10), ["aaaa", "bb"]);
    }

    #[test]
    fn words_longer_than_a_line_are_split() {
        assert_eq!(wrap("a bcdefgh", 6), ["a", "bcd", "efg", "h"]);
        assert_eq!(wrap("abcdefgh", 6), ["abc", "def", "gh"]);
    }

    #[test]
    fn japanese_text_ignores_spaces_for_wrapping() {
        assert_eq!(wrap("あ いうえ", 12), ["あ い", "うえ"]);
    }

    #[test]
    fn unknown_characters_take_the_replacement_width() {
        assert_eq!(wrap("a漢b", 6), ["a漢", "b"]);
    }

    #[test]
    fn break_line_reports_where_the_next_line_starts() {
        let fixture = Fixture::new();
        let fonts = fixture.fonts();
        let set = FontSet::new(&fonts, 0, '〓');

        assert_eq!(
            set.break_line("あい\nう", 12),
            LineBreak { end: 6, next: 7 }
        );
        assert_eq!(
            set.break_line("あいうえ", 12),
            LineBreak { end: 9, next: 9 }
        );
        assert_eq!(set.break_line("", 12), LineBreak { end: 0, next: 0 });
    }
}
