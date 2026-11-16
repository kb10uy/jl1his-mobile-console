use std::{collections::BTreeSet, env, fs, path::PathBuf};

use aozora::{Document, strip_markup};
use font_shinonome::SHINONOME_12;

enum Format {
    Plain,
    Aozora { last_sentence: &'static str },
}

struct TextAsset {
    source: &'static str,
    output: &'static str,
    format: Format,
}

const TEXTS: [TextAsset; 4] = [
    TextAsset {
        source: "assets/bocchan.txt",
        output: "botchan.txt",
        format: Format::Aozora {
            last_sentence: "やっぱり立っていた。何だか大変小さく見えた。",
        },
    },
    TextAsset {
        source: "assets/udhr.txt",
        output: "udhr.txt",
        format: Format::Plain,
    },
    TextAsset {
        source: "assets/poranono_hiroba.txt",
        output: "polano.txt",
        format: Format::Aozora {
            last_sentence: "イーハトーヴォの五月から十月までを書きつけましょう。",
        },
    },
    TextAsset {
        source: "assets/lorem-ipsum.txt",
        output: "lorem-ipsum.txt",
        format: Format::Plain,
    },
];

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    fs::write(out.join("memory.x"), include_bytes!("memory.x")).unwrap();
    println!("cargo::rustc-link-search={}", out.display());
    println!("cargo::rerun-if-changed=memory.x");
    println!("cargo::rustc-link-arg-bins=--nmagic");
    println!("cargo::rustc-link-arg-bins=-Tlink.x");

    for asset in TEXTS {
        println!("cargo::rerun-if-changed={}", asset.source);
        let source = fs::read_to_string(manifest.join(asset.source)).unwrap();
        let text = match asset.format {
            Format::Plain => source,
            Format::Aozora { last_sentence } => {
                aozora_excerpt(asset.source, &source, last_sentence)
            }
        };
        warn_missing_glyphs(asset.source, &text);
        fs::write(out.join(asset.output), text).unwrap();
    }
}

fn aozora_excerpt(name: &str, source: &str, last_sentence: &str) -> String {
    let document = Document::parse(source);
    let body = strip_markup(document.body);
    let end = body
        .find(last_sentence)
        .unwrap_or_else(|| panic!("{name}: the excerpt's last sentence is missing"))
        + last_sentence.len();
    format!(
        "{}\n{}\n\n{}\n\n{}\n",
        document.title,
        document.author,
        &body[..end],
        document.footer
    )
}

fn warn_missing_glyphs(source: &str, text: &str) {
    let missing: BTreeSet<char> = text
        .chars()
        .filter(|&c| c != '\n' && SHINONOME_12.glyph(c).is_none())
        .collect();
    for c in missing {
        println!(
            "cargo::warning={source}: no Shinonome glyph for {c:?} (U+{:04X})",
            u32::from(c)
        );
    }
}
