use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pathfinder_geometry::vector::vec2f;
use warp_core::features::FeatureFlag;
use warp_core::ui::appearance::Appearance;
use warp_editor::render::element::VerticalExpansionBehavior;
use warp_util::user_input::UserInput;
use warpui::elements::ScrollbarWidth;
use warpui::elements::new_scrollable::ScrollableAppearance;
use warpui::platform::WindowStyle;
use warpui::{
    App, EntityIdSet, Presenter, TypedActionView, ViewHandle, WindowId, WindowInvalidation,
};

use super::{CodeEditorRenderOptions, CodeEditorView, CodeEditorViewAction};
use crate::AuthStateProvider;
use crate::cloud_object::model::persistence::CloudModel;
use crate::editor::InteractionState;
use crate::notebooks::editor::keys::NotebookKeybindings;
use crate::server::server_api::team::MockTeamClient;
use crate::server::server_api::workspace::MockWorkspaceClient;
use crate::settings_view::keybindings::KeybindingChangedNotifier;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::vim_registers::VimRegisters;
use crate::workspace::ActiveSession;
use crate::workspace::sync_inputs::SyncedInputState;
use crate::workspaces::user_workspaces::UserWorkspaces;

fn initialize_editor(app: &mut App) -> (WindowId, ViewHandle<CodeEditorView>) {
    initialize_settings_for_tests(app);

    // Add all required singleton models for EditorView dependencies
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|_| SyncedInputState::mock());
    app.add_singleton_model(|_| VimRegisters::new());
    app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());

    // Add mocks required by rich text editor (used in CommentEditor)
    app.add_singleton_model(CloudModel::mock);
    app.add_singleton_model(|_| ActiveSession::default());
    app.add_singleton_model(NotebookKeybindings::new);

    // Add UserWorkspaces mock (required by EditorView)
    let team_client_mock = Arc::new(MockTeamClient::new());
    let workspace_client_mock = Arc::new(MockWorkspaceClient::new());
    app.add_singleton_model(|ctx| {
        UserWorkspaces::mock(
            team_client_mock.clone(),
            workspace_client_mock.clone(),
            vec![],
            ctx,
        )
    });

    let (window, editor_view) = app.add_window(WindowStyle::NotStealFocus, |ctx| {
        CodeEditorView::new(
            None,
            None,
            CodeEditorRenderOptions::new(VerticalExpansionBehavior::GrowToMaxHeight),
            ctx,
        )
        .with_horizontal_scrollbar_appearance(ScrollableAppearance::new(ScrollbarWidth::Auto, true))
    });

    (window, editor_view)
}

#[test]
fn test_interaction_state_prevents_editing() {
    App::test((), |mut app| async move {
        let (_window, editor_view) = initialize_editor(&mut app);

        let text = editor_view.update(&mut app, |view, ctx| {
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("abc")), ctx);
            view.text(ctx)
        });

        assert_eq!(text.as_str(), "abc");

        // Set to be only selectable
        editor_view.update(&mut app, |view, ctx| {
            view.set_interaction_state(InteractionState::Selectable, ctx);
        });

        let text = editor_view.update(&mut app, |view, ctx| {
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("def")), ctx);
            view.text(ctx)
        });

        assert_eq!(text.as_str(), "abc");
    });
}

fn set_marked_text(editor_view: &ViewHandle<CodeEditorView>, text: &str, app: &mut App) {
    editor_view.update(app, |view, ctx| {
        view.handle_action(
            &CodeEditorViewAction::SetMarkedText(UserInput::new(text)),
            ctx,
        );
    });
}

#[test]
fn ime_marked_text_is_shown_without_editing_the_buffer() {
    App::test((), |mut app| async move {
        let _guard = FeatureFlag::ImeMarkedText.override_enabled(true);
        let (_window, editor_view) = initialize_editor(&mut app);

        set_marked_text(&editor_view, "ni", &mut app);

        editor_view.read(&app, |view, ctx| {
            assert_eq!(view.ime_marked_text.as_deref(), Some("ni"));
            assert_eq!(view.text(ctx).as_str(), "");
        });
    });
}

