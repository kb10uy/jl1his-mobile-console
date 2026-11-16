#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;

const GAIJI_PLACEHOLDER: char = '〓';
const INDENT: char = '　';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Document<'a> {
    pub title: &'a str,
    pub author: &'a str,
    pub body: &'a str,
    pub footer: &'a str,
}

impl<'a> Document<'a> {
    pub fn parse(source: &'a str) -> Self {
        let mut lines = LineOffsets::new(source);
        let title = lines.next().map_or("", |(start, end)| &source[start..end]);
        let author = lines.next().map_or("", |(start, end)| &source[start..end]);

        let mut body_start = lines.position();
        let mut separators = 0;
        let mut footer_start = source.len();
        for (start, end) in lines {
            let line = &source[start..end];
            if separators < 2 && is_separator(line) {
                separators += 1;
                body_start = end;
            } else if line.starts_with("底本：") {
                footer_start = start;
                break;
            }
        }

        Self {
            title,
            author,
            body: source[body_start..footer_start].trim_matches('\n'),
            footer: source[footer_start..].trim_end(),
        }
    }
}

pub fn strip_markup(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut rest = text;
    let mut at_line_start = true;
    let mut line_start = 0;
    let mut dropped_note = false;
    while let Some(c) = rest.chars().next() {
        if let Some((_, after)) = enclosed(rest, "※［＃", '］') {
            plain.push(GAIJI_PLACEHOLDER);
            at_line_start = false;
            rest = after;
        } else if let Some((note, after)) = enclosed(rest, "［＃", '］') {
            if at_line_start && let Some(width) = indent_width(note) {
                plain.extend(core::iter::repeat_n(INDENT, width));
                at_line_start = false;
            } else {
                dropped_note = true;
            }
            rest = after;
        } else if let Some((_, after)) = enclosed(rest, "《", '》') {
            rest = after;
        } else {
            if c == '\n' {
                if !(dropped_note && plain.len() == line_start) {
                    plain.push(c);
                }
                at_line_start = true;
                line_start = plain.len();
                dropped_note = false;
            } else if c != '｜' {
                plain.push(c);
                at_line_start = false;
            }
            rest = &rest[c.len_utf8()..];
        }
    }
    plain
}

fn enclosed<'t>(text: &'t str, open: &str, close: char) -> Option<(&'t str, &'t str)> {
    let inner = text.strip_prefix(open)?;
    let end = inner.find([close, '\n'])?;
    inner[end..]
        .strip_prefix(close)
        .map(|after| (&inner[..end], after))
}

fn indent_width(note: &str) -> Option<usize> {
    let digits = note.strip_suffix("字下げ")?;
    if digits.is_empty() {
        return None;
    }
    digits.chars().try_fold(0, |width: usize, c| {
        let digit = match c {
            '0'..='9' => c.to_digit(10),
            '０'..='９' => Some(u32::from(c) - u32::from('０')),
            _ => None,
        }?;
        width.checked_mul(10)?.checked_add(digit as usize)
    })
}

fn is_separator(line: &str) -> bool {
    line.len() >= 5 && line.bytes().all(|byte| byte == b'-')
}

struct LineOffsets<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> LineOffsets<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    fn position(&self) -> usize {
        self.position
    }
}

impl Iterator for LineOffsets<'_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.position;
        if start >= self.source.len() {
            return None;
        }
        let end = self.source[start..]
            .find('\n')
            .map_or(self.source.len(), |offset| start + offset);
        self.position = (end + 1).min(self.source.len());
        Some((start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "坊っちゃん
夏目漱石

-------------------------------------------------------
【テキスト中に現れる記号について】

《》：ルビ
（例）坊《ぼ》っちゃん
-------------------------------------------------------

［＃５字下げ］一［＃「一」は中見出し］

　親譲《おやゆず》りの無鉄砲《むてっぽう》で小供の時から損ばかりしている。
　ある日の夕方｜折戸《おりど》の蔭《かげ》に隠《かく》れて



底本：「ちくま日本文学全集　夏目漱石」筑摩書房
入力：真先芳秋
";

    #[test]
    fn parse_splits_title_author_body_and_footer() {
        let document = Document::parse(SAMPLE);

        assert_eq!(document.title, "坊っちゃん");
        assert_eq!(document.author, "夏目漱石");
        assert!(document.body.starts_with("［＃５字下げ］一"));
        assert!(document.body.ends_with("隠《かく》れて"));
        assert_eq!(
            document.footer,
            "底本：「ちくま日本文学全集　夏目漱石」筑摩書房\n入力：真先芳秋"
        );
    }

    #[test]
    fn parse_without_header_block_starts_the_body_after_the_author() {
        let document = Document::parse("題\n作者\n\n本文\n");

        assert_eq!(document.body, "本文");
        assert_eq!(document.footer, "");
    }

    #[test]
    fn ruby_and_ruby_start_markers_are_removed() {
        assert_eq!(strip_markup("親譲《おやゆず》りの"), "親譲りの");
        assert_eq!(strip_markup("夕方｜折戸《おりど》の"), "夕方折戸の");
    }

    #[test]
    fn line_start_indent_becomes_ideographic_spaces() {
        assert_eq!(
            strip_markup("［＃５字下げ］一［＃「一」は中見出し］\n本文"),
            "　　　　　一\n本文"
        );
        assert_eq!(
            strip_markup("［＃12字下げ］x"),
            format!("{}x", "　".repeat(12))
        );
    }

    #[test]
    fn indent_outside_the_line_start_is_dropped() {
        assert_eq!(strip_markup("本文［＃２字下げ］続き"), "本文続き");
    }

    #[test]
    fn other_annotations_are_removed() {
        assert_eq!(
            strip_markup("おくれんかな［＃「おくれんかな」に傍点］と"),
            "おくれんかなと"
        );
        assert_eq!(strip_markup("［＃改ページ］\n次"), "次");
    }

    #[test]
    fn lines_holding_only_annotations_are_removed() {
        assert_eq!(
            strip_markup("前\n［＃ここから３字下げ］\n本文\n\n［＃ここで字下げ終わり］\n後"),
            "前\n本文\n\n後"
        );
        assert_eq!(strip_markup("本文\n［＃改ページ］"), "本文\n");
    }

    #[test]
    fn gaiji_notes_become_the_placeholder() {
        assert_eq!(
            strip_markup("※［＃「てへん＋劣」、第3水準1-84-77］る"),
            "〓る"
        );
        assert_eq!(strip_markup("※底本の注"), "※底本の注");
    }

    #[test]
    fn unterminated_markup_is_kept_as_text() {
        assert_eq!(strip_markup("《閉じない\n次《る》"), "《閉じない\n次");
        assert_eq!(strip_markup("［＃閉じない"), "［＃閉じない");
    }

    #[test]
    fn parsed_body_strips_to_plain_text() {
        let plain = strip_markup(Document::parse(SAMPLE).body);

        assert_eq!(
            plain,
            "　　　　　一\n\n　親譲りの無鉄砲で小供の時から損ばかりしている。\n　ある日の夕方折戸の蔭に隠れて"
        );
    }
}
