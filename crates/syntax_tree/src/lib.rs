mod queries;
use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use arborium::tree_sitter::{
    InputEdit, Language as ParserGrammar, Parser, Range as ParserRange, Tree,
};
use futures::stream::AbortHandle;
use languages::{EmbeddedTag, Language};
use parking_lot::Mutex;
use queries::highlight_query::HighlightQuery;
pub use queries::highlight_query::{ColorMap, TextSlice};
use queries::indent_query::{IndentDelta, indentation_delta};
use rangemap::{RangeMap, RangeSet};
use string_offset::{ByteOffset, CharOffset};
use warp_editor::content::buffer::{
    Buffer, BufferSnapshot, ToBufferByteOffset, ToBufferCharOffset,
};
use warp_editor::content::edit::PreciseDelta;
use warp_editor::content::text::IndentUnit;
use warp_editor::content::version::BufferVersion;
use warp_editor::decoration::DecorationLayer;
use warpui_core::color::ColorU;
use warpui_core::text::point::Point;
use warpui_core::{AppContext, Entity, ModelContext, WeakModelHandle};

const MAX_SYNTAX_TREES: usize = 3;

/// Maximum buffer size in bytes for which we attempt to parse a syntax tree.
/// Files larger than this are skipped to avoid tree-sitter's super-linear
/// memory growth on large inputs. See this tree-sitter issue:
/// https://github.com/tree-sitter/tree-sitter/issues/222#issuecomment-435987441
const MAX_PARSE_BYTES: usize = 2 * 1024 * 1024; // 2 MB

thread_local! {
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}
pub enum DecorationStateEvent {
    DecorationUpdated { version: BufferVersion },
}

struct LanguageQueries {
    language: Arc<Language>,
    syntax_query: HighlightQuery,
    /// Highlights the code inside the tags of a template language.
    embedded_syntax_query: Option<HighlightQuery>,
}

impl LanguageQueries {
    fn new(language: Arc<Language>, color_map: ColorMap) -> Self {
        Self {
            syntax_query: HighlightQuery::new(&language.highlight_query, color_map),
            embedded_syntax_query: language
                .embedded
                .as_ref()
                .map(|embedded| HighlightQuery::new(&embedded.language.highlight_query, color_map)),
            language,
        }
    }
}

/// The syntax trees of one buffer version.
#[derive(Clone)]
struct SyntaxTrees {
    /// Tree for the language's own grammar. For a template language it covers only the text
    /// outside the tags.
    host: Tree,
    /// Tree for the code inside the tags of a template language. `None` for every other
    /// language, and for a template that has no embedded code.
    embedded: Option<Tree>,
}

impl SyntaxTrees {
    fn edit(&mut self, edit: &InputEdit) {
        self.host.edit(edit);
        if let Some(embedded) = &mut self.embedded {
            embedded.edit(edit);
        }
    }
}

/// Single-entry cache for highlight queries.
/// Stores the most recent highlight computation result.
struct HighlightCache {
    key: HighlightCacheKey,
    highlights: RangeMap<CharOffset, ColorU>,
}

struct HighlightCacheKey {
    version: BufferVersion,
    ranges: RangeSet<CharOffset>,
    language_id: Option<arborium::tree_sitter::Language>,
}

impl HighlightCacheKey {
    /// Check if this cache entry matches the given content version, ranges, and language.
    fn matches(
        &self,
        version: BufferVersion,
        ranges: &RangeSet<CharOffset>,
        language_id: &Option<arborium::tree_sitter::Language>,
    ) -> bool {
        if self.version != version {
            return false;
        }
        if &self.language_id != language_id {
            return false;
        }
        // RangeSet derives PartialEq, so we can compare directly
        &self.ranges == ranges
    }
}

