use std::path::Path;

use lsp::supported_servers::LSPServerType;
use warpui::App;

use super::{EnablementState, LSPEnablementResultForFile, PersistedWorkspace};

#[test]
fn removing_lsp_registrations_preserves_workspace_metadata_and_other_folders() {
    App::test((), |mut app| async move {
        let model = app.add_model(PersistedWorkspace::new_for_test);
        app.update(|ctx| {
            model.update(ctx, |workspace, ctx| {
                let path = Path::new("/projects/remove");
                let other = Path::new("/projects/keep");
                workspace.enable_lsp_server_for_path(path, LSPServerType::Pyright, ctx);
                workspace.disable_lsp_server_for_path(path, LSPServerType::RustAnalyzer, ctx);
                workspace.enable_lsp_server_for_path(other, LSPServerType::Pyright, ctx);
                let metadata = workspace.workspaces.get(path).unwrap().metadata.clone();

                workspace.remove_lsp_servers_for_path(path, ctx);

                assert_eq!(workspace.all_lsp_servers(path, true).unwrap().count(), 0);
                assert!(matches!(
                    workspace.has_enabled_lsp_server_for_file_path(&path.join("main.py")),
                    LSPEnablementResultForFile::LSPNotEnabled { .. }
                ));
                let preserved = &workspace.workspaces.get(path).unwrap().metadata;
                assert_eq!(preserved.path, metadata.path);
                assert_eq!(preserved.modified_ts, metadata.modified_ts);
                assert_eq!(preserved.navigated_ts, metadata.navigated_ts);
                assert_eq!(preserved.queried_ts, metadata.queried_ts);
                assert_eq!(
                    workspace
                        .all_lsp_servers(other, false)
                        .unwrap()
                        .collect::<Vec<_>>(),
                    vec![(LSPServerType::Pyright, EnablementState::Yes)]
                );

                workspace.remove_lsp_servers_for_path(path, ctx);
                workspace.enable_lsp_server_for_path(path, LSPServerType::Pyright, ctx);
                assert!(matches!(
                    workspace.has_enabled_lsp_server_for_file_path(&path.join("main.py")),
                    LSPEnablementResultForFile::Enabled
                ));
            });
        });
    });
}

#[test]
fn removing_unregistered_child_does_not_remove_parent_language_servers() {
    App::test((), |mut app| async move {
        let model = app.add_model(PersistedWorkspace::new_for_test);
        app.update(|ctx| {
            model.update(ctx, |workspace, ctx| {
                let parent = Path::new("/projects/parent");
                workspace.enable_lsp_server_for_path(parent, LSPServerType::Pyright, ctx);

                workspace.remove_lsp_servers_for_path(&parent.join("child"), ctx);

                assert_eq!(
                    workspace
                        .enabled_lsp_servers(parent)
                        .unwrap()
                        .collect::<Vec<_>>(),
                    vec![LSPServerType::Pyright]
                );
            });
        });
    });
}

#[cfg(feature = "local_fs")]
pub(super) mod installation {
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;

    use futures::FutureExt;
    use futures::channel::oneshot;
    use futures::future::{BoxFuture, pending};
    use lsp::LspManagerModel;
    use lsp::supported_servers::LSPServerType;
    use warpui::r#async::SpawnedFutureHandle;
    use warpui::{App, ModelHandle, SingletonEntity};

    use super::super::{LspRepoStatus, LspTask, PersistedWorkspace};
    use crate::server::server_api::ServerApiProvider;
    use crate::terminal::local_shell::LocalShellState;

    // Control external I/O while exercising the real task dispatch and completion callbacks.
    #[derive(Default)]
    pub(crate) struct LspTestIo {
        pub path: Option<BoxFuture<'static, Option<String>>>,
        pub installation: Option<BoxFuture<'static, anyhow::Result<()>>>,
        pub path_task: Option<SpawnedFutureHandle>,
        pub installation_task: Option<SpawnedFutureHandle>,
    }

    fn workspace_for_installation(app: &mut App) -> ModelHandle<PersistedWorkspace> {
        app.add_singleton_model(|_| LocalShellState::NotLoaded);
        app.add_singleton_model(|_| ServerApiProvider::new_for_test());
        app.add_singleton_model(|_| LspManagerModel::new());
        app.add_model(PersistedWorkspace::new_for_test)
    }

    #[test]
    fn removal_during_path_lookup_prevents_installation() {
        App::test((), |mut app| async move {
            let model = workspace_for_installation(&mut app);
            let path = Path::new("/projects/remove");
            let (path_ready, path_result) = oneshot::channel();
            let events = Rc::new(RefCell::new(Vec::new()));
            app.update(|ctx| {
                let events = Rc::clone(&events);
                ctx.subscribe_to_model(&model, move |_, event, _| {
                    events.borrow_mut().push(event.clone());
                });
            });
            let path_task = app.update(|ctx| {
                model.update(ctx, |workspace, ctx| {
                    workspace.disable_lsp_server_for_path(path, LSPServerType::Pyright, ctx);
                    workspace.lsp_test_io.path =
                        Some(async move { path_result.await.unwrap() }.boxed());
                    workspace.lsp_test_io.installation = Some(pending().boxed());
                    workspace.execute_lsp_task(
                        LspTask::Install {
                            file_path: path.join("main.py"),
                            repo_root: path.to_path_buf(),
                            server_type: LSPServerType::Pyright,
                        },
                        ctx,
                    );
                    workspace.remove_lsp_servers_for_path(path, ctx);
                    ctx.await_spawned_future(
                        workspace.lsp_test_io.path_task.take().unwrap().future_id(),
                    )
                })
            });

            path_ready.send(None).unwrap();
            path_task.await;

            app.update(|ctx| {
                assert_eq!(
                    model
                        .as_ref(ctx)
                        .all_lsp_servers(path, true)
                        .unwrap()
                        .count(),
                    0
                );
                assert!(
                    LspManagerModel::as_ref(ctx)
                        .servers_for_workspace(path)
                        .is_none()
                );
            });
            assert!(
                events.borrow().is_empty(),
                "a removed folder must not start an installation"
            );
        });
    }

