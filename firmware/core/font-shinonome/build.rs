use std::{
    collections::BTreeMap,
    env, fs,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use encoding_rs::EUC_JP;
use textdraw::build::{pack_glyph, range_table, rank_table};

const SOURCE_ROOT: &str = "vendor/shinonome-font";
const HEIGHT: u32 = 12;
const HALF_WIDTH: u32 = 6;
const FULL_WIDTH: u32 = 12;

const LATIN1_RANGES: [RangeInclusive<u16>; 2] = [0x20..=0x7E, 0xA0..=0xFF];
const JIS_X0201_KANA: RangeInclusive<u16> = 0xA1..=0xDF;
const HALFWIDTH_KANA: RangeInclusive<u16> = 0xFF61..=0xFF9F;

const ALIASES: [(char, char); 7] = [
    ('\u{301C}', '\u{FF5E}'),
    ('\u{2016}', '\u{2225}'),
    ('\u{2212}', '\u{FF0D}'),
    ('\u{2015}', '\u{2014}'),
    ('\u{00A2}', '\u{FFE0}'),
    ('\u{00A3}', '\u{FFE1}'),
    ('\u{00AC}', '\u{FFE2}'),
];

type Glyphs = BTreeMap<u16, Vec<u8>>;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let source = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join(SOURCE_ROOT);
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    let latin1 = load(&source.join("12/latin1/font_src.bit"), HALF_WIDTH);
    let kana = load(&source.join("12/hankaku/font_src_diff.bit"), HALF_WIDTH);
    let kanji = load(&source.join("12/kanjic/font_src.bit"), FULL_WIDTH);

    let half_glyphs: Vec<u8> = LATIN1_RANGES
        .into_iter()
        .flatten()
        .map(|code| glyph(&latin1, code))
        .chain(JIS_X0201_KANA.map(|code| glyph(&kana, code)))
        .flatten()
        .copied()
        .collect();
    let half_ranges = range_table(&[
        LATIN1_RANGES[0].clone(),
        LATIN1_RANGES[1].clone(),
        HALFWIDTH_KANA,
    ]);

    let mut full: BTreeMap<u16, &[u8]> = kanji
        .iter()
        .map(|(&jis, data)| (bmp_code_point(jis_to_char(jis)), data.as_slice()))
        .collect();
    assert_eq!(full.len(), kanji.len(), "JIS codes share a code point");
    for (first, second) in ALIASES.map(|(a, b)| (bmp_code_point(a), bmp_code_point(b))) {
        match (full.get(&first).copied(), full.get(&second).copied()) {
            (Some(data), None) => full.insert(second, data),
            (None, Some(data)) => full.insert(first, data),
            (Some(_), Some(_)) => None,
            (None, None) => panic!("alias U+{first:04X} / U+{second:04X} matches no glyph"),
        };
    }
    let full_glyphs: Vec<u8> = full.values().copied().flatten().copied().collect();
    let full_rank = rank_table(full.keys().copied());

    write(&out, "half12.glyphs", &half_glyphs);
    write(&out, "half12.ranges", &half_ranges);
    write(&out, "full12.glyphs", &full_glyphs);
    write(&out, "full12.rank", &full_rank);
}

fn load(path: &Path, width: u32) -> Glyphs {
    println!("cargo::rerun-if-changed={}", path.display());
    let text = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {error}\n\
             fetch the Shinonome font sources with `git submodule update --init`",
            path.display()
        )
    });

    let mut glyphs = Glyphs::new();
    let mut encoding = None;
    let mut pixels: Option<Vec<bool>> = None;
    for (index, line) in text.lines().enumerate() {
        let location = format!("{}:{}", path.display(), index + 1);
        if let Some(value) = line.strip_prefix("ENCODING ") {
            let code = value
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("{location}: bad ENCODING"));
            encoding = Some(code);
        } else if line.starts_with("BITMAP") {
            pixels = Some(Vec::new());
        } else if line.starts_with("ENDCHAR") {
            let code = encoding
                .take()
                .unwrap_or_else(|| panic!("{location}: no ENCODING"));
            let pixels = pixels
                .take()
                .unwrap_or_else(|| panic!("{location}: no BITMAP"));
            let previous = glyphs.insert(code, pack_glyph(width, HEIGHT, pixels));
            assert!(previous.is_none(), "{location}: duplicate ENCODING {code}");
        } else if let Some(pixels) = &mut pixels {
            assert_eq!(line.len(), width as usize, "{location}: bad row width");
            pixels.extend(line.chars().map(|c| match c {
                '@' => true,
                '.' => false,
                other => panic!("{location}: unexpected bitmap character {other:?}"),
            }));
        }
    }
    glyphs
}

fn glyph(glyphs: &Glyphs, code: u16) -> &[u8] {
    glyphs
        .get(&code)
        .unwrap_or_else(|| panic!("source font has no glyph for code {code:#06x}"))
}

fn jis_to_char(jis: u16) -> char {
    let euc = jis.to_be_bytes().map(|byte| byte | 0x80);
    let text = EUC_JP
        .decode_without_bom_handling_and_without_replacement(&euc)
        .unwrap_or_else(|| panic!("JIS code {jis:#06x} has no Unicode mapping"));
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c,
        _ => panic!("JIS code {jis:#06x} maps to {text:?}"),
    }
}

fn bmp_code_point(c: char) -> u16 {
    u16::try_from(u32::from(c)).unwrap_or_else(|_| panic!("{c:?} is outside the BMP"))
}

fn write(out: &Path, name: &str, data: &[u8]) {
    let path = out.join(name);
    fs::write(&path, data)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
}
