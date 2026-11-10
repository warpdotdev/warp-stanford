use std::path::Path;
use std::sync::Arc;

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

/// Rails names its templates `<name>.<format>.erb`, so the last extension alone decides.
#[test]
fn erb_extensions_resolve_to_erb() {
    for filename in ["show.html.erb", "partial.erb"] {
        let path = StandardizedPath::try_new(&format!("/tmp/{filename}"))
            .expect("test path should be absolute");
        let language = language_by_filename(&path)
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(language.display_name(), "ERB");

        let language = language_by_local_filename(Path::new(filename))
            .unwrap_or_else(|| panic!("expected {filename} to resolve to a language"));
        assert_eq!(language.display_name(), "ERB");
    }
}

#[test]
fn erb_embeds_ruby_and_plain_ruby_embeds_nothing() {
    let erb = language_by_name("erb").expect("erb should load");
    let embedded = erb.embedded.as_ref().expect("erb should embed a language");
    assert_eq!(embedded.language.display_name(), "Ruby");
    assert!(Arc::ptr_eq(
        &embedded.language,
        &language_by_name("ruby").expect("ruby should load")
    ));

    let tags = (embedded.find_tags)(b"<p><%= name %></p>");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].range, 3..14);
    assert_eq!(tags[0].code, Some(6..12));

    assert!(embedded.language.embedded.is_none());
    assert_eq!(
        language_by_name("eruby").map(|language| language.display_name().to_owned()),
        Some("ERB".to_owned())
    );
}
