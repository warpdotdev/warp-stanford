# Add Settings Icon to the Tools Panel — Tech Spec
Product spec: `specs/GH8599/product.md`
GitHub issue: https://github.com/warpdotdev/warp/issues/8599

## Context
The user-facing "Tools Panel" is `LeftPanelView` in `app/src/workspace/view/left_panel.rs`. Its header row is built in `LeftPanelView::render` (`left_panel.rs:1301`): a left-aligned "toolbelt" row of view-switching icon buttons and a right-aligned close button, laid out with `MainAxisAlignment::SpaceBetween` (`left_panel.rs:1386-1394`).

Relevant code:
- `left_panel.rs:75-81` — `MouseStateHandles`, one `MouseStateHandle` per toolbelt button, created once in `LeftPanelView::new`.
- `left_panel.rs:84-90` — `LeftPanelAction` (`ProjectExplorer`, `GlobalSearch`, `WarpDrive`, `ConversationListView`, `SignIn`).
- `left_panel.rs:197-213` — `ToolbeltButtonConfig`; `left_panel.rs:222` — `toolbelt_buttons: Vec<ToolbeltButtonConfig>`, rebuilt from the available views in `update_available_views`.
- `left_panel.rs:994-1028` — `close_button`, which renders an `icon_button_with_color` with a keybinding tooltip and dispatches `WorkspaceAction::ToggleLeftPanel` directly from the panel. This is the precedent for a panel button that dispatches a `WorkspaceAction` rather than a `LeftPanelAction`.
- `left_panel.rs:1030-1046` — `update_button_active_states`, an exhaustive match over `LeftPanelAction`.
- `left_panel.rs:1048-1112` — `render_button`, the shared styling for toolbelt buttons (24x24, active/inactive colors, tooltip with keybinding sublabel).
- `left_panel.rs:1304-1329` — the toolbelt row. Mouse state handles are held in a fixed four-element `vec` and paired with `toolbelt_buttons` via `zip`, and the row is only rendered when `toolbelt_buttons.len() > 1`.
- `app/src/workspace/view.rs:18952-18963` — `Workspace::show_settings` → `show_settings_with_section(None)` → `close_all_overlays` + `open_settings_pane` (`view.rs:8821`), which focuses the window's existing Settings pane or opens a new Settings tab.
- `app/src/workspace/view.rs:22097-22112` — the tab bar's `render_settings_button`, which uses `Icon::Gear`, tooltip "Settings", and the `SHOW_SETTINGS_KEYBINDING_NAME` (`"workspace:show_settings"`, `view.rs:676`) keybinding, and dispatches `WorkspaceAction::ShowSettings`.
- `crates/warp_core/src/ui/icons.rs:44,396` — `Icon::Gear` → `bundled/svg/gear.svg`.
- `app/src/workspace/view.rs:24068` — `compute_left_panel_views`, which gates each view on cargo features, feature flags, and the Settings → Appearance → Tools panel toggles. The Settings button is not a view and is not affected by this function.

## Proposed changes
### 1. Render a Settings button in the Tools Panel header
Add a `settings_button_mouse_state: MouseStateHandle` field to `LeftPanelView` alongside `close_button_mouse_state`, initialized once in `LeftPanelView::new`. Per the repo's WarpUI rules, the handle must be stored on the view and cloned during render, never created inline.

Add a `settings_button(&self, appearance, app) -> Box<dyn Element>` method:
- Icon: `Icon::Gear`, matching the tab bar Settings button.
- Color and sizing: extract the styling `render_button` applies (icon color, 24x24 size, 4px padding, active background, tooltip with keybinding sublabel, pointing-hand cursor) into a shared `toolbelt_icon_button` helper that both `render_button` and `settings_button` call with `is_active` set appropriately. Sharing one helper keeps the gear visually identical to its neighbors and prevents the two from drifting (product invariant 3).
- Tooltip: `"Settings"`, with a sublabel from `keybinding_name_to_display_string("workspace:show_settings", app)` when a binding exists (invariant 4). Reading the binding at render time, as `close_button` does, keeps the tooltip correct after the user rebinds or unbinds the shortcut without extra event plumbing.
- Click: `ctx.dispatch_typed_action(WorkspaceAction::ShowSettings)`, reusing the existing Settings flow end-to-end (invariant 5).

