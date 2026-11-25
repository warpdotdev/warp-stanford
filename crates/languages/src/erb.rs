//! Finds the Ruby embedded in an ERB template.

use std::ops::Range;

use crate::EmbeddedTag;

const OPEN: &[u8] = b"<%";
const CLOSE: &[u8] = b"%>";

/// Returns every ERB tag in `source`, in document order.
///
/// A tag runs from `<%` to the next `%>`. A tag that is never closed runs to the end of the
/// document, which is how ERB reads it. `<%%` and `%%>` are ERB's escapes for a literal `<%`
/// and `%>`, so they neither open nor close a tag.
pub(crate) fn tags(source: &[u8]) -> Vec<EmbeddedTag> {
    let mut tags = Vec::new();
    let mut position = 0;
    while let Some(open) = find(source, position, OPEN) {
        let body_start = open + OPEN.len();
        if source.get(body_start) == Some(&b'%') {
            position = body_start + 1;
            continue;
        }

        let body_end = find_close(source, body_start).unwrap_or(source.len());
        let end = (body_end + CLOSE.len()).min(source.len());
        tags.push(EmbeddedTag {
            range: open..end,
            code: code_range(source, body_start..body_end),
        });
        position = end;
    }
    tags
}

/// Narrows the body of a tag to the Ruby inside it. The `=` of an output tag and the `-` that
/// trims whitespace belong to ERB, and the body of a `<%#` comment is not code at all.
fn code_range(source: &[u8], body: Range<usize>) -> Option<Range<usize>> {
    let mut start = body.start;
    let mut end = body.end;
    match source.get(start) {
        Some(b'#') => return None,
        Some(b'-') => start += 1,
        Some(b'=') => {
            start += 1;
            // `<%==` is Rails' unescaped output tag.
            if source.get(start) == Some(&b'=') {
                start += 1;
            }
        }
        _ => {}
    }
    if end > start && source[end - 1] == b'-' {
        end -= 1;
    }
    (start < end).then_some(start..end)
}

fn find_close(source: &[u8], from: usize) -> Option<usize> {
    let mut position = from;
    while let Some(close) = find(source, position, CLOSE) {
        let is_escaped = close > from && source[close - 1] == b'%';
        if !is_escaped {
            return Some(close);
        }
        position = close + CLOSE.len();
    }
    None
}

fn find(source: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    source
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

#[cfg(test)]
#[path = "erb_tests.rs"]
mod tests;
