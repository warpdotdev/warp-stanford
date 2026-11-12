use std::path::Path;

use arborium::tree_sitter::{Parser, QueryCursor, StreamingIterator};
use warp_util::standardized_path::StandardizedPath;

use crate::{
    SUPPORTED_LANGUAGES, language_by_filename, language_by_local_filename, language_by_name,
    load_language,
};

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

/// `.R` and lowercase `.r` files both resolve to R.
#[test]
fn r_extensions_resolve_to_r() {
    for filename in ["analysis.R", "analysis.r"] {
        let path = StandardizedPath::try_new(&format!("/tmp/{filename}"))
            .expect("test path should be absolute");
        let language = language_by_filename(&path)
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(
            language.display_name(),
            "R",
            "{filename} should resolve to R"
        );
    }
}

#[test]
fn local_r_extensions_resolve_to_r() {
    for filename in ["analysis.R", "analysis.r"] {
        let language = language_by_local_filename(Path::new(filename))
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(
            language.display_name(),
            "R",
            "{filename} should resolve to R"
        );
    }
}

/// Code fences and the agent commonly spell the language `R`, so the uppercase name
/// should resolve to the same entry as `r`.
#[test]
fn uppercase_r_language_name_resolves_to_r() {
    let language = language_by_name("R").expect("`R` should resolve to a language");
    assert_eq!(language.display_name(), "R");
}

/// The bundled R highlight query should capture the pieces of a typical script: the
/// `function` keyword, `if`/`for` control flow, function calls, strings, numbers and
/// comments. This guards against the grammar loading but producing no highlights.
#[test]
fn r_highlight_query_captures_r_syntax() {
    let source = r#"# Summarise a vector
library(dplyr)
total <- function(x) {
  if (length(x) == 0) {
    return(0)
  }
  s <- 0
  for (v in x) {
    s <- s + v
  }
  s
}
total(c(1, 2.5, 3L))
message("done")
"#;
    let language = load_language("r").expect("r should load");
    let mut parser = Parser::new();
    parser
        .set_language(&language.grammar)
        .expect("r grammar should be compatible with the parser");
    let tree = parser.parse(source, None).expect("r source should parse");

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
    for (text, prefix) in [
        ("function", "keyword"),
        ("return", "keyword"),
        ("in", "keyword"),
        ("if", "conditional"),
        ("for", "repeat"),
        ("library", "function"),
        ("total", "function"),
        ("message", "function"),
        ("\"done\"", "string"),
        ("2.5", "number"),
        ("3L", "number"),
        ("# Summarise a vector", "comment"),
    ] {
        assert!(
            is_captured_as(text, prefix),
            "expected `{text}` to be captured as `{prefix}`, got {captured:?}"
        );
    }
}
