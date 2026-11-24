use std::iter;
use std::ops::Range;

use arborium::tree_sitter::{Node, Query, QueryCursor, TextProvider, Tree};
use rangemap::RangeMap;
use streaming_iterator::StreamingIterator;
use string_offset::{ByteOffset, CharOffset};
use warp_editor::content::buffer::{Buffer, ToBufferByteOffset, ToBufferCharOffset};
use warp_editor::content::text::Bytes;
use warpui_core::color::ColorU;

/// Color mapping from parsed syntax token name to its corresponding highlighting color.
#[derive(Clone, Copy)]
pub struct ColorMap {
    pub keyword_color: ColorU,
    pub function_color: ColorU,
    pub string_color: ColorU,
    pub type_color: ColorU,
    pub number_color: ColorU,
    pub comment_color: ColorU,
    pub property_color: ColorU,
    pub tag_color: ColorU,
}

/// Query for retrieving syntax highlighting information on the tokens.
pub struct HighlightQuery {
    highlight_map: Vec<Option<ColorU>>,
}

impl HighlightQuery {
    pub fn new(query: &Query, color_map: ColorMap) -> Self {
        let highlight_map = query
            .capture_names()
            .iter()
            .map(|name| convert_capture_name_to_color(name, &color_map))
            .collect();

        Self { highlight_map }
    }

    /// Given the a character range, return its corresponding highlight colors.
    pub fn get_highlighted_chunks(
        &self,
        range: Range<CharOffset>,
        query: &Query,
        buffer: &Buffer,
        tree: &Tree,
    ) -> RangeMap<CharOffset, ColorU> {
        let mut range_map = RangeMap::new();

        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(tree_byte(range.start, buffer)..tree_byte(range.end, buffer));
        let mut captures = cursor.captures(query, tree.root_node(), TextBuffer(buffer));

        while let Some(matches) = captures.next() {
            for cap in matches.0.captures {
                let insertion_range = cap.node.byte_range();
                let color = self
                    .highlight_map
                    .get(cap.index as usize)
                    .and_then(|inner| *inner);

                if let Some(color) = color {
                    let char_start = char_offset(insertion_range.start, buffer);
                    let char_end = char_offset(insertion_range.end, buffer);
                    if char_start < char_end {
                        range_map.insert(char_start..char_end, color);
                    }
                }
            }
        }

        range_map
    }
}

/// The buffer keeps a one-byte block marker ahead of its text, which the syntax tree is parsed
/// without. Byte `n` of the tree is therefore byte `n + 1` of the buffer.
fn buffer_byte(tree_byte: usize) -> ByteOffset {
    ByteOffset::from(tree_byte + 1)
}

/// Converts a byte position in the syntax tree to the character position highlights are keyed
/// by, which also counts from the start of the text.
fn char_offset(tree_byte: usize, buffer: &Buffer) -> CharOffset {
    let buffer_chars = buffer_byte(tree_byte).to_buffer_char_offset(buffer);
    CharOffset::from(buffer_chars.as_usize().saturating_sub(1))
}

/// Converts a character position counted from the start of the text to its byte position in the
/// syntax tree.
fn tree_byte(offset: CharOffset, buffer: &Buffer) -> usize {
    let buffer_bytes = CharOffset::from(offset.as_usize() + 1).to_buffer_byte_offset(buffer);
    buffer_bytes.as_usize().saturating_sub(1)
}

fn convert_capture_name_to_color(name: &str, color_map: &ColorMap) -> Option<ColorU> {
    match name {
        "text.title" => return Some(color_map.keyword_color),
        "text.literal" => return Some(color_map.string_color),
        "text.uri" => return Some(color_map.function_color),
        "text.reference" => return Some(color_map.property_color),
        _ => {}
    }
    match name.split('.').next() {
        // Some bundled grammars (e.g. Lua, Kotlin, Scala) still use these legacy capture names for
        // keywords instead of `keyword.*`.
        Some("keyword" | "conditional" | "repeat" | "exception" | "include") => {
            Some(color_map.keyword_color)
        }
        Some("function") => Some(color_map.function_color),
        Some("string") => Some(color_map.string_color),
        Some("type") => Some(color_map.type_color),
        Some("number") => Some(color_map.number_color),
        Some("comment") => Some(color_map.comment_color),
        Some("property") => Some(color_map.property_color),
        Some("tag") => Some(color_map.tag_color),
        Some("punctuation") => Some(color_map.comment_color),
        _ => None,
    }
}

// The default tree-sitter implementation here is unsafe (since the cursor could query invalid ranges outside of content length).
// TODO(kevin): Once we migrate buffer to store ArrayStrings. We should implement the chunks API on buffer directly to avoid collecting
// into a String and then chunking them again for highlighting.
pub struct TextSlice<'a>(pub &'a [u8]);

impl TextSlice<'_> {
    fn get(&self, range: Range<usize>) -> Self {
        Self(self.0.get(range).unwrap_or_default())
    }
}

impl AsRef<[u8]> for TextSlice<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl<'a> TextProvider<TextSlice<'a>> for TextSlice<'a> {
    type I = iter::Once<TextSlice<'a>>;

    fn text(&mut self, node: Node) -> Self::I {
        iter::once(self.get(node.byte_range()))
    }
}

pub struct TextBuffer<'a>(pub &'a Buffer);

impl<'a> TextProvider<&'a [u8]> for TextBuffer<'a> {
    type I = Bytes<'a>;

    fn text(&mut self, node: Node) -> Self::I {
        let range = node.range();
        self.0
            .bytes_in_range(buffer_byte(range.start_byte), buffer_byte(range.end_byte))
    }
}

#[cfg(test)]
#[path = "highlight_query_tests.rs"]
mod tests;