#[test]
fn committing_ime_composition_inserts_once_and_clears_marked_text() {
    App::test((), |mut app| async move {
        let _guard = FeatureFlag::ImeMarkedText.override_enabled(true);
        let (_window, editor_view) = initialize_editor(&mut app);

        set_marked_text(&editor_view, "n", &mut app);
        set_marked_text(&editor_view, "ni", &mut app);
        editor_view.update(&mut app, |view, ctx| {
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("你")), ctx);
        });

        editor_view.read(&app, |view, ctx| {
            assert_eq!(view.ime_marked_text, None);
            assert_eq!(view.text(ctx).as_str(), "你");
        });

        // The composition never touched the buffer, so a single undo removes the committed text.
        editor_view.update(&mut app, |view, ctx| view.undo(ctx));
        editor_view.read(&app, |view, ctx| assert_eq!(view.text(ctx).as_str(), ""));
    });
}

#[test]
fn clearing_ime_composition_leaves_buffer_untouched() {
    App::test((), |mut app| async move {
        let _guard = FeatureFlag::ImeMarkedText.override_enabled(true);
        let (_window, editor_view) = initialize_editor(&mut app);

        editor_view.update(&mut app, |view, ctx| {
            view.handle_action(&CodeEditorViewAction::UserTyped(UserInput::new("a")), ctx);
        });
        set_marked_text(&editor_view, "ni", &mut app);
        editor_view.update(&mut app, |view, ctx| {
            view.handle_action(&CodeEditorViewAction::ClearMarkedText, ctx);
        });

        editor_view.read(&app, |view, ctx| {
            assert_eq!(view.ime_marked_text, None);
            assert_eq!(view.text(ctx).as_str(), "a");
        });
    });
}

#[test]
fn empty_ime_composition_clears_marked_text() {
    App::test((), |mut app| async move {
        let _guard = FeatureFlag::ImeMarkedText.override_enabled(true);
        let (_window, editor_view) = initialize_editor(&mut app);

        set_marked_text(&editor_view, "ni", &mut app);
        set_marked_text(&editor_view, "", &mut app);

        editor_view.read(&app, |view, _ctx| assert_eq!(view.ime_marked_text, None));
    });
}

#[test]
fn ime_marked_text_is_ignored_when_not_editable_or_flag_disabled() {
    App::test((), |mut app| async move {
        let (_window, editor_view) = initialize_editor(&mut app);

        {
            let _guard = FeatureFlag::ImeMarkedText.override_enabled(false);
            set_marked_text(&editor_view, "ni", &mut app);
            editor_view.read(&app, |view, _ctx| assert_eq!(view.ime_marked_text, None));
        }

        let _guard = FeatureFlag::ImeMarkedText.override_enabled(true);
        editor_view.update(&mut app, |view, ctx| {
            view.set_interaction_state(InteractionState::Selectable, ctx);
        });
        set_marked_text(&editor_view, "ni", &mut app);
        editor_view.read(&app, |view, _ctx| assert_eq!(view.ime_marked_text, None));
    });
}

/// Regression test for the rich-text element dropping IME events before they reach the view.
#[test]
fn platform_ime_events_reach_the_code_editor() {
    App::test((), |mut app| async move {
        let _guard = FeatureFlag::ImeMarkedText.override_enabled(true);
        let (window_id, editor_view) = initialize_editor(&mut app);

        let presenter = Rc::new(RefCell::new(Presenter::new(window_id)));
        let mut updated = EntityIdSet::default();
        updated.insert(app.root_view_id(window_id).unwrap());
        let invalidation = WindowInvalidation {
            updated,
            ..Default::default()
        };
        app.update({
            let presenter = presenter.clone();
            move |ctx| {
                presenter.borrow_mut().invalidate(invalidation, ctx);
                presenter
                    .borrow_mut()
                    .build_scene(vec2f(800., 600.), 1., None, ctx);
            }
        });

        app.update({
            let presenter = presenter.clone();
            move |ctx| {
                ctx.simulate_window_event(
                    warpui::Event::SetMarkedText {
                        marked_text: "ni".to_owned(),
                        selected_range: 2..2,
                    },
                    window_id,
                    presenter,
                );
            }
        });
        editor_view.read(&app, |view, _ctx| {
            assert_eq!(view.ime_marked_text.as_deref(), Some("ni"));
        });

        app.update(move |ctx| {
            ctx.simulate_window_event(warpui::Event::ClearMarkedText, window_id, presenter);
        });
        editor_view.read(&app, |view, _ctx| assert_eq!(view.ime_marked_text, None));
    });
}
