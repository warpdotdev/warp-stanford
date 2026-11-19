use std::sync::Arc;

use arborium::tree_sitter::{InputEdit, Point as ParserPoint};
use languages::{EmbeddedTag, Language, language_by_filename};
use rangemap::RangeMap;
use string_offset::CharOffset;
use warp_editor::content::buffer::{Buffer, BufferSnapshot};
use warp_editor::content::selection_model::BufferSelectionModel;
use warp_editor::content::text::IndentBehavior;
use warp_util::standardized_path::StandardizedPath;
use warpui_core::App;
use warpui_core::color::ColorU;

use super::*;

const KEYWORD: ColorU = ColorU {
    r: 1,
    g: 0,
    b: 0,
    a: 255,
};
const FUNCTION: ColorU = ColorU {
    r: 2,
    g: 0,
    b: 0,
    a: 255,
};
const STRING: ColorU = ColorU {
    r: 3,
    g: 0,
    b: 0,
    a: 255,
};
const TYPE: ColorU = ColorU {
    r: 4,
    g: 0,
    b: 0,
    a: 255,
};
const NUMBER: ColorU = ColorU {
    r: 5,
    g: 0,
    b: 0,
    a: 255,
};
const COMMENT: ColorU = ColorU {
    r: 6,
    g: 0,
    b: 0,
    a: 255,
};
const PROPERTY: ColorU = ColorU {
    r: 7,
    g: 0,
    b: 0,
    a: 255,
};
const TAG: ColorU = ColorU {
    r: 8,
    g: 0,
    b: 0,
    a: 255,
};

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

/// A view that exercises every kind of ERB tag next to the HTML around it.
const VIEW: &str = r#"<%# Greets the signed-in user %>
<div class="card <%= card_class %>">
  <% if @user.admin? %>
    <p>Hello, <%= t("greeting") %>! You have <%= 1 + 2 %> alerts.</p>
  <% else %>
    <a href="<%= login_path %>">if you sign in, then it will end</a>
  <% end -%>
</div>
"#;

fn language_for(filename: &str) -> Arc<Language> {
    let path =
        StandardizedPath::try_new(&format!("/{filename}")).expect("test path should be absolute");
    language_by_filename(&path).expect("test language should be supported")
}

fn parse(text: &str, old_trees: Option<SyntaxTrees>, language: &Language) -> SyntaxTrees {
    let snapshot = BufferSnapshot::from_plain_text(text);
    warpui_core::r#async::block_on(SyntaxTreeState::parse_trees(snapshot, old_trees, language))
        .expect("test buffer is small and should parse")
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

/// Highlights `text` as the language of `filename` and passes the result to `check`.
fn check_highlights(filename: &'static str, text: &'static str, check: fn(&Highlights)) {
    check_highlights_within(filename, text, text, check);
}

/// Highlights only `visible`, a unique piece of `text`, the way the editor highlights only the
/// lines in its viewport.
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
        let trees =
            warpui_core::r#async::block_on(SyntaxTreeState::parse_trees(snapshot, None, &language))
                .expect("test buffer is small and should parse");
        let queries = LanguageQueries::new(language, COLOR_MAP);

        buffer_handle.read(&app, |buffer, _| {
            let visible_start = text
                .find(visible)
                .expect("visible should be in the document");
            let first_char = text[..visible_start].chars().count();
            let range = CharOffset::from(first_char)
                ..CharOffset::from(first_char + visible.chars().count());
            let colors = highlighted_chunks(range.clone(), &queries, &COLOR_MAP, buffer, &trees);
            assert!(
                colors
                    .iter()
                    .all(|(colored, _)| range.start <= colored.start && colored.end <= range.end),
                "highlights should stay inside the requested range"
            );
            check(&Highlights {
                text: text.to_owned(),
                colors,
            });
        });
    });
}