/// Manages the decoration styles derived from the underlying text source (e.g. syntax highlighting).
/// The updates are computed asynchronously and we notify the editor model upon completion via
/// DecorationUpdated event.
pub struct SyntaxTreeState {
    syntax_tree: Mutex<HashMap<BufferVersion, SyntaxTrees>>,
    language_queries: Option<LanguageQueries>,
    buffer_version: BufferVersion,
    color_map: ColorMap,
    buffer_handle: WeakModelHandle<Buffer>,
    parsing_handle: Option<AbortHandle>,
    /// Cache for highlight results to avoid recomputing for the same viewport ranges.
    highlight_cache: RefCell<Option<HighlightCache>>,
}

impl SyntaxTreeState {
    pub fn new(
        buffer_handle: WeakModelHandle<Buffer>,
        buffer_version: BufferVersion,
        color_map: ColorMap,
    ) -> Self {
        Self {
            color_map,
            syntax_tree: Mutex::new(HashMap::new()),
            buffer_version,
            buffer_handle,
            parsing_handle: None,
            language_queries: None,
            highlight_cache: RefCell::new(None),
        }
    }

    pub fn set_language(&mut self, language: Arc<Language>) {
        self.language_queries = Some(LanguageQueries::new(language, self.color_map));
    }

    pub fn has_supported_highlighting(&self) -> bool {
        self.language_queries.is_some()
    }

    pub fn indent_unit(&self) -> Option<IndentUnit> {
        self.language_queries
            .as_ref()
            .map(|queries| queries.language.indent_unit)
    }

    pub fn bracket_pairs(&self) -> Option<&[(char, char)]> {
        self.language_queries
            .as_ref()
            .map(|queries| queries.language.bracket_pairs.as_slice())
    }

    pub fn comment_prefix(&self) -> Option<&str> {
        self.language_queries
            .as_ref()
            .and_then(|queries| queries.language.comment_prefix.as_ref())
            .map(|s| s.as_str())
    }

    /// Given multiple character ranges, return their corresponding highlight colors.
    /// If the tree is not ready or the buffer model has been deallocated, this returns None.
    pub fn highlights_in_ranges(
        &self,
        ranges: RangeSet<CharOffset>,
        render_content_version: Option<BufferVersion>,
        ctx: &AppContext,
    ) -> Option<Ref<'_, RangeMap<CharOffset, ColorU>>> {
        // If no render content version is provided, default the most recent content version.
        let buffer_version = render_content_version.unwrap_or(self.buffer_version);

        let language_id = self
            .language_queries
            .as_ref()
            .map(|q| q.language.grammar.clone());

        // Check cache first
        if let Ok(cache) = Ref::filter_map(self.highlight_cache.borrow(), |c| c.as_ref())
            && cache.key.matches(buffer_version, &ranges, &language_id)
        {
            // Return a borrowed reference to the cached highlights
            return Some(Ref::map(cache, |c| &c.highlights));
        }

        // Cache miss - compute highlights
        let mut syntax_tree_lock = self.syntax_tree.lock();
        let trees = syntax_tree_lock.get(&buffer_version)?;
        let buffer = self.buffer_handle.upgrade(ctx)?;
        let language_queries = self.language_queries.as_ref()?;

        let mut combined_highlights = RangeMap::new();

        // Iterate over all ranges and collect highlights for each
        for range in ranges.iter() {
            let highlights = highlighted_chunks(
                range.clone(),
                language_queries,
                &self.color_map,
                buffer.as_ref(ctx),
                trees,
            );

            // Merge the highlights into the combined map
            for (highlight_range, color) in highlights.iter() {
                combined_highlights.insert(highlight_range.clone(), *color);
            }
        }

        // Once we have rendered content version X, we could discard syntax trees belonging to versions before X.
        if let Some(render_content_version) = render_content_version {
            // First, drop any versions older than the rendered one in a single pass.
            syntax_tree_lock.retain(|version, _| *version >= render_content_version);
            Self::truncate_tree_state(&mut syntax_tree_lock, self.buffer_version);
        }

        // Store in cache before returning
        *self.highlight_cache.borrow_mut() = Some(HighlightCache {
            key: HighlightCacheKey {
                version: buffer_version,
                ranges,
                language_id,
            },
            highlights: combined_highlights,
        });