This button is intentionally **not** added to `toolbelt_buttons` and does not get a `LeftPanelAction` variant:
- `toolbelt_buttons` represents selectable panel views and drives the active-state highlight. Keeping Settings out of it guarantees the icon never renders as active and never changes the active view (invariant 6), with no new match arms in `update_button_active_states` or `handle_action_with_force_open`.
- The toolbelt row pairs buttons and mouse states positionally with `zip` over a fixed four-element vec. Appending a fifth config there would silently drop it. Rendering Settings as its own child with its own handle avoids that pitfall.

### 2. Compose the header row
In `LeftPanelView::render`, build the left side of the header as a `Flex::row()` (same `CrossAxisAlignment::Center` and 4px spacing as the toolbelt) containing:
1. the existing toolbelt buttons, only when `toolbelt_buttons.len() > 1` (unchanged rule, so a single view button stays hidden — invariant 8), then
2. the Settings button, always.

The close button stays as the right-aligned child, so the Settings icon sits after the view buttons and before the X (invariant 2). Because the header is part of `LeftPanelView`, the icon automatically follows the panel when docked right (`panel_position`) and is not shown when the panel is closed (invariant 9). There is no gating on login, AI state, or the Tools panel visibility settings (invariant 7).

### 3. No new setting, feature flag, or telemetry
The change is a small, additive UI entry point to an existing action, so it is not behind a feature flag. No toggleable setting is added (product non-goal), so no Command Palette entry is needed. No telemetry is added (product non-goal). `WorkspaceAction::ShowSettings` does not emit an event today, and adding a "settings opened from" event would be a separate change.

## Testing and validation
### Integration test (`crates/integration/`)
Wrap the Settings button in a `SavePosition` with a stable id exported as `TOOLS_PANEL_SETTINGS_BUTTON_POSITION_ID` (`"left_panel:settings_button"`) from `warp::workspace`, following the existing `NEW_TAB_BUTTON_POSITION_ID` pattern. Add a test that:
1. Opens the Tools Panel, asserts it is open (`assert_is_left_panel_open`), and records the panel's active view.
2. Clicks the Settings button and asserts a second tab (Settings) is open and focused (invariant 5, first bullet).
3. Navigates Settings to the Appearance page (`click_settings_nav_page`), so later assertions do not depend on the default Settings page.
4. Returns to the original tab and asserts the Tools Panel is still open with the same active view it had before the click (invariant 6). The active view is read through a `Workspace::left_panel_active_view` accessor gated on the `integration_tests` feature.
5. Clicks the button again and asserts there are still exactly two tabs, the Settings tab is focused, and it is still on the Appearance page (invariant 5, second bullet).

### Unit test (`app/src/workspace/view_tests.rs`)
Following `test_tools_panel_warp_drive_toggle_updates_available_views` (`view_tests.rs:5415`), add a test that opens the Tools Panel with a single available view, renders the window, and asserts the Settings button's saved position is present in the presenter's position cache (invariant 8). Checking the rendered frame, rather than adding a test-only accessor, verifies what is actually drawn.

### Manual validation (screenshots required for the PR)
- Open the Tools Panel and confirm the gear icon appears after the view buttons and before the X, matching the issue screenshot. Capture before/after screenshots (invariants 1–3).
- Hover the gear and confirm the "Settings" tooltip shows `Ctrl+,` on Windows/Linux. Rebind and then unbind "Open Settings" in Settings → Keyboard shortcuts and confirm the tooltip updates (invariant 4).
- Click the gear with no Settings tab open: a Settings tab opens on the default page. Navigate Settings to another page, switch tabs, click the gear again: the same Settings tab is focused on that page, and no duplicate is created (invariant 5).
- Confirm the active view highlight and panel content are unchanged after clicking, and the gear never highlights (invariant 6).
- In Settings → Appearance → Tools panel, turn off views until only one remains; confirm the single view button is hidden and the gear is still visible (invariants 7–8).
- Dock the Tools Panel on the right and repeat the click check (invariant 9).
- Confirm `Ctrl+,`, the command palette "Open Settings" entry, and the tab bar gear still work (invariant 10).

## Risks and mitigations
- **Header width at narrow panel sizes.** Adding a fifth icon slightly widens the left side of the header. The left side is already wrapped in `Shrinkable`, and the panel enforces `MIN_SIDEBAR_WIDTH`. Verify at minimum width that the gear and the close button remain visible and do not overlap.
- **Redundancy with the tab bar gear.** When the `AvatarInTabBar` flag is off, the tab bar already shows a Settings gear. Having two gears on screen is acceptable because they serve different locations, and the issue requests the Tools Panel shortcut explicitly.

## Follow-ups
- If users ask to hide the gear, add a Tools panel visibility toggle in Settings → Appearance with a matching Command Palette entry.
