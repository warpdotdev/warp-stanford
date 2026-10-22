//! Integration tests for the editable `editor_view:yank` binding: Settings rebind/clear/reset, the
//! conflict warning against `editor_view:delete_all_left`, and keybindings saved before startup.

use warp::integration_testing::create_file_with_contents;
use warp::integration_testing::keybindings::keybinding_file_path;
use warp::integration_testing::settings::open_settings_page;
use warp::integration_testing::step::new_step_with_default_assertions;
use warp::integration_testing::terminal::{
    assert_input_editor_contents, wait_until_bootstrapped_single_pane_for_tab,
};
use warp::integration_testing::view_getters::single_input_view_for_tab;
use warp::integration_testing::workspace::assert_tab_count;
use warp::settings_view::SettingsSection;
use warp::settings_view::keybindings::{KeybindingsView, KeybindingsViewAction};
use warpui_core::integration::{AssertionOutcome, TestStep};
use warpui_core::keymap::{DescriptionContext, Keystroke, Trigger};
use warpui_core::platform::OperatingSystem;
use warpui_core::{App, ViewHandle, WindowId, async_assert, async_assert_eq};

use super::new_builder;
use crate::Builder;

const SENTINEL: &str = "echo yank-sentinel";
const YANK_BINDING: &str = "editor_view:yank";
const YANK_DESCRIPTION: &str = "Yank (last killed text)";
/// Settings shows binding descriptions in title case.
const YANK_SETTINGS_LABEL: &str = "Yank (Last Killed Text)";
const DELETE_ALL_LEFT_BINDING: &str = "editor_view:delete_all_left";
const DELETE_ALL_LEFT_DESCRIPTION: &str = "Delete all left";
const DELETE_ALL_LEFT_SETTINGS_LABEL: &str = "Delete All Left";
/// Input editor, code editor and notebook editor each register `editor_view:delete_all_left`.
const DELETE_ALL_LEFT_REGISTRATIONS: usize = 3;
const REPLACEMENT_KEY: &str = "ctrl-alt-y";

fn keystrokes(key: &str) -> Trigger {
    Trigger::Keystrokes(vec![Keystroke::parse(key).expect("valid keystroke")])
}

fn default_delete_all_left_trigger() -> Trigger {
    if OperatingSystem::get().is_mac() {
        keystrokes("cmd-backspace")
    } else {
        Trigger::Empty
    }
}

fn seed_keybindings_file(builder: Builder, yaml: &'static str) -> Builder {
    builder.with_setup(move |_utils| {
        create_file_with_contents(yaml.as_bytes(), &keybinding_file_path());
    })
}

fn keybindings_view(app: &App, window_id: WindowId) -> ViewHandle<KeybindingsView> {
    app.views_of_type::<KeybindingsView>(window_id)
        .and_then(|views| views.into_iter().next())
        .expect("Keybindings settings view should exist")
}

/// Asserts the effective trigger, and the default trigger it overrides, of every registration of
/// the editable binding `name`.
fn assert_editable_binding_triggers(
    name: &'static str,
    expected_registrations: usize,
    expected_trigger: Trigger,
    expected_default: Trigger,
) -> TestStep {
    TestStep::new(&format!("Check registered triggers of {name}")).add_named_assertion(
        format!(
            "All {expected_registrations} {name} registrations use {expected_trigger:?} over \
             default {expected_default:?}"
        ),
        move |app, _| {
            let actual: Vec<(Trigger, Trigger)> = app.read(|ctx| {
                ctx.editable_bindings()
                    .filter(|lens| lens.name == name)
                    .map(|lens| {
                        (
                            lens.trigger.clone(),
                            lens.original_trigger.unwrap_or(lens.trigger).clone(),
                        )
                    })
                    .collect()
            });
            let expected =
                vec![(expected_trigger.clone(), expected_default.clone()); expected_registrations];
            async_assert_eq!(
                actual,
                expected,
                "Unexpected (effective, default) triggers for {name}"
            )
        },
    )
}

