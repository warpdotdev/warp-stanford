use std::sync::Arc;

use languages::{Language, language_by_filename, language_by_name};
use rangemap::RangeMap;
use string_offset::CharOffset;
use warp_editor::content::buffer::Buffer;
use warp_editor::content::selection_model::BufferSelectionModel;
use warp_editor::content::text::IndentBehavior;
use warp_util::standardized_path::StandardizedPath;
use warpui_core::App;
use warpui_core::color::ColorU;

use super::*;
use crate::SyntaxTreeState;

const fn color(id: u8) -> ColorU {
    ColorU {
        r: id,
        g: 0,
        b: 0,
        a: 255,
    }
}

const KEYWORD: ColorU = color(1);
const FUNCTION: ColorU = color(2);
const STRING: ColorU = color(3);
const TYPE: ColorU = color(4);
const NUMBER: ColorU = color(5);
const COMMENT: ColorU = color(6);
const PROPERTY: ColorU = color(7);
const TAG: ColorU = color(8);

const COLOR_MAP: ColorMap = ColorMap {
    keyword_color: KEYWORD,
    function_color: FUNCTION,
    string_color: STRING,
    type_color: TYPE,
    number_color: NUMBER,
    comment_color: COMMENT,
    property_color: PROPERTY,
    tag_color: TAG,
};

fn language_for(filename: &str) -> Arc<Language> {
    let path =
        StandardizedPath::try_new(&format!("/{filename}")).expect("test path should be absolute");
    language_by_filename(&path).expect("test language should be supported")
}

/// The highlight colors of a document, looked up by the text they cover.
struct Highlights {
    text: String,
    colors: RangeMap<CharOffset, ColorU>,
}

impl Highlights {
    /// Returns the color of `part`, which must have one color throughout. `context` is a unique
    /// piece of the document that contains `part`.
    #[track_caller]
    fn color_of(&self, context: &str, part: &str) -> Option<ColorU> {
        let mut matches = self.text.match_indices(context);
        let (context_start, _) = matches
            .next()
            .unwrap_or_else(|| panic!("{context:?} should be in the document"));
        assert!(matches.next().is_none(), "{context:?} should be unique");
        let part_start = context_start
            + context
                .find(part)
                .unwrap_or_else(|| panic!("{part:?} should be in {context:?}"));

        let first_char = self.text[..part_start].chars().count();
        let mut colors = (first_char..first_char + part.chars().count())
            .map(|index| self.colors.get(&CharOffset::from(index)).copied());
        let color = colors.next().expect("part should not be empty");
        assert!(
            colors.all(|other| other == color),
            "{part:?} in {context:?} should have one color"
        );
        color
    }
}

/// Highlights `visible`, a unique piece of `text`, as the language of `filename`, the way the
/// editor highlights only the lines in its viewport, and passes the result to `check`.
fn check_highlights_within(
    filename: &'static str,
    text: &'static str,
    visible: &'static str,
    check: fn(&Highlights),
) {
    App::test((), move |mut app| async move {
        let language = language_for(filename);
        let buffer_handle = app.add_model(|_| Buffer::new(Box::new(|_, _| IndentBehavior::Ignore)));
        let selection = app.add_model(|_| BufferSelectionModel::new(buffer_handle.clone()));
        buffer_handle.update(&mut app, |buffer, ctx| {
            *buffer = Buffer::from_plain_text(
                text,
                None,
                Box::new(|_, _| IndentBehavior::Ignore),
                selection,
                ctx,
            );
        });

        let snapshot = buffer_handle.read(&app, |buffer, _| buffer.buffer_snapshot());
        let tree =
            warpui_core::r#async::block_on(SyntaxTreeState::parse_text(snapshot, None, &language))
                .expect("test buffer is small and should parse");
        let query = HighlightQuery::new(&language.highlight_query, COLOR_MAP);

        buffer_handle.read(&app, |buffer, _| {
            let visible_start = text
                .find(visible)
                .expect("visible should be in the document");
            let first_char = text[..visible_start].chars().count();
            let range = CharOffset::from(first_char)
                ..CharOffset::from(first_char + visible.chars().count());
            let colors =
                query.get_highlighted_chunks(range, &language.highlight_query, buffer, &tree);
            check(&Highlights {
                text: text.to_owned(),
                colors,
            });
        });
    });
}

fn check_highlights(filename: &'static str, text: &'static str, check: fn(&Highlights)) {
    check_highlights_within(filename, text, text, check);
}

#[test]
fn a_comment_after_non_ascii_text_keeps_its_color() {
    check_highlights("notes.rb", "x = 1 # café\ny = 2 # ok\n", |highlights| {
        assert_eq!(highlights.color_of("# café", "# café"), Some(COMMENT));
        assert_eq!(highlights.color_of("y = 2", "2"), Some(NUMBER));
        assert_eq!(highlights.color_of("# ok", "# ok"), Some(COMMENT));
    });
}

#[test]
fn tokens_after_a_non_ascii_string_keep_their_colors() {
    check_highlights(
        "notes.rb",
        "s = \"naïve ☕\"; n = 42 # done\n",
        |highlights| {
            assert_eq!(
                highlights.color_of("\"naïve ☕\"", "\"naïve ☕\""),
                Some(STRING)
            );
            assert_eq!(highlights.color_of("= 42", " "), None);
            assert_eq!(highlights.color_of("n = 42", "42"), Some(NUMBER));
            assert_eq!(highlights.color_of("42 #", " "), None);
            assert_eq!(highlights.color_of("# done", "# done"), Some(COMMENT));
        },
    );
}

