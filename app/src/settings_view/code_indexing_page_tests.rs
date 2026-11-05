use ai::workspace::WorkspaceMetadata;
use lsp::LspServerConfig;
use remote_server::codebase_index_proto::{RemoteCodebaseIndexState, RemoteCodebaseIndexStatus};
use warpui::App;
use warpui::platform::WindowStyle;

use super::*;
use crate::server::server_api::ServerApiProvider;
use crate::test_util::terminal::initialize_app_for_terminal_view;

fn initialize_indexing_page(app: &mut App) {
    initialize_app_for_terminal_view(app);
    app.add_singleton_model(|_| LspManagerModel::new());
    app.add_singleton_model(|_| ProjectContextModel::default());
    app.add_singleton_model(|_| RemoteCodebaseIndexModel::default());
}

fn render_page_text(page: &CodeIndexingPageView, ctx: &AppContext) -> String {
    let PageType::Monolith { widget, .. } = &page.page else {
        panic!("expected the code indexing settings widget");
    };
    widget
        .render(page, Appearance::as_ref(ctx), ctx)
        .debug_text_content()
        .unwrap_or_default()
}

#[test]
fn lsp_only_folder_can_be_removed_with_its_registered_servers() {
    App::test((), |mut app| async move {
        initialize_indexing_page(&mut app);
        let (_, page) = app.add_window(WindowStyle::NotStealFocus, CodeIndexingPageView::new);
        let path = PathBuf::from("/projects/lsp-only");
        let other = PathBuf::from("/projects/other");
        app.update(|ctx| {
            PersistedWorkspace::handle(ctx).update(ctx, |workspace, ctx| {
                workspace.enable_lsp_server_for_path(&path, LSPServerType::Pyright, ctx);
                workspace.disable_lsp_server_for_path(&path, LSPServerType::RustAnalyzer, ctx);
                workspace.enable_lsp_server_for_path(&other, LSPServerType::Pyright, ctx);
            });
            let http = ServerApiProvider::as_ref(ctx).get_http_client();
            LspManagerModel::handle(ctx).update(ctx, |manager, ctx| {
                manager.register(
                    path.clone(),
                    LspServerConfig::new(
                        LSPServerType::Pyright,
                        path.clone(),
                        None,
                        "test".into(),
                        http.clone(),
                    ),
                    ctx,
                );
                manager.register(
                    path.clone(),
                    LspServerConfig::new(
                        LSPServerType::RustAnalyzer,
                        path.clone(),
                        None,
                        "test".into(),
                        http.clone(),
                    ),
                    ctx,
                );
                manager.register(
                    other.clone(),
                    LspServerConfig::new(
                        LSPServerType::Pyright,
                        other.clone(),
                        None,
                        "test".into(),
                        http,
                    ),
                    ctx,
                );
            });
        });
        app.update(|ctx| {
            let before = render_page_text(page.as_ref(ctx), ctx);
            assert!(before.contains("/projects/lsp-only"), "{before}");
            assert!(before.contains("Remove folder"), "{before}");

            page.update(ctx, |page, ctx| {
                page.handle_action(&CodeIndexingPageAction::RemoveLspFolder(path.clone()), ctx);
            });

            assert!(
                LspManagerModel::as_ref(ctx)
                    .servers_for_workspace(&path)
                    .is_none()
            );
            assert!(LspManagerModel::as_ref(ctx).server_registered(
                &other,
                LSPServerType::Pyright,
                ctx
            ));
            assert_eq!(
                PersistedWorkspace::as_ref(ctx)
                    .all_lsp_servers(&path, true)
                    .unwrap()
                    .count(),
                0
            );
            let after = render_page_text(page.as_ref(ctx), ctx);
            assert!(!after.contains("/projects/lsp-only"), "{after}");
            assert!(after.contains("/projects/other"), "{after}");
        });
    });
}