fn type_sentinel() -> TestStep {
    new_step_with_default_assertions("Type the sentinel into the input")
        .with_typed_characters(&[SENTINEL])
        .add_named_assertion(
            "Input contains exactly the sentinel",
            assert_input_editor_contents(0, SENTINEL),
        )
}

fn press_key_expecting_input(key: &'static str, expected: &'static str) -> TestStep {
    new_step_with_default_assertions(&format!("Press {key}"))
        .with_keystrokes(&[key])
        .add_named_assertion(
            format!("Input is exactly {expected:?} after {key}"),
            assert_input_editor_contents(0, expected),
        )
}

/// With no binding on Ctrl-Y, the injected keystroke is unhandled and falls through as the typed
/// character `y`. Backspace then removes that character without touching the kill buffer.
fn press_unbound_ctrl_y() -> Vec<TestStep> {
    vec![
        press_key_expecting_input("ctrl-y", "y"),
        press_key_expecting_input("backspace", ""),
    ]
}

fn type_and_kill_sentinel() -> Vec<TestStep> {
    vec![type_sentinel(), press_key_expecting_input("ctrl-u", "")]
}

fn assert_input_focused() -> TestStep {
    TestStep::new("Check the terminal input is focused").add_named_assertion(
        "Terminal input editor is focused",
        |app, window_id| {
            let input = single_input_view_for_tab(app, window_id, 0);
            input.read(app, |input, ctx| {
                async_assert!(
                    input.editor().is_focused(ctx),
                    "Terminal input editor should be focused"
                )
            })
        },
    )
}

/// Opens the Keyboard shortcuts page and filters it with the page's own search.
fn open_keybindings_and_search(query: &'static str) -> Vec<TestStep> {
    vec![
        open_settings_page(SettingsSection::Keybindings),
        new_step_with_default_assertions(&format!("Search keyboard shortcuts for {query:?}"))
            .with_action(move |app, window_id, _| {
                keybindings_view(app, window_id)
                    .update(app, |view, ctx| view.search_for_binding(query, ctx));
            }),
    ]
}

/// Asserts that the top-ranked shortcut row is `name`, which appears exactly once in the results.
fn assert_first_row(
    name: &'static str,
    description: &'static str,
    trigger: Option<&'static str>,
) -> TestStep {
    TestStep::new(&format!("Check the top shortcut row is {name}")).add_named_assertion(
        format!("Top row is the only {name} row ({description:?}) with trigger {trigger:?}"),
        move |app, window_id| {
            let expected_trigger = trigger.map(|key| Keystroke::parse(key).expect("valid key"));
            keybindings_view(app, window_id).read(app, |view, _| {
                let rows: Vec<_> = view.rows.iter().flatten().collect();
                let first_row = rows.first().map(|row| {
                    (
                        row.binding.name.clone(),
                        row.binding
                            .description
                            .in_context(DescriptionContext::Default)
                            .to_owned(),
                        row.binding.trigger.clone(),
                    )
                });
                let rows_named = rows.iter().filter(|row| row.binding.name == name).count();
                async_assert_eq!(
                    (first_row, rows_named),
                    (
                        Some((
                            name.to_owned(),
                            description.to_owned(),
                            expected_trigger.clone()
                        )),
                        1
                    ),
                    "Unexpected (top row, number of {name} rows)"
                )
            })
        },
    )
}

fn assert_first_row_trigger(
    trigger: Option<&'static str>,
) -> impl FnMut(&mut App, WindowId) -> AssertionOutcome + 'static {
    move |app, window_id| {
        let expected = trigger.map(|key| Keystroke::parse(key).expect("valid key"));
        keybindings_view(app, window_id).read(app, |view, _| {
            let actual = view
                .rows
                .as_ref()
                .and_then(|rows| rows.first())
                .and_then(|row| row.binding.trigger.clone());
            async_assert_eq!(
                actual,
                expected,
                "Unexpected trigger on the first shortcut row"
            )
        })
    }
}