    #[test]
    fn removal_during_installation_prevents_reenabling_the_folder() {
        App::test((), |mut app| async move {
            let model = workspace_for_installation(&mut app);
            let path = Path::new("/projects/remove");
            let (installation_ready, installation_result) = oneshot::channel();
            let path_task = app.update(|ctx| {
                model.update(ctx, |workspace, ctx| {
                    workspace.disable_lsp_server_for_path(path, LSPServerType::Pyright, ctx);
                    workspace.lsp_test_io.installation =
                        Some(async move { installation_result.await.unwrap() }.boxed());
                    workspace.execute_lsp_task(
                        LspTask::Install {
                            file_path: path.join("main.py"),
                            repo_root: path.to_path_buf(),
                            server_type: LSPServerType::Pyright,
                        },
                        ctx,
                    );
                    ctx.await_spawned_future(
                        workspace.lsp_test_io.path_task.take().unwrap().future_id(),
                    )
                })
            });
            path_task.await;
            let installation_task = app.update(|ctx| {
                model.update(ctx, |workspace, ctx| {
                    assert_eq!(
                        workspace.detect_lsp_workspace_status(
                            path.to_path_buf(),
                            LSPServerType::Pyright,
                            ctx
                        ),
                        LspRepoStatus::Installing {
                            server_type: LSPServerType::Pyright
                        }
                    );
                    workspace.remove_lsp_servers_for_path(path, ctx);
                    // Keep a regressed spawn from starting a real language-server process.
                    workspace.lsp_test_io.path = Some(pending().boxed());
                    ctx.await_spawned_future(
                        workspace
                            .lsp_test_io
                            .installation_task
                            .take()
                            .unwrap()
                            .future_id(),
                    )
                })
            });

            installation_ready.send(Ok(())).unwrap();
            installation_task.await;

            app.update(|ctx| {
                model.update(ctx, |workspace, ctx| {
                    assert_eq!(workspace.all_lsp_servers(path, true).unwrap().count(), 0);
                    assert_eq!(
                        workspace.detect_lsp_workspace_status(
                            path.to_path_buf(),
                            LSPServerType::Pyright,
                            ctx
                        ),
                        LspRepoStatus::DisabledAndInstalled {
                            server_type: LSPServerType::Pyright
                        }
                    );
                    assert!(
                        workspace.lsp_test_io.path_task.is_none(),
                        "stale completion must not schedule a server spawn"
                    );
                });
                assert!(
                    LspManagerModel::as_ref(ctx)
                        .servers_for_workspace(path)
                        .is_none()
                );
            });
        });
    }

    #[test]
    fn a_new_installation_after_removal_can_reenable_the_folder() {
        App::test((), |mut app| async move {
            let model = workspace_for_installation(&mut app);
            let path = Path::new("/projects/remove");
            let (installation_ready, installation_result) = oneshot::channel();
            let path_task = app.update(|ctx| {
                model.update(ctx, |workspace, ctx| {
                    workspace.disable_lsp_server_for_path(path, LSPServerType::Pyright, ctx);
                    workspace.remove_lsp_servers_for_path(path, ctx);
                    workspace.lsp_test_io.installation =
                        Some(async move { installation_result.await.unwrap() }.boxed());
                    workspace.execute_lsp_task(
                        LspTask::Install {
                            file_path: path.join("main.py"),
                            repo_root: path.to_path_buf(),
                            server_type: LSPServerType::Pyright,
                        },
                        ctx,
                    );
                    ctx.await_spawned_future(
                        workspace.lsp_test_io.path_task.take().unwrap().future_id(),
                    )
                })
            });
            path_task.await;
            let installation_task = app.update(|ctx| {
                model.update(ctx, |workspace, ctx| {
                    workspace.lsp_test_io.path = Some(pending().boxed());
                    ctx.await_spawned_future(
                        workspace
                            .lsp_test_io
                            .installation_task
                            .take()
                            .unwrap()
                            .future_id(),
                    )
                })
            });

            installation_ready.send(Ok(())).unwrap();
            installation_task.await;

            app.update(|ctx| {
                model.update(ctx, |workspace, ctx| {
                    assert_eq!(
                        workspace
                            .enabled_lsp_servers(path)
                            .unwrap()
                            .collect::<Vec<_>>(),
                        vec![LSPServerType::Pyright]
                    );
                    assert_eq!(
                        workspace.detect_lsp_workspace_status(
                            path.to_path_buf(),
                            LSPServerType::Pyright,
                            ctx
                        ),
                        LspRepoStatus::Enabled
                    );
                    workspace
                        .lsp_test_io
                        .path_task
                        .take()
                        .expect("a new installation must schedule a server spawn")
                        .abort();
                });
            });
        });
    }
}
