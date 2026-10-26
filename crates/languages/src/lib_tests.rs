use std::path::Path;

use arborium::tree_sitter::{Parser, QueryCursor, StreamingIterator};
use warp_util::standardized_path::StandardizedPath;

use crate::{SUPPORTED_LANGUAGES, language_by_filename, language_by_local_filename, load_language};

/// Validate that every supported language can be loaded successfully.
/// This catches invalid node types, syntax errors, and other issues in .scm query files
/// (highlights, indents, identifiers) that would otherwise only surface at runtime.
#[test]
fn all_supported_languages_load_successfully() {
    let failures: Vec<_> = SUPPORTED_LANGUAGES
        .iter()
        .filter(|lang| load_language(lang).is_none())
        .collect();

    assert!(
        failures.is_empty(),
        "The following languages failed to load:\n{}",
        failures
            .iter()
            .map(|lang| format!("  - {lang}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Both `.html` and the legacy three-character `.htm` extension should resolve to
/// the same HTML language entry. `.htm` is widely produced by static-site generators
/// and historical web tooling (DOS 8.3 filename limits) and is already treated as
/// an HTML/text file elsewhere in the codebase
/// (see `is_development_text_extension` in `crates/warp_util/src/file_type.rs`).
#[test]
fn html_extensions_resolve_to_html() {
    for filename in ["index.html", "index.htm"] {
        let path = StandardizedPath::try_new(&format!("/tmp/{filename}"))
            .expect("test path should be absolute");
        let language = language_by_filename(&path)
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(
            language.display_name(),
            "HTML",
            "{filename} should resolve to HTML",
        );
    }
}

#[test]
fn local_html_extensions_resolve_to_html() {
    for filename in ["index.html", "index.htm"] {
        let path = Path::new(filename);
        let language = language_by_local_filename(path)
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(
            language.display_name(),
            "HTML",
            "{filename} should resolve to HTML",
        );
    }
}

/// `.command` is the macOS convention for double-clickable shell scripts.
/// Make sure `language_by_filename` recognizes it as shell so the editor
/// renders syntax highlighting instead of the
/// "Language support is unavailable for this file type" footer.
#[test]
fn command_extension_resolves_to_shell() {
    let path =
        StandardizedPath::try_new("/tmp/script.command").expect("test path should be absolute");
    let language =
        language_by_filename(&path).expect("`.command` files should resolve to a language");
    assert_eq!(language.display_name(), "Shell");
}

#[test]
fn local_command_extension_resolves_to_shell() {
    let language = language_by_local_filename(Path::new("script.command"))
        .expect("`.command` files should resolve to a language");
    assert_eq!(language.display_name(), "Shell");
}

/// `.md` and `.markdown` should resolve to the Markdown language so the editor applies
/// syntax highlighting to Markdown source files.
#[test]
fn markdown_extensions_resolve_to_markdown() {
    for filename in ["README.md", "notes.markdown"] {
        let path = StandardizedPath::try_new(&format!("/tmp/{filename}"))
            .expect("test path should be absolute");
        let language = language_by_filename(&path)
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(
            language.display_name(),
            "Markdown",
            "{filename} should resolve to Markdown",
        );
    }
}

/// `.svelte` files (including SvelteKit route files like `+page.svelte`) should resolve
/// to the Svelte language so the editor applies syntax highlighting instead of showing
/// "Language support is unavailable for this file type".
#[test]
fn svelte_extension_resolves_to_svelte() {
    for filename in ["App.svelte", "+page.svelte"] {
        let path = StandardizedPath::try_new(&format!("/tmp/{filename}"))
            .expect("test path should be absolute");
        let language = language_by_filename(&path)
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(language.display_name(), "Svelte");
    }
}

#[test]
fn local_svelte_extension_resolves_to_svelte() {
    let language = language_by_local_filename(Path::new("App.svelte"))
        .expect("`.svelte` files should resolve to a language");
    assert_eq!(language.display_name(), "Svelte");
}

/// The Svelte highlight query should capture Svelte block syntax (`{#if}`, `{#each}`)
/// as keywords and markup as tags, not just parse the file.
#[test]
fn svelte_highlight_query_captures_svelte_syntax() {
    let source = r#"<script>
  let items = [];
</script>

{#if items.length}
  <ul>
    {#each items as item}
      <li class="item">{item}</li>
    {/each}
  </ul>
{:else}
  <p>Empty</p>
{/if}
"#;
    let language = load_language("svelte").expect("svelte should load");
    let mut parser = Parser::new();
    parser
        .set_language(&language.grammar)
        .expect("svelte grammar should be compatible with the parser");
    let tree = parser
        .parse(source, None)
        .expect("svelte source should parse");

    let capture_names = language.highlight_query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(
        &language.highlight_query,
        tree.root_node(),
        source.as_bytes(),
    );
    let mut captured = Vec::new();
    while let Some((query_match, capture_index)) = captures.next() {
        let capture = query_match.captures[*capture_index];
        let text = &source[capture.node.byte_range()];
        captured.push((text.to_string(), capture_names[capture.index as usize]));
    }

    let is_captured_as = |text: &str, prefix: &str| {
        captured
            .iter()
            .any(|(t, name)| t == text && name.split('.').next() == Some(prefix))
    };
    for keyword in ["if", "each", "as", "else"] {
        assert!(
            is_captured_as(keyword, "keyword"),
            "expected `{keyword}` to be captured as a keyword, got {captured:?}"
        );
    }
    for tag in ["ul", "li", "p"] {
        assert!(
            is_captured_as(tag, "tag"),
            "expected `{tag}` to be captured as a tag, got {captured:?}"
        );
    }
}