#[test]
fn erb_colors_markup_as_html_and_tags_as_ruby() {
    check_highlights("show.html.erb", VIEW, |highlights| {
        assert_eq!(highlights.color_of("<div class", "div"), Some(TAG));
        assert_eq!(highlights.color_of("</div>", "div"), Some(TAG));
        assert_eq!(highlights.color_of("<a href", "a"), Some(TAG));
        assert_eq!(highlights.color_of("\"card ", "card"), Some(STRING));
        assert_eq!(highlights.color_of("Hello, ", "Hello,"), None);
        assert_eq!(highlights.color_of("<% if @user", "if"), Some(KEYWORD));
        assert_eq!(highlights.color_of("<% if @user", "@user"), Some(PROPERTY));
        assert_eq!(highlights.color_of("<% else %>", "else"), Some(KEYWORD));
        assert_eq!(highlights.color_of("<% end -%>", "end"), Some(KEYWORD));
        assert_eq!(
            highlights.color_of("t(\"greeting\")", "\"greeting\""),
            Some(STRING)
        );
        assert_eq!(highlights.color_of("<%= 1 + 2 %>", "1"), Some(NUMBER));
        assert_eq!(highlights.color_of("<%= 1 + 2 %>", "2"), Some(NUMBER));
    });
}

#[test]
fn erb_delimiters_and_comment_tags_take_the_comment_color() {
    check_highlights("show.html.erb", VIEW, |highlights| {
        assert_eq!(
            highlights.color_of(
                "<%# Greets the signed-in user %>",
                "<%# Greets the signed-in user %>"
            ),
            Some(COMMENT)
        );
        assert_eq!(highlights.color_of("<% if @user", "<%"), Some(COMMENT));
        assert_eq!(highlights.color_of("admin? %>", "%>"), Some(COMMENT));
        assert_eq!(highlights.color_of("<%= 1 + 2 %>", "<%="), Some(COMMENT));
        assert_eq!(highlights.color_of("<% end -%>", "-%>"), Some(COMMENT));
    });
}

#[test]
fn each_language_stays_inside_its_own_part_of_an_erb_template() {
    check_highlights("show.html.erb", VIEW, |highlights| {
        assert_eq!(
            highlights.color_of(
                "if you sign in, then it will end",
                "if you sign in, then it will end"
            ),
            None
        );
        assert_eq!(highlights.color_of("<%= 1 + 2 %>", " + "), None);
        assert_eq!(
            highlights.color_of("<%= card_class %>", "card_class"),
            Some(FUNCTION)
        );
        assert_eq!(
            highlights.color_of("<%= login_path %>", "login_path"),
            Some(FUNCTION)
        );
        assert_eq!(
            highlights.color_of("<%= card_class %>", "<%="),
            Some(COMMENT)
        );
        assert_eq!(highlights.color_of("\">\n  <% if", ">"), Some(COMMENT));
    });
}

/// The requested range starts and ends inside a tag delimiter, so its first and last characters
/// only get their color if the range is mapped onto the trees exactly.
#[test]
fn erb_highlights_a_range_that_cuts_through_tags() {
    check_highlights_within(
        "show.html.erb",
        VIEW,
        "% if @user.admin? %>\n    <p>Hello, <%= t(\"greeting\") %>! You have <",
        |highlights| {
            assert_eq!(highlights.color_of("% if @user", "%"), Some(COMMENT));
            assert_eq!(highlights.color_of("% if @user", "if"), Some(KEYWORD));
            assert_eq!(highlights.color_of("admin? %>", "%>"), Some(COMMENT));
            assert_eq!(highlights.color_of("<p>Hello", "p"), Some(TAG));
            assert_eq!(
                highlights.color_of("t(\"greeting\")", "\"greeting\""),
                Some(STRING)
            );
            assert_eq!(highlights.color_of("You have <", "<"), Some(COMMENT));
            assert_eq!(highlights.color_of("<div class", "div"), None);
            assert_eq!(highlights.color_of("<%= 1 + 2 %>", "1"), None);
        },
    );
}

#[test]
fn erb_highlights_line_up_after_multibyte_text() {
    check_highlights(
        "menu.html.erb",
        "<p>café ☕<%= \"naïve\" if open %>✓ ok</p>\n",
        |highlights| {
            assert_eq!(highlights.color_of("<p>café", "p"), Some(TAG));
            assert_eq!(highlights.color_of("café ☕", "café ☕"), None);
            assert_eq!(highlights.color_of("☕<%= \"", "<%="), Some(COMMENT));
            assert_eq!(highlights.color_of("\"naïve\"", "\"naïve\""), Some(STRING));
            assert_eq!(highlights.color_of(" if open", "if"), Some(KEYWORD));
            assert_eq!(highlights.color_of("open %>✓", "%>"), Some(COMMENT));
            assert_eq!(highlights.color_of("✓ ok", "✓ ok"), None);
            assert_eq!(highlights.color_of("ok</p>", "p"), Some(TAG));
        },
    );
}

