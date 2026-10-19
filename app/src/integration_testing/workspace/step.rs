use warpui::integration::TestStep;
use warpui::{SingletonEntity, async_assert, async_assert_eq};

use crate::integration_testing::view_getters::workspace_view;
use crate::undo_close::UndoCloseStack;
use crate::workspace::Workspace;
use crate::workspace::view::left_panel::ToolPanelView;

const TOOLS_PANEL_ACTIVE_VIEW_KEY: &str = "tools_panel_active_view";

/// Records the tools panel's active view so a later step can assert it did not change.
pub fn save_tools_panel_active_view() -> TestStep {
    TestStep::new("Save tools panel active view").with_action(|app, window_id, data| {
        let workspace = workspace_view(app, window_id);
        let active_view =
            workspace.read(app, |workspace, ctx| workspace.left_panel_active_view(ctx));
        data.insert(TOOLS_PANEL_ACTIVE_VIEW_KEY, active_view);
    })
}

/// Asserts the tools panel's active view matches the one recorded by
/// [`save_tools_panel_active_view`].
pub fn assert_tools_panel_active_view_unchanged() -> TestStep {
    TestStep::new("Assert tools panel active view is unchanged")
        .add_named_assertion_with_data_from_prior_step(
            "Tools panel active view is unchanged",
            |app, window_id, data| {
                let saved_view = *data
                    .get::<_, ToolPanelView>(TOOLS_PANEL_ACTIVE_VIEW_KEY)
                    .expect("tools panel active view should have been saved");
                let workspace = workspace_view(app, window_id);
                workspace.read(app, |workspace, ctx| {
                    async_assert_eq!(workspace.left_panel_active_view(ctx), saved_view)
                })
            },
        )
}

/// Mock pressing a button on the Warp-native quit modal. Note that this modal is currently only
/// used on Linux, not macOS.
pub fn press_native_modal_button(button_index: usize) -> TestStep {
    TestStep::new("Press a native modal button")
        .with_action(move |app, _, _data| {
            let active_window = app
                .read(|ctx| ctx.windows().active_window())
                .expect("no active window");
            let workspace = workspace_view(app, active_window);
            app.update(|ctx| {
                assert!(
                    workspace.as_ref(ctx).is_native_quit_modal_open(ctx),
                    "Native modal should be open"
                );
                Workspace::press_native_modal_button(&workspace, button_index, ctx);
            });
        })
        .add_assertion(|app, window_id| {
            let workspace = workspace_view(app, window_id);
            workspace.read(app, |workspace, ctx| {
                async_assert!(
                    !workspace.is_native_quit_modal_open(ctx),
                    "Native modal is still open"
                )
            })
        })
}

/// Trigger undo close (restore closed pane/tab/window) action.
pub fn trigger_undo_close() -> TestStep {
    TestStep::new("Trigger undo close").with_action(move |app, _, _data| {
        app.update(|ctx| {
            UndoCloseStack::handle(ctx).update(ctx, |stack, model_ctx| {
                stack.undo_close(model_ctx);
            });
        });
    })
}