fn click_first_row() -> TestStep {
    new_step_with_default_assertions("Click the first shortcut row to edit it")
        .with_click_on_saved_position("first_keybinding_setting")
        .add_named_assertion(
            "Shortcut editing suspends key binding dispatch",
            |app, window_id| {
                async_assert!(
                    !app.key_bindings_dispatching_enabled(window_id),
                    "Key binding dispatching should be disabled while editing a shortcut"
                )
            },
        )
}

fn assert_first_row_conflict(
    expected: bool,
) -> impl FnMut(&mut App, WindowId) -> AssertionOutcome + 'static {
    move |app, window_id| {
        keybindings_view(app, window_id).read(app, |view, _| {
            async_assert_eq!(
                view.row_has_conflict(0),
                expected,
                "Unexpected conflict warning state on the first shortcut row"
            )
        })
    }
}

fn capture_keystroke_in_first_row(key: &'static str) -> TestStep {
    new_step_with_default_assertions(&format!("Press {key} in the shortcut editor"))
        .with_keystrokes(&[key])
        .add_named_assertion(
            format!("First row captured {key}"),
            assert_first_row_trigger(Some(key)),
        )
}

/// Dispatches an existing Keyboard shortcuts page action, the same one its buttons dispatch.
fn dispatch_keybindings_action(
    description: &str,
    action: fn() -> KeybindingsViewAction,
    expected_trigger: Option<&'static str>,
) -> TestStep {
    new_step_with_default_assertions(description)
        .with_action(move |app, window_id, _| {
            let view_id = keybindings_view(app, window_id).id();
            app.dispatch_typed_action(window_id, &[view_id], &action());
        })
        .add_named_assertion(
            format!("First row trigger is {expected_trigger:?}"),
            assert_first_row_trigger(expected_trigger),
        )
        .add_named_assertion("Key binding dispatch is re-enabled", |app, window_id| {
            async_assert!(
                app.key_bindings_dispatching_enabled(window_id),
                "Key binding dispatching should be re-enabled after editing"
            )
        })
}

fn close_settings_tab() -> Vec<TestStep> {
    vec![
        new_step_with_default_assertions("Close the settings tab")
            .with_hover_over_saved_position("close_tab_button:1")
            .with_click_on_saved_position("close_tab_button:1")
            .add_named_assertion("Only the terminal tab remains", assert_tab_count(1)),
        assert_input_focused(),
    ]
}

/// Rebinding Yank in Settings moves yank to the captured key and leaves the old default inactive.
pub fn test_yank_settings_rebind() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_steps(open_keybindings_and_search(YANK_DESCRIPTION))
        .with_step(assert_first_row(
            YANK_BINDING,
            YANK_SETTINGS_LABEL,
            Some("ctrl-y"),
        ))
        .with_step(click_first_row())
        .with_step(capture_keystroke_in_first_row(REPLACEMENT_KEY))
        .with_step(dispatch_keybindings_action(
            "Save the new Yank shortcut",
            || KeybindingsViewAction::ConfirmKeyStroke(0),
            Some(REPLACEMENT_KEY),
        ))
        .with_steps(close_settings_tab())
        .with_step(assert_editable_binding_triggers(
            YANK_BINDING,
            1,
            keystrokes(REPLACEMENT_KEY),
            keystrokes("ctrl-y"),
        ))
        .with_steps(type_and_kill_sentinel())
        .with_steps(press_unbound_ctrl_y())
        .with_step(press_key_expecting_input(REPLACEMENT_KEY, SENTINEL))
}