#[test]
fn an_erb_template_without_ruby_is_highlighted_as_html() {
    check_highlights(
        "static.html.erb",
        "<p class=\"note\">if 50% is left then end</p>\n",
        |highlights| {
            assert_eq!(highlights.color_of("<p class", "p"), Some(TAG));
            assert_eq!(highlights.color_of("\"note\"", "note"), Some(STRING));
            assert_eq!(
                highlights.color_of("if 50% is left then end", "if 50% is left then end"),
                None
            );
        },
    );
}

#[test]
fn an_erb_template_of_only_tags_is_highlighted_as_ruby() {
    check_highlights(
        "only_tags.erb",
        "<%# setup %><% total = 1 %><%= total %>",
        |highlights| {
            assert_eq!(
                highlights.color_of("<%# setup %>", "<%# setup %>"),
                Some(COMMENT)
            );
            assert_eq!(highlights.color_of("<% total = 1 %>", "1"), Some(NUMBER));
            assert_eq!(highlights.color_of("<% total = 1 %>", " = "), None);
            assert_eq!(highlights.color_of("1 %><%= total", "%><%="), Some(COMMENT));
        },
    );
}

#[test]
fn an_unclosed_erb_tag_is_ruby_to_the_end_of_the_document() {
    check_highlights("typing.html.erb", "<p>Total</p>\n<%= 1 + 2", |highlights| {
        assert_eq!(highlights.color_of("<p>Total", "p"), Some(TAG));
        assert_eq!(highlights.color_of("<%= 1 + 2", "<%="), Some(COMMENT));
        assert_eq!(highlights.color_of("<%= 1 + 2", "2"), Some(NUMBER));
    });
}

#[test]
fn ruby_files_are_highlighted_as_before() {
    check_highlights(
        "user.rb",
        "if @user\n  puts \"<%= not erb %>\"\nend\n",
        |highlights| {
            assert_eq!(highlights.color_of("if @user", "if"), Some(KEYWORD));
            assert_eq!(highlights.color_of("if @user", "@user"), Some(PROPERTY));
            assert_eq!(
                highlights.color_of("\"<%= not erb %>\"", "\"<%= not erb %>\""),
                Some(STRING)
            );
        },
    );
}

#[test]
fn erb_parses_markup_and_ruby_as_separate_trees() {
    let erb = language_for("show.html.erb");
    let trees = parse(VIEW, None, &erb);

    assert_eq!(trees.host.root_node().kind(), "document");
    assert!(
        !trees.host.root_node().has_error(),
        "the HTML parser should never see a tag: {}",
        trees.host.root_node().to_sexp()
    );
    let embedded = trees.embedded.expect("the view has Ruby in it");
    assert_eq!(embedded.root_node().kind(), "program");

    let tags = (erb.embedded.as_ref().expect("erb embeds ruby").find_tags)(VIEW.as_bytes());
    let code: Vec<_> = tags.iter().filter_map(|tag| tag.code.clone()).collect();
    let parsed: Vec<_> = embedded
        .included_ranges()
        .iter()
        .map(|range| range.start_byte..range.end_byte)
        .collect();
    assert_eq!(parsed, code);
    assert_eq!(&VIEW[code[0].clone()], " card_class ");
}

#[test]
fn only_template_languages_get_an_embedded_tree() {
    for filename in ["user.rb", "index.html"] {
        let trees = parse(VIEW, None, &language_for(filename));
        assert!(trees.embedded.is_none(), "{filename} embeds nothing");
    }
    let erb = language_for("static.html.erb");
    let trees = parse("<p>No Ruby here</p><%# only a comment %>", None, &erb);
    assert!(trees.embedded.is_none());
}