        // Return a borrowed reference to the cached highlights
        Ref::filter_map(self.highlight_cache.borrow(), |c| {
            c.as_ref().map(|cache| &cache.highlights)
        })
        .ok()
    }

    /// Given a point in buffer, return the absolute indentation level the point should have.
    pub fn indentation_at_point(&self, point: Point, ctx: &AppContext) -> Option<IndentDelta> {
        let syntax_tree_lock = self.syntax_tree.lock();
        let trees = syntax_tree_lock.get(&self.buffer_version)?;
        let buffer = self.buffer_handle.upgrade(ctx)?;
        let language_queries = self.language_queries.as_ref()?;

        indentation_delta(
            buffer.as_ref(ctx),
            &trees.host,
            point,
            language_queries.language.indents_query.as_ref()?,
        )
    }

    /// Re-parse the trees based on the updated trees and source content.
    ///
    /// Returns `None` if the buffer exceeds [`MAX_PARSE_BYTES`].
    async fn parse_trees(
        content: BufferSnapshot,
        old_trees: Option<SyntaxTrees>,
        language: &Language,
    ) -> Option<SyntaxTrees> {
        if content.byte_len() > MAX_PARSE_BYTES {
            return None;
        }
        let (old_host, old_embedded) = match old_trees {
            Some(trees) => (Some(trees.host), trees.embedded),
            None => (None, None),
        };

        let Some(embedded) = &language.embedded else {
            return Some(SyntaxTrees {
                host: Self::parse(&content, &language.grammar, &[], old_host.as_ref()),
                embedded: None,
            });
        };

        let source = source_bytes(&content);
        let tags = (embedded.find_tags)(&source);
        let ranges = template_ranges(&source, &tags);
        let host = Self::parse(&content, &language.grammar, &ranges.host, old_host.as_ref());
        // An empty list of ranges means the whole document to the parser, so a template with no
        // embedded code gets no embedded tree at all.
        let embedded = (!ranges.code.is_empty()).then(|| {
            Self::parse(
                &content,
                &embedded.language.grammar,
                &ranges.code,
                old_embedded.as_ref(),
            )
        });
        Some(SyntaxTrees { host, embedded })
    }

    /// Parses the parts of `content` inside `included_ranges` with `grammar`. An empty list of
    /// ranges parses the whole document.
    fn parse(
        content: &BufferSnapshot,
        grammar: &ParserGrammar,
        included_ranges: &[ParserRange],
        old_tree: Option<&Tree>,
    ) -> Tree {
        PARSER.with(|parser| {
            let mut parser = parser.borrow_mut();
            parser.set_language(grammar).expect("incompatible grammar");
            // The parser is shared, so this also clears the ranges of the previous parse.
            parser
                .set_included_ranges(included_ranges)
                .expect("ranges should be in order and should not overlap");
            let mut bytes = content.bytes();
            let mut callback = |byte_offset: usize, _point: arborium::tree_sitter::Point| {
                // Add 1 since the buffer is 1 indexed.
                bytes.seek(ByteOffset::from(byte_offset + 1));
                bytes.next().unwrap_or_default()
            };
            parser
                .parse_with_options(&mut callback, old_tree, None)
                .expect("Should succeed")
        })
    }

    /// Translate an incoming edit delta into an InputEdit for incrementally updating the syntax
    /// tree. Uses the precomputed byte edit info (which was captured from the correct intermediate
    /// buffer state) and `replaced_points` instead of re-deriving from the final buffer.
    fn delta_to_input_edit(delta: &PreciseDelta) -> InputEdit {
        // Convert 1-indexed ByteOffset values to 0-indexed for tree-sitter.
        let start_byte = delta.replaced_byte_range.start.as_usize().saturating_sub(1);
        let old_end_byte = delta.replaced_byte_range.end.as_usize().saturating_sub(1);

        InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte: start_byte + delta.new_byte_length,
            start_position: point_to_syntax_point(delta.replaced_points.start),
            old_end_position: point_to_syntax_point(delta.replaced_points.end),
            new_end_position: point_to_syntax_point(delta.new_end_point),
        }
    }

    pub fn invalidate_highlight_cache_for_version(&self, version: BufferVersion) {
        // Check if the cache exists and if it matches the version being invalidated
        let mut cache = self.highlight_cache.borrow_mut();
        if let Some(ref cached) = *cache
            && cached.key.version == version
        {
            *cache = None;
        }
    }

    pub fn set_color_map(&mut self, color_map: ColorMap) {
        self.color_map = color_map;
        if let Some(language_query) = self.language_queries.take() {
            self.set_language(language_query.language);
        }
        // Clear highlight cache since colors have changed
        *self.highlight_cache.borrow_mut() = None;
    }

    /// Truncates the syntax tree cache to maintain the MAX_SYNTAX_TREES policy.
    /// Keeps the oldest MAX_SYNTAX_TREES - 1 versions and the provided content_version.
    fn truncate_tree_state(
        syntax_tree_lock: &mut HashMap<BufferVersion, SyntaxTrees>,
        buffer_version: BufferVersion,
    ) {
        if syntax_tree_lock.len() <= MAX_SYNTAX_TREES {
            return;
        }

        let mut versions: Vec<BufferVersion> = syntax_tree_lock.keys().copied().collect();
        versions.sort();

        let mut keep: HashSet<BufferVersion> = versions
            .iter()
            .take(MAX_SYNTAX_TREES - 1)
            .copied()
            .collect();
        keep.insert(buffer_version);

        syntax_tree_lock.retain(|v, _| keep.contains(v));
    }
}