#[test]
fn non_ascii_text_between_html_tags_is_not_colored() {
    check_highlights("page.html", "<p>✓</p>\n", |highlights| {
        assert_eq!(highlights.color_of("<p>", "p"), Some(TAG));
        assert_eq!(highlights.color_of("✓", "✓"), None);
        assert_eq!(highlights.color_of("</p>", "p"), Some(TAG));
    });
}

#[test]
fn ascii_highlights_are_unchanged() {
    check_highlights(
        "notes.rb",
        "x = 1 # cafe\ndef greet\n  \"hi\"\nend\n",
        |highlights| {
            assert_eq!(highlights.color_of("1 #", "1"), Some(NUMBER));
            assert_eq!(highlights.color_of("# cafe", "# cafe"), Some(COMMENT));
            assert_eq!(highlights.color_of("def greet", "def"), Some(KEYWORD));
            assert_eq!(highlights.color_of("\"hi\"", "\"hi\""), Some(STRING));
        },
    );
}

/// Rust's highlight query colors a path segment as a type only when its text starts with an
/// uppercase letter, so this depends on the query reading the right bytes for each node.
#[test]
fn query_predicates_read_the_text_of_their_own_node() {
    check_highlights(
        "main.rs",
        "fn main() { let v = Vec::new(); let s = std::mem::take(&mut v); }\n",
        |highlights| {
            assert_eq!(highlights.color_of("Vec::new", "Vec"), Some(TYPE));
            assert_eq!(highlights.color_of("= std::mem", "std"), None);
        },
    );
}

#[test]
fn highlighting_part_of_a_document_stops_at_the_end_of_that_part() {
    check_highlights_within(
        "notes.rb",
        "a = 1\nb = 2\nc = 3\n",
        "b = 2\n",
        |highlights| {
            assert_eq!(highlights.color_of("b = 2", "2"), Some(NUMBER));
            assert_eq!(highlights.color_of("a = 1", "a = 1"), None);
            assert_eq!(highlights.color_of("c = 3", "c = 3"), None);
        },
    );
}

/// The comment ends exactly where the highlighted part starts, right after a non-ASCII character.
#[test]
fn highlighting_part_of_a_document_starts_at_the_start_of_that_part() {
    check_highlights_within(
        "notes.rb",
        "a = 1 # é\nb = 2 # ok\n",
        "\nb = 2 # ok\n",
        |highlights| {
            assert_eq!(highlights.color_of("# é", "# é"), None);
            assert_eq!(highlights.color_of("b = 2", "2"), Some(NUMBER));
            assert_eq!(highlights.color_of("# ok", "# ok"), Some(COMMENT));
        },
    );
}

/// Lua, Kotlin and Scala tag these keywords with legacy capture names rather than `keyword.*`.
#[test]
fn legacy_keyword_captures_use_keyword_color() {
    App::test((), |mut app| async move {
        let keyword_color = ColorU::white();
        let other_color = ColorU::black();
        let color_map = ColorMap {
            keyword_color,
            function_color: other_color,
            string_color: other_color,
            type_color: other_color,
            number_color: other_color,
            comment_color: other_color,
            property_color: other_color,
            tag_color: other_color,
        };

        for (language_name, source, keywords) in [
            (
                "lua",
                "if x then\n  while y do end\nend",
                &["if", "while"][..],
            ),
            (
                "kotlin",
                "fun f() {\n  try { } catch (e: Exception) { }\n}",
                &["try", "catch"][..],
            ),
            ("scala", "import a.b", &["import"][..]),
        ] {
            let language = language_by_name(language_name).expect("language should be supported");

            let buffer_handle =
                app.add_model(|_| Buffer::new(Box::new(|_, _| IndentBehavior::Ignore)));
            let selection = app.add_model(|_| BufferSelectionModel::new(buffer_handle.clone()));
            buffer_handle.update(&mut app, |buffer, ctx| {
                *buffer = Buffer::from_plain_text(
                    source,
                    None,
                    Box::new(|_, _| IndentBehavior::Ignore),
                    selection,
                    ctx,
                );
            });

            let buffer_snapshot = buffer_handle.read(&app, |buffer, _| buffer.buffer_snapshot());
            let tree = warpui_core::r#async::block_on(async {
                SyntaxTreeState::parse_text(buffer_snapshot, None, &language).await
            })
            .expect("test buffer is small and should parse");

            buffer_handle.read(&app, |buffer, _| {
                let query = &language.highlight_query;
                let highlights = HighlightQuery::new(query, color_map).get_highlighted_chunks(
                    CharOffset::zero()..buffer.max_charoffset(),
                    query,
                    buffer,
                    &tree,
                );

                for keyword in keywords {
                    let offset = source.find(keyword).expect("keyword should be in source");
                    assert_eq!(
                        highlights.get(&CharOffset::from(offset)),
                        Some(&keyword_color),
                        "`{keyword}` should be highlighted as a keyword in {language_name}"
                    );
                }
            });
        }
    });
}