#[test]
fn suggested_servers_do_not_restore_a_removed_folder_in_settings() {
    App::test((), |mut app| async move {
        initialize_indexing_page(&mut app);
        let path = PathBuf::from("/projects/removed");
        app.update(|ctx| {
            PersistedWorkspace::handle(ctx).update(ctx, |workspace, ctx| {
                // Startup discovery may find suggestions for preserved workspace metadata.
                *workspace = PersistedWorkspace::new(
                    vec![WorkspaceMetadata {
                        path: path.clone(),
                        navigated_ts: None,
                        modified_ts: Some(chrono::DateTime::UNIX_EPOCH),
                        queried_ts: None,
                    }],
                    HashMap::from([(
                        path.clone(),
                        HashMap::from([(LSPServerType::Pyright, EnablementState::Suggested)]),
                    )]),
                    None,
                    ctx,
                );
            });
        });
        let (_, page) = app.add_window(WindowStyle::NotStealFocus, CodeIndexingPageView::new);
        app.update(|ctx| {
            let hidden = render_page_text(page.as_ref(ctx), ctx);
            assert!(!hidden.contains("/projects/removed"), "{hidden}");

            PersistedWorkspace::handle(ctx).update(ctx, |workspace, ctx| {
                workspace.disable_lsp_server_for_path(&path, LSPServerType::Pyright, ctx);
            });
            let disabled = render_page_text(page.as_ref(ctx), ctx);
            assert!(disabled.contains("/projects/removed"), "{disabled}");
            assert!(disabled.contains("Remove folder"), "{disabled}");
        });
    });
}

#[test]
fn removing_language_servers_keeps_an_existing_code_index() {
    App::test((), |mut app| async move {
        initialize_indexing_page(&mut app);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap();
        app.update(|ctx| {
            PersistedWorkspace::handle(ctx).update(ctx, |workspace, ctx| {
                workspace.enable_lsp_server_for_path(&path, LSPServerType::Pyright, ctx);
            });
            CodebaseIndexManager::handle(ctx).update(ctx, |manager, ctx| {
                assert!(manager.index_directory(path.clone(), ctx));
            });
        });
        let (_, page) = app.add_window(WindowStyle::NotStealFocus, CodeIndexingPageView::new);
        app.update(|ctx| {
            let before = render_page_text(page.as_ref(ctx), ctx);
            assert!(!before.contains("Remove folder"), "{before}");

            page.update(ctx, |page, ctx| {
                page.handle_action(&CodeIndexingPageAction::RemoveLspFolder(path.clone()), ctx);
            });

            assert!(
                CodebaseIndexManager::as_ref(ctx)
                    .get_codebase_index_status_for_path(&path, ctx)
                    .is_some()
            );
            let after = render_page_text(page.as_ref(ctx), ctx);
            assert!(after.contains(path.to_str().unwrap()), "{after}");
        });
    });
}

fn remote_status_with_failure(failure_message: Option<&str>) -> RemoteCodebaseIndexStatus {
    RemoteCodebaseIndexStatus {
        repo_path: "/workspaces/repo".to_string(),
        state: RemoteCodebaseIndexState::Unavailable,
        last_updated_epoch_millis: Some(1),
        progress_completed: None,
        progress_total: None,
        failure_message: failure_message.map(ToOwned::to_owned),
        root_hash: None,
    }
}

#[test]
fn remote_index_limit_failure_is_detected_from_status_message() {
    let status = remote_status_with_failure(Some(
        "Cannot index remote codebase because the maximum number of codebase indexes has been reached.",
    ));

    assert!(remote_codebase_index_limit_reached(&status));
}

#[test]
fn other_unavailable_failures_are_not_index_limit_failures() {
    let status = remote_status_with_failure(Some(
        "Cannot index remote codebase because indexing did not start.",
    ));

    assert!(!remote_codebase_index_limit_reached(&status));
}

#[test]
fn current_team_disabling_indexing_uses_generic_tooltip_text() {
    assert_eq!(
        codebase_indexing_disabled_admin_text(Some("Team A"), true),
        INDEXING_DISABLED_ADMIN_TEXT
    );
}

#[test]
fn other_team_disabling_indexing_names_that_team() {
    assert_eq!(
        codebase_indexing_disabled_admin_text(Some("Team A"), false),
        "Codebase indexing is unavailable because Team A has disabled it."
    );
}