impl DecorationLayer for SyntaxTreeState {
    fn update_internal_state_with_delta(
        &mut self,
        deltas: &[PreciseDelta],
        version: BufferVersion,
        content: BufferSnapshot,
        ctx: &mut ModelContext<Self>,
    ) {
        // If there is an active parsing in progress. Abort that first before starting another one.
        if let Some(handle) = self.parsing_handle.take() {
            handle.abort();
        }

        let Some(language) = self
            .language_queries
            .as_ref()
            .map(|language_queries| language_queries.language.clone())
        else {
            return;
        };

        let mut syntax_tree_lock = self.syntax_tree.lock();
        let mut trees = syntax_tree_lock.get(&self.buffer_version).cloned();
        if let Some(trees) = &mut trees {
            for delta in deltas {
                let edit = Self::delta_to_input_edit(delta);
                trees.edit(&edit);
            }

            // We write to the tree immediately after editing first to prevent flickering in the render
            // state before reparsing gets completed.
            if let Some(existing) = syntax_tree_lock.get_mut(&version) {
                existing.clone_from(trees);
            } else {
                syntax_tree_lock.insert(version, trees.clone());
                Self::truncate_tree_state(&mut syntax_tree_lock, version);
            }
        }

        let handle = ctx
            .spawn(
                async move {
                    let new_trees = Self::parse_trees(content, trees, &language).await;
                    futures_lite::future::yield_now().await;
                    new_trees
                },
                move |model, new_trees, ctx| {
                    let Some(new_trees) = new_trees else {
                        // Buffer exceeded MAX_PARSE_BYTES; skip updating the syntax tree, but
                        // still emit DecorationUpdated so any delayed rendering is flushed
                        let mut syntax_tree_lock = model.syntax_tree.lock();
                        syntax_tree_lock.remove(&version);
                        drop(syntax_tree_lock);
                        model.invalidate_highlight_cache_for_version(version);
                        // (the editor delays showing content until this event fires).
                        ctx.emit(DecorationStateEvent::DecorationUpdated { version });
                        return;
                    };
                    let mut syntax_tree_lock = model.syntax_tree.lock();
                    model.invalidate_highlight_cache_for_version(version);
                    if let Some(old_trees) = syntax_tree_lock.get_mut(&version) {
                        *old_trees = new_trees;
                    } else {
                        // This is for the case where we are updating the syntax tree for the first time.
                        syntax_tree_lock.insert(version, new_trees);
                        Self::truncate_tree_state(&mut syntax_tree_lock, model.buffer_version);
                    }
                    ctx.emit(DecorationStateEvent::DecorationUpdated { version });
                },
            )
            .abort_handle();

        self.buffer_version = version;
        self.parsing_handle = Some(handle);
    }
}

