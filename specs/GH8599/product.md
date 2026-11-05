# Add Settings Icon to the Tools Panel — Product Spec
GitHub issue: https://github.com/warpdotdev/warp/issues/8599
Design reference: annotated screenshot in the issue (no Figma)

## Summary
Add a Settings (gear) icon button to the header row of the Tools Panel. Clicking it opens Warp's existing Settings interface exactly as the other Settings entry points do today. No new Settings UI is introduced.

## Problem
The Tools Panel header contains icon buttons for switching between panel views (Project explorer, Conversation history, Global search, Warp Drive), but there is no way to reach Settings from it. Users who work from the Tools Panel must move to the tab bar gear, the user menu, the command palette, or remember `Ctrl+,` / `Cmd+,`. The issue requests a Settings shortcut in the Tools Panel so Settings is quick to reach from where the user is already working.

## Goals
- A Settings icon is visible in the Tools Panel header whenever the Tools Panel is open.
- Clicking it opens Settings with the same behavior as the existing "Open Settings" action.
- The icon looks and behaves consistently with the existing Tools Panel header buttons and with the existing Settings gear elsewhere in the app.

## Non-goals
- Changing the Settings interface, its pages, or how Settings opens.
- Making the Settings icon a Tools Panel *view*. The panel's content does not change when the icon is clicked.
- Adding a user setting to hide or show the Settings icon in the Tools Panel.
- Opening Settings to a specific page (for example, the Appearance page's "Tools panel" section) from this icon.
- Removing or changing any existing Settings entry points (tab bar gear, user menu, command palette, keyboard shortcut, macOS app menu).
- Adding new telemetry events.

## Behavior
1. When the Tools Panel is open, its header row shows a Settings icon button using the same gear icon as the existing Settings button in the tab bar.
2. The Settings icon appears after the panel's view buttons (Project explorer, Conversation history, Global search, Warp Drive) and before the right-aligned "Close panel" (X) button, matching the position marked in the issue's screenshot.
3. The Settings icon looks and feels like the other header buttons: same size, spacing, padding, default icon color, and hover treatment, with a pointing-hand cursor on hover.
4. Hovering the Settings icon shows a tooltip reading "Settings", with the current keybinding for "Open Settings" as a sublabel (`Ctrl+,` on Windows and Linux, `Cmd+,` on macOS by default). If the user has unbound or rebound "Open Settings", the tooltip reflects that: no sublabel when unbound, the new shortcut when rebound.
5. Clicking the Settings icon behaves exactly like the existing "Open Settings" action:
   - If no Settings tab is open in the window, a new Settings tab opens and becomes active, showing the default Settings page.
   - If a Settings tab is already open in the window, that tab is focused and keeps whatever page it was showing. No second Settings tab is created.
   - Any open overlays (for example, palettes or menus) are closed, as they are by the existing action.
6. The Settings icon is an action, not a view: clicking it does not close the Tools Panel, change which panel view is active, or change the highlighted view button, and the icon itself never renders in an "active" (highlighted) state.
7. The Settings icon is shown regardless of which Tools Panel views the user has enabled in Settings → Appearance → Tools panel, and regardless of login or AI state.
8. When only one panel view is available, the Tools Panel continues to hide that single view button (current behavior), but the Settings icon is still shown in the header row.
9. The Settings icon is part of the Tools Panel: it is not visible when the panel is closed, and it behaves the same whether the panel is docked on the left or the right.
10. All existing ways to open Settings continue to work unchanged.

## Open questions
- Should the Settings icon be visually separated from the view buttons (for example, extra spacing or a divider) to signal that it is an action rather than a view? This spec assumes no separator, matching the issue's screenshot.