/// The parser is shared between parses, so the ranges of a template must not carry over to the
/// next document.
#[test]
fn the_ranges_of_a_template_do_not_leak_into_the_next_parse() {
    parse(VIEW, None, &language_for("show.html.erb"));

    let html = "<p>one</p><p>two</p>";
    let trees = parse(html, None, &language_for("index.html"));
    let root = trees.host.root_node();
    assert_eq!(root.byte_range(), 0..html.len());
    assert_eq!(root.named_child_count(), 2);

    let ruby = "a = 1\nb = 2\n";
    let trees = parse(ruby, None, &language_for("user.rb"));
    assert_eq!(trees.host.root_node().byte_range(), 0..ruby.len());
    assert!(!trees.host.root_node().has_error());
}

#[test]
fn reparsing_an_edited_template_matches_a_fresh_parse() {
    let erb = language_for("show.html.erb");
    let before = "<ul>\n  <li><%= first %></li>\n</ul>\n";
    let inserted = "<% if show %>\n  <li>new</li>\n  <% end %>\n  ";
    let at = before.find("<li>").expect("the list has an item");
    let after = format!("{}{inserted}{}", &before[..at], &before[at..]);

    let mut trees = parse(before, None, &erb);
    trees.edit(&InputEdit {
        start_byte: at,
        old_end_byte: at,
        new_end_byte: at + inserted.len(),
        start_position: ParserPoint { row: 1, column: 2 },
        old_end_position: ParserPoint { row: 1, column: 2 },
        new_end_position: ParserPoint { row: 4, column: 2 },
    });
    let reparsed = parse(&after, Some(trees), &erb);
    let fresh = parse(&after, None, &erb);

    assert_eq!(
        reparsed.host.root_node().to_sexp(),
        fresh.host.root_node().to_sexp()
    );
    assert_eq!(
        reparsed.host.included_ranges(),
        fresh.host.included_ranges()
    );
    let (reparsed, fresh) = (
        reparsed.embedded.expect("the edit added Ruby"),
        fresh.embedded.expect("the document has Ruby"),
    );
    assert_eq!(reparsed.root_node().to_sexp(), fresh.root_node().to_sexp());
    assert_eq!(reparsed.included_ranges(), fresh.included_ranges());
    assert_eq!(fresh.included_ranges().len(), 3);
}

#[test]
fn template_ranges_split_a_document_at_its_tags() {
    let source = "a\n<%= b %>\ncd<% e %>";
    let tags = [
        EmbeddedTag {
            range: 2..10,
            code: Some(5..8),
        },
        EmbeddedTag {
            range: 13..20,
            code: Some(15..18),
        },
    ];
    let ranges = template_ranges(source.as_bytes(), &tags);

    let bytes = |ranges: &[ParserRange]| -> Vec<_> {
        ranges
            .iter()
            .map(|range| &source[range.start_byte..range.end_byte])
            .collect()
    };
    assert_eq!(bytes(&ranges.host), vec!["a\n", "\ncd"]);
    assert_eq!(bytes(&ranges.code), vec![" b ", " e "]);

    let points = |range: &ParserRange| {
        (
            (range.start_point.row, range.start_point.column),
            (range.end_point.row, range.end_point.column),
        )
    };
    assert_eq!(points(&ranges.host[0]), ((0, 0), (1, 0)));
    assert_eq!(points(&ranges.code[0]), ((1, 3), (1, 6)));
    assert_eq!(points(&ranges.host[1]), ((1, 8), (2, 2)));
    assert_eq!(points(&ranges.code[1]), ((2, 4), (2, 7)));
}

#[test]
fn template_ranges_leave_the_host_no_text_in_a_document_of_only_tags() {
    let source = "<% a %>";
    let tags = [EmbeddedTag {
        range: 0..7,
        code: Some(2..5),
    }];
    let ranges = template_ranges(source.as_bytes(), &tags);
    assert_eq!(ranges.host.len(), 1);
    assert_eq!(
        (ranges.host[0].start_byte, ranges.host[0].end_byte),
        (source.len(), source.len())
    );

    let ranges = template_ranges(b"<p>plain</p>", &[]);
    assert!(ranges.host.is_empty() && ranges.code.is_empty());
}