impl Entity for SyntaxTreeState {
    type Event = DecorationStateEvent;
}

/// Returns the highlight colors for `range`.
///
/// In a template language the host tree colors the text outside the tags and the embedded tree
/// colors the code inside them. What is left of a tag is its delimiters, or all of it when it is
/// a comment, and takes the comment color like other punctuation.
fn highlighted_chunks(
    range: Range<CharOffset>,
    queries: &LanguageQueries,
    color_map: &ColorMap,
    buffer: &Buffer,
    trees: &SyntaxTrees,
) -> RangeMap<CharOffset, ColorU> {
    let language = &queries.language;
    let mut highlights = queries.syntax_query.get_highlighted_chunks(
        range.clone(),
        &language.highlight_query,
        buffer,
        &trees.host,
    );
    let (Some(embedded), Some(embedded_query)) =
        (&language.embedded, &queries.embedded_syntax_query)
    else {
        return highlights;
    };

    // A host node such as a quoted attribute value can span a tag, so painting the tags also
    // keeps the host's color out of them.
    let visible_bytes = visible_byte_range(&range, buffer);
    for tag in gaps_between(&trees.host.included_ranges(), &visible_bytes) {
        if let Some(tag) = char_range(tag, &range, buffer) {
            highlights.insert(tag, color_map.comment_color);
        }
    }

    let Some(embedded_tree) = &trees.embedded else {
        return highlights;
    };
    for code in ranges_within(&embedded_tree.included_ranges(), &visible_bytes) {
        if let Some(code) = char_range(code, &range, buffer) {
            highlights.remove(code);
        }
    }
    let embedded_highlights = embedded_query.get_highlighted_chunks(
        range,
        &embedded.language.highlight_query,
        buffer,
        embedded_tree,
    );
    for (highlight_range, color) in embedded_highlights.iter() {
        highlights.insert(highlight_range.clone(), *color);
    }
    highlights
}

/// Returns the bytes of the syntax tree that `range` covers.
fn visible_byte_range(range: &Range<CharOffset>, buffer: &Buffer) -> Range<usize> {
    tree_byte(range.start, buffer)..tree_byte(range.end, buffer)
}

/// Returns the parts of `ranges`, which are in document order, that fall inside `visible`.
fn ranges_within(ranges: &[ParserRange], visible: &Range<usize>) -> Vec<Range<usize>> {
    let first = ranges.partition_point(|range| range.end_byte <= visible.start);
    ranges[first..]
        .iter()
        .take_while(|range| range.start_byte < visible.end)
        .map(|range| range.start_byte.max(visible.start)..range.end_byte.min(visible.end))
        .collect()
}

/// Returns the parts of `visible` that none of `ranges`, which are in document order, cover.
/// For the ranges a template's host language parses, these are the template's tags.
fn gaps_between(ranges: &[ParserRange], visible: &Range<usize>) -> Vec<Range<usize>> {
    let mut gaps = Vec::new();
    let mut gap_start = visible.start;
    let first = ranges.partition_point(|range| range.end_byte <= visible.start);
    for range in &ranges[first..] {
        if range.start_byte >= visible.end {
            break;
        }
        if gap_start < range.start_byte {
            gaps.push(gap_start..range.start_byte);
        }
        gap_start = gap_start.max(range.end_byte);
    }
    if gap_start < visible.end {
        gaps.push(gap_start..visible.end);
    }
    gaps
}

/// Converts a byte range of the syntax tree into the characters of `visible` that it covers.
fn char_range(
    bytes: Range<usize>,
    visible: &Range<CharOffset>,
    buffer: &Buffer,
) -> Option<Range<CharOffset>> {
    let start = char_offset(bytes.start, buffer).max(visible.start);
    let end = char_offset(bytes.end, buffer).min(visible.end);
    (start < end).then_some(start..end)
}

