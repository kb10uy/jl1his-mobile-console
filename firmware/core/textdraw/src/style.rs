use embedded_graphics::{
    Pixel,
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::PixelColor,
    primitives::{PointsIter, Rectangle},
    text::{
        Baseline,
        renderer::{CharacterStyle, TextMetrics, TextRenderer},
    },
};

use crate::font::{FontSet, Glyph};

#[derive(Debug, Clone, Copy)]
pub struct TextStyle<'a, C> {
    fonts: FontSet<'a>,
    text_color: Option<C>,
    background_color: Option<C>,
}

impl<'a, C: PixelColor> TextStyle<'a, C> {
    pub const fn new(fonts: FontSet<'a>, text_color: C) -> Self {
        Self {
            fonts,
            text_color: Some(text_color),
            background_color: None,
        }
    }

    pub const fn with_background(fonts: FontSet<'a>, text_color: C, background_color: C) -> Self {
        Self {
            fonts,
            text_color: Some(text_color),
            background_color: Some(background_color),
        }
    }

    pub const fn fonts(&self) -> FontSet<'a> {
        self.fonts
    }

    fn baseline_offset(&self, baseline: Baseline) -> Point {
        let line_height = self.fonts.line_height() as i32;
        let y = match baseline {
            Baseline::Top => 0,
            Baseline::Bottom => line_height - 1,
            Baseline::Middle => (line_height - 1) / 2,
            Baseline::Alphabetic => self.fonts.baseline() as i32,
        };
        Point::new(0, y)
    }

    fn glyphs<'t>(&self, text: &'t str) -> impl Iterator<Item = Glyph<'a>> + use<'a, 't, C> {
        let fonts = self.fonts;
        text.chars()
            .filter_map(move |c| fonts.glyph_or_replacement(c))
    }

    fn draw_glyph<D>(
        &self,
        glyph: Glyph<'a>,
        top_left: Point,
        target: &mut D,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = C>,
    {
        let area = Rectangle::new(top_left, glyph.size());
        let pixels_matching = |wanted: bool, color: C| {
            area.points()
                .zip(glyph.pixels())
                .filter(move |&(_, lit)| lit == wanted)
                .map(move |(point, _)| Pixel(point, color))
        };

        match (self.text_color, self.background_color) {
            (Some(text), Some(background)) => target.fill_contiguous(
                &area,
                glyph
                    .pixels()
                    .map(|lit| if lit { text } else { background }),
            ),
            (Some(text), None) => target.draw_iter(pixels_matching(true, text)),
            (None, Some(background)) => target.draw_iter(pixels_matching(false, background)),
            (None, None) => Ok(()),
        }
    }
}

impl<C: PixelColor> TextRenderer for TextStyle<'_, C> {
    type Color = C;

    fn draw_string<D>(
        &self,
        text: &str,
        position: Point,
        baseline: Baseline,
        target: &mut D,
    ) -> Result<Point, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut next = position;
        for glyph in self.glyphs(text) {
            self.draw_glyph(glyph, next - self.baseline_offset(baseline), target)?;
            next.x += glyph.size().width as i32;
        }
        Ok(next)
    }

    fn draw_whitespace<D>(
        &self,
        width: u32,
        position: Point,
        baseline: Baseline,
        target: &mut D,
    ) -> Result<Point, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if let Some(background) = self.background_color
            && width > 0
        {
            let area = Rectangle::new(
                position - self.baseline_offset(baseline),
                Size::new(width, self.fonts.line_height()),
            );
            target.fill_solid(&area, background)?;
        }
        Ok(position + Point::new(width as i32, 0))
    }

    fn measure_string(&self, text: &str, position: Point, baseline: Baseline) -> TextMetrics {
        let width: u32 = self.glyphs(text).map(|glyph| glyph.size().width).sum();
        TextMetrics {
            bounding_box: Rectangle::new(
                position - self.baseline_offset(baseline),
                Size::new(width, self.fonts.line_height()),
            ),
            next_position: position + Point::new(width as i32, 0),
        }
    }

    fn line_height(&self) -> u32 {
        self.fonts.line_height()
    }
}

impl<C: PixelColor> CharacterStyle for TextStyle<'_, C> {
    type Color = C;

    fn set_text_color(&mut self, text_color: Option<Self::Color>) {
        self.text_color = text_color;
    }

    fn set_background_color(&mut self, background_color: Option<Self::Color>) {
        self.background_color = background_color;
    }
}

#[cfg(test)]
mod tests {
    use embedded_graphics::{
        Drawable, mock_display::MockDisplay, pixelcolor::BinaryColor, text::Text,
    };

