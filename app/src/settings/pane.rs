use enum_iterator::Sequence;
use pathfinder_color::ColorU;
use serde::{Deserialize, Serialize};
use settings::macros::define_settings_group;
use settings::{RespectUserSyncSetting, SupportedPlatforms, SyncToCloud};
use warp_core::ui::color::hex_color;
use warp_core::ui::theme::{AnsiColorIdentifier, WarpTheme};

/// A color for the divider between split panes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
#[schemars(
    description = "Either the name of a standard ANSI terminal color, or a hex color such as \"#39ff14\"."
)]
pub enum PaneDividerColorChoice {
    /// One of the theme's ANSI colors, which changes along with the theme.
    Ansi(AnsiColorIdentifier),
    /// A fixed color that is independent of the theme.
    Custom(
        #[serde(with = "hex_color")]
        #[schemars(with = "String")]
        ColorU,
    ),
}

impl PaneDividerColorChoice {
    pub fn resolve(self, theme: &WarpTheme) -> ColorU {
        match self {
            Self::Ansi(color) => color.to_ansi_color(&theme.terminal_colors().normal).into(),
            Self::Custom(color) => color,
        }
    }
}

impl settings_value::SettingsValue for PaneDividerColorChoice {}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Deserialize,
    Serialize,
    Sequence,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Thickness of the divider between split panes.",
    rename_all = "snake_case"
)]
pub enum PaneDividerThickness {
    #[default]
    Default,
    Medium,
    Thick,
}

impl PaneDividerThickness {
    /// The divider thickness in pixels, or `None` to use the built-in thickness.
    pub fn pixels(self) -> Option<f32> {
        match self {
            Self::Default => None,
            Self::Medium => Some(4.),
            Self::Thick => Some(6.),
        }
    }
}

define_settings_group!(PaneSettings, settings: [
    should_dim_inactive_panes: ShouldDimInactivePanes {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "appearance.panes.should_dim_inactive_panes",
        description: "Whether inactive panes are visually dimmed.",
    },
    focus_panes_on_hover: FocusPaneOnHover {
        type: bool,
        default: false,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "appearance.panes.focus_pane_on_hover",
        description: "Whether panes are focused when hovered over.",
    },
    pane_divider_color: PaneDividerColor {
        type: Option<PaneDividerColorChoice>,
        default: None,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "appearance.panes.pane_divider_color",
        description: "The color of the divider between split panes: a standard ANSI color name from the terminal theme, or a hex color. When unset, the theme's default divider color is used.",
    },
    pane_divider_thickness: PaneDividerThicknessSetting {
        type: PaneDividerThickness,
        default: PaneDividerThickness::Default,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "appearance.panes.pane_divider_thickness",
        description: "The thickness of the divider between split panes.",
    }
]);

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