/// Converts a byte offset of the syntax tree into the character offset that highlights use.
/// Both leave out the block marker that opens the buffer, so they are one behind the buffer's
/// own offsets.
fn char_offset(tree_byte: usize, buffer: &Buffer) -> CharOffset {
    let buffer_offset = ByteOffset::from(tree_byte + 1).to_buffer_char_offset(buffer);
    CharOffset::from(buffer_offset.as_usize().saturating_sub(1))
}

/// Converts a character offset that highlights use into a byte offset of the syntax tree. This
/// is the inverse of [`char_offset`].
fn tree_byte(char_offset: CharOffset, buffer: &Buffer) -> usize {
    let buffer_offset = CharOffset::from(char_offset.as_usize() + 1).to_buffer_byte_offset(buffer);
    buffer_offset.as_usize().saturating_sub(1)
}

/// The parts of a template that each of its two languages parses.
struct TemplateRanges {
    /// Everything outside the tags.
    host: Vec<ParserRange>,
    /// The embedded code inside the tags.
    code: Vec<ParserRange>,
}

fn template_ranges(source: &[u8], tags: &[EmbeddedTag]) -> TemplateRanges {
    let mut ranges = TemplateRanges {
        host: Vec::new(),
        code: Vec::new(),
    };
    // Without tags the host language parses the whole document, which is what an empty list of
    // ranges already means.
    if tags.is_empty() {
        return ranges;
    }

    let mut positions = Positions::new(source);
    let mut host_start = 0;
    for tag in tags {
        if host_start < tag.range.start {
            ranges
                .host
                .push(positions.range(host_start..tag.range.start));
        }
        if let Some(code) = &tag.code {
            ranges.code.push(positions.range(code.clone()));
        }
        host_start = tag.range.end;
    }
    // A document that is nothing but tags leaves the host language no text. It still needs a
    // range, because an empty list would hand it the whole document.
    if host_start < source.len() || ranges.host.is_empty() {
        ranges.host.push(positions.range(host_start..source.len()));
    }
    ranges
}

/// Finds the row and column of byte offsets that are asked for in increasing order.
struct Positions<'a> {
    source: &'a [u8],
    offset: usize,
    point: arborium::tree_sitter::Point,
}

impl<'a> Positions<'a> {
    fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            offset: 0,
            point: arborium::tree_sitter::Point { row: 0, column: 0 },
        }
    }

    fn point_at(&mut self, offset: usize) -> arborium::tree_sitter::Point {
        debug_assert!(self.offset <= offset, "offsets should not go backwards");
        for &byte in &self.source[self.offset..offset] {
            if byte == b'\n' {
                self.point.row += 1;
                self.point.column = 0;
            } else {
                self.point.column += 1;
            }
        }
        self.offset = offset;
        self.point
    }

    fn range(&mut self, bytes: Range<usize>) -> ParserRange {
        ParserRange {
            start_byte: bytes.start,
            end_byte: bytes.end,
            start_point: self.point_at(bytes.start),
            end_point: self.point_at(bytes.end),
        }
    }
}

/// Collects the text of `content` with the byte offsets the syntax tree uses.
fn source_bytes(content: &BufferSnapshot) -> Vec<u8> {
    let mut bytes = content.bytes();
    // Skip the block marker at the start of the buffer, as parsing does.
    bytes.seek(ByteOffset::from(1));
    let mut source = Vec::with_capacity(content.byte_len());
    for chunk in bytes {
        source.extend_from_slice(chunk);
    }
    source
}

/// Convert a 1-indexed buffer Point into a 0-indexed tree-sitter Point.
fn point_to_syntax_point(point: Point) -> arborium::tree_sitter::Point {
    // Subtracting 1 from row to convert from 1-indexed buffer rows to 0-indexed tree-sitter rows.
    arborium::tree_sitter::Point {
        row: point.row.saturating_sub(1) as usize,
        column: point.column as usize,
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