/// Clearing Yank in Settings leaves no fixed Ctrl-Y fallback; resetting it restores Ctrl-Y yank.
pub fn test_yank_settings_clear_then_reset() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_steps(open_keybindings_and_search(YANK_DESCRIPTION))
        .with_step(assert_first_row(
            YANK_BINDING,
            YANK_SETTINGS_LABEL,
            Some("ctrl-y"),
        ))
        .with_step(click_first_row())
        .with_step(dispatch_keybindings_action(
            "Clear the Yank shortcut",
            || KeybindingsViewAction::RemoveKeyStroke(0),
            None,
        ))
        .with_steps(close_settings_tab())
        .with_step(assert_editable_binding_triggers(
            YANK_BINDING,
            1,
            Trigger::Empty,
            keystrokes("ctrl-y"),
        ))
        .with_steps(type_and_kill_sentinel())
        .with_steps(press_unbound_ctrl_y())
        .with_steps(open_keybindings_and_search(YANK_DESCRIPTION))
        .with_step(assert_first_row(YANK_BINDING, YANK_SETTINGS_LABEL, None))
        .with_step(click_first_row())
        .with_step(dispatch_keybindings_action(
            "Reset the Yank shortcut to its default",
            || KeybindingsViewAction::ResetToDefaultKeyStroke(0),
            Some("ctrl-y"),
        ))
        .with_steps(close_settings_tab())
        .with_steps(type_and_kill_sentinel())
        .with_step(press_key_expecting_input("ctrl-y", SENTINEL))
}

/// Binding Delete all left to Ctrl-Y in Settings shows the existing conflict warning on both rows,
/// and the explicit Delete all left choice takes precedence over the default Yank binding.
pub fn test_yank_settings_conflict_warning() -> Builder {
    new_builder()
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_steps(open_keybindings_and_search(DELETE_ALL_LEFT_DESCRIPTION))
        .with_step(assert_first_row(
            DELETE_ALL_LEFT_BINDING,
            DELETE_ALL_LEFT_SETTINGS_LABEL,
            if OperatingSystem::get().is_mac() {
                Some("cmd-backspace")
            } else {
                None
            },
        ))
        // On macOS, Delete all left's default Cmd-Backspace already conflicts with
        // `terminal:executing_command_delete_line_start`; elsewhere it has no default.
        .with_step(click_first_row().add_named_assertion(
            "Delete all left conflicts only through its macOS default",
            assert_first_row_conflict(OperatingSystem::get().is_mac()),
        ))
        .with_step(
            capture_keystroke_in_first_row("ctrl-y")
                .add_named_assertion(
                    "Delete all left shows the conflict warning for Ctrl-Y",
                    assert_first_row_conflict(true),
                )
                .with_take_screenshot("delete_all_left_ctrl_y_conflict.png"),
        )
        .with_step(dispatch_keybindings_action(
            "Save Ctrl-Y for Delete all left",
            || KeybindingsViewAction::ConfirmKeyStroke(0),
            Some("ctrl-y"),
        ))
        .with_step(
            new_step_with_default_assertions("Search keyboard shortcuts for Yank").with_action(
                |app, window_id, _| {
                    keybindings_view(app, window_id).update(app, |view, ctx| {
                        view.search_for_binding(YANK_DESCRIPTION, ctx)
                    });
                },
            ),
        )
        .with_step(assert_first_row(
            YANK_BINDING,
            YANK_SETTINGS_LABEL,
            Some("ctrl-y"),
        ))
        .with_step(
            click_first_row()
                .add_named_assertion(
                    "Yank shows the conflict warning for Ctrl-Y",
                    assert_first_row_conflict(true),
                )
                .with_take_screenshot("yank_ctrl_y_conflict.png"),
        )
        .with_step(dispatch_keybindings_action(
            "Cancel editing the Yank shortcut",
            || KeybindingsViewAction::CancelKeyStrokeEditing(0),
            Some("ctrl-y"),
        ))
        .with_steps(close_settings_tab())
        .with_step(assert_editable_binding_triggers(
            DELETE_ALL_LEFT_BINDING,
            DELETE_ALL_LEFT_REGISTRATIONS,
            keystrokes("ctrl-y"),
            default_delete_all_left_trigger(),
        ))
        .with_step(type_sentinel())
        .with_step(press_key_expecting_input("ctrl-y", ""))
}

