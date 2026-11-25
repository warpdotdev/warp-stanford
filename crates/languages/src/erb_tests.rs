use super::tags;

/// Returns the text of each tag and of the Ruby inside it.
fn tags_in(source: &str) -> Vec<(&str, Option<&str>)> {
    tags(source.as_bytes())
        .into_iter()
        .map(|tag| (&source[tag.range], tag.code.map(|code| &source[code])))
        .collect()
}

#[test]
fn finds_the_ruby_inside_each_kind_of_tag() {
    assert_eq!(
        tags_in("<p><% if user %>Hi <%= user.name %><%- end -%></p><%== raw %>"),
        vec![
            ("<% if user %>", Some(" if user ")),
            ("<%= user.name %>", Some(" user.name ")),
            ("<%- end -%>", Some(" end ")),
            ("<%== raw %>", Some(" raw ")),
        ]
    );
}

#[test]
fn a_comment_tag_has_no_ruby() {
    assert_eq!(
        tags_in("<%# not = code %><%= 1 %>"),
        vec![("<%# not = code %>", None), ("<%= 1 %>", Some(" 1 "))]
    );
}

#[test]
fn a_tag_can_span_lines() {
    assert_eq!(
        tags_in("<%= link_to(\n  name,\n  path) %>"),
        vec![(
            "<%= link_to(\n  name,\n  path) %>",
            Some(" link_to(\n  name,\n  path) ")
        )]
    );
}

#[test]
fn an_unclosed_tag_runs_to_the_end_of_the_document() {
    assert_eq!(
        tags_in("<div>\n<% if user\n</div>"),
        vec![("<% if user\n</div>", Some(" if user\n</div>"))]
    );
    assert_eq!(tags_in("<p><%"), vec![("<%", None)]);
    assert_eq!(tags_in("<p><%="), vec![("<%=", None)]);
}

#[test]
fn escaped_delimiters_neither_open_nor_close_a_tag() {
    assert_eq!(tags_in("<%% not a tag %>"), vec![]);
    assert_eq!(
        tags_in("<%= '%%>' %> text"),
        vec![("<%= '%%>' %>", Some(" '%%>' "))]
    );
}

#[test]
fn tags_without_ruby_have_no_code() {
    assert_eq!(
        tags_in("<%%><%-%><%=%><% %>"),
        vec![("<%-%>", None), ("<%=%>", None), ("<% %>", Some(" "))]
    );
}

#[test]
fn plain_markup_has_no_tags() {
    assert_eq!(tags_in("<p class=\"a\">50% > 40%</p>"), vec![]);
    assert_eq!(tags_in(""), vec![]);
}

#[test]
fn multibyte_text_keeps_byte_offsets_aligned() {
    assert_eq!(
        tags_in("<p>café ☕ <%= \"naïve\" %> ✓</p>"),
        vec![("<%= \"naïve\" %>", Some(" \"naïve\" "))]
    );
}