    use super::*;
    use crate::{
        Font, Mapping, RangeTable,
        build::{pack_glyph, range_table},
    };

    const NARROW_A: &str = "###\
                            #.#\
                            ###\
                            #.#";
    const NARROW_SPACE: &str = "...\
                                ...\
                                ...\
                                ...";
    const WIDE_KANA: &str = "######\
                             #....#\
                             #....#\
                             ######";
    const WIDE_GETA: &str = "######\
                             ......\
                             ######\
                             ......";

    struct Fixture {
        narrow_glyphs: Vec<u8>,
        narrow_ranges: Vec<u8>,
        wide_glyphs: Vec<u8>,
        wide_ranges: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            let pack = |width, patterns: &[&str]| -> Vec<u8> {
                patterns
                    .iter()
                    .flat_map(|pattern| pack_glyph(width, 4, pattern.chars().map(|c| c == '#')))
                    .collect()
            };
            Self {
                narrow_glyphs: pack(3, &[NARROW_SPACE, NARROW_A]),
                narrow_ranges: range_table(&[0x20..=0x20, 0x41..=0x41]),
                wide_glyphs: pack(6, &[WIDE_KANA, WIDE_GETA]),
                wide_ranges: range_table(&[0x3042..=0x3042, 0x3013..=0x3013]),
            }
        }

        fn fonts(&self) -> [Font<'_>; 2] {
            [
                Font::new(
                    &self.narrow_glyphs,
                    3,
                    4,
                    Mapping::Ranges(RangeTable::new(&self.narrow_ranges)),
                ),
                Font::new(
                    &self.wide_glyphs,
                    6,
                    4,
                    Mapping::Ranges(RangeTable::new(&self.wide_ranges)),
                ),
            ]
        }
    }

    #[test]
    fn mixed_width_text_advances_by_each_glyph_width() {
        let fixture = Fixture::new();
        let fonts = fixture.fonts();
        let style = TextStyle::new(FontSet::new(&fonts, 3, '〓'), BinaryColor::On);
        let mut display = MockDisplay::new();

        let next = Text::with_baseline("Aあ A", Point::new(1, 1), style, Baseline::Top)
            .draw(&mut display)
            .unwrap();

        assert_eq!(next, Point::new(16, 1));
        display.assert_pattern(&[
            "                ",
            " #########   ###",
            " # ##    #   # #",
            " ####    #   ###",
            " # #######   # #",
        ]);
    }

    #[test]
    fn background_color_fills_whole_glyph_cells() {
        let fixture = Fixture::new();
        let fonts = fixture.fonts();
        let style = TextStyle::with_background(
            FontSet::new(&fonts, 3, '〓'),
            BinaryColor::On,
            BinaryColor::Off,
        );
        let mut display = MockDisplay::new();

        Text::with_baseline("A あ", Point::zero(), style, Baseline::Top)
            .draw(&mut display)
            .unwrap();

        display.assert_pattern(&[
            "###...######",
            "#.#...#....#",
            "###...#....#",
            "#.#...######",
        ]);
    }

    #[test]
    fn unknown_characters_draw_the_replacement_glyph() {
        let fixture = Fixture::new();
        let fonts = fixture.fonts();
        let style = TextStyle::new(FontSet::new(&fonts, 3, '〓'), BinaryColor::On);
        let mut display = MockDisplay::new();

        Text::with_baseline("A😀", Point::zero(), style, Baseline::Top)
            .draw(&mut display)
            .unwrap();

        display.assert_pattern(&["#########", "# #      ", "#########", "# #      "]);
    }

    #[test]
    fn alphabetic_baseline_places_the_baseline_row_at_the_position() {
        let fixture = Fixture::new();
        let fonts = fixture.fonts();
        let style = TextStyle::new(FontSet::new(&fonts, 2, '〓'), BinaryColor::On);
        let mut display = MockDisplay::new();

        Text::new("A", Point::new(0, 2), style)
            .draw(&mut display)
            .unwrap();

        display.assert_pattern(&["###", "# #", "###", "# #"]);
    }

    #[test]
    fn measure_string_matches_the_drawn_extent() {
        let fixture = Fixture::new();
        let fonts = fixture.fonts();
        let style = TextStyle::new(FontSet::new(&fonts, 3, '〓'), BinaryColor::On);

        let metrics = style.measure_string("Aあ", Point::new(5, 10), Baseline::Alphabetic);

        assert_eq!(
            metrics.bounding_box,
            Rectangle::new(Point::new(5, 7), Size::new(9, 4))
        );
        assert_eq!(metrics.next_position, Point::new(14, 10));
        assert_eq!(style.line_height(), 4);
    }
}