/// A saved `none` for Delete all left keeps it cleared, and Ctrl-Y yanks by default.
pub fn test_yank_saved_delete_all_left_none() -> Builder {
    seed_keybindings_file(new_builder(), r#""editor_view:delete_all_left": none"#)
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(assert_editable_binding_triggers(
            DELETE_ALL_LEFT_BINDING,
            DELETE_ALL_LEFT_REGISTRATIONS,
            Trigger::Empty,
            default_delete_all_left_trigger(),
        ))
        .with_step(assert_editable_binding_triggers(
            YANK_BINDING,
            1,
            keystrokes("ctrl-y"),
            keystrokes("ctrl-y"),
        ))
        .with_steps(type_and_kill_sentinel())
        .with_step(press_key_expecting_input("ctrl-y", SENTINEL))
}

/// A saved Delete all left rebinding keeps deleting on its custom key, and Ctrl-Y still yanks.
pub fn test_yank_saved_delete_all_left_rebound() -> Builder {
    seed_keybindings_file(
        new_builder(),
        r#""editor_view:delete_all_left": ctrl-alt-y"#,
    )
    .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
    .with_step(assert_editable_binding_triggers(
        DELETE_ALL_LEFT_BINDING,
        DELETE_ALL_LEFT_REGISTRATIONS,
        keystrokes(REPLACEMENT_KEY),
        default_delete_all_left_trigger(),
    ))
    .with_step(type_sentinel())
    .with_step(press_key_expecting_input(REPLACEMENT_KEY, ""))
    .with_steps(type_and_kill_sentinel())
    .with_step(press_key_expecting_input("ctrl-y", SENTINEL))
}

/// A saved explicit Ctrl-Y for Delete all left still deletes, taking precedence over default Yank.
pub fn test_yank_saved_delete_all_left_ctrl_y() -> Builder {
    seed_keybindings_file(new_builder(), r#""editor_view:delete_all_left": ctrl-y"#)
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(assert_editable_binding_triggers(
            DELETE_ALL_LEFT_BINDING,
            DELETE_ALL_LEFT_REGISTRATIONS,
            keystrokes("ctrl-y"),
            default_delete_all_left_trigger(),
        ))
        .with_step(assert_editable_binding_triggers(
            YANK_BINDING,
            1,
            keystrokes("ctrl-y"),
            keystrokes("ctrl-y"),
        ))
        .with_step(type_sentinel())
        .with_step(press_key_expecting_input("ctrl-y", ""))
        .with_steps(type_and_kill_sentinel())
        .with_step(press_key_expecting_input("ctrl-y", ""))
}

/// A saved `none` for Yank leaves Ctrl-Y without any yank fallback.
pub fn test_yank_saved_none() -> Builder {
    seed_keybindings_file(new_builder(), r#""editor_view:yank": none"#)
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(assert_editable_binding_triggers(
            YANK_BINDING,
            1,
            Trigger::Empty,
            keystrokes("ctrl-y"),
        ))
        .with_steps(type_and_kill_sentinel())
        .with_steps(press_unbound_ctrl_y())
}

/// A saved custom Yank trigger yanks on that key instead of the default Ctrl-Y.
pub fn test_yank_saved_custom_trigger() -> Builder {
    seed_keybindings_file(new_builder(), r#""editor_view:yank": ctrl-alt-y"#)
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(assert_editable_binding_triggers(
            YANK_BINDING,
            1,
            keystrokes(REPLACEMENT_KEY),
            keystrokes("ctrl-y"),
        ))
        .with_steps(type_and_kill_sentinel())
        .with_steps(press_unbound_ctrl_y())
        .with_step(press_key_expecting_input(REPLACEMENT_KEY, SENTINEL))
}
