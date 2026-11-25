use languages::language_by_name;
use warp_editor::content::buffer::Buffer;
use warp_editor::content::selection_model::BufferSelectionModel;
use warp_editor::content::text::IndentBehavior;
use warpui_core::App;

use super::*;
use crate::SyntaxTreeState;

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
