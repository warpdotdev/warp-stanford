use enum_iterator::all;
use serde_json::json;
use settings_value::SettingsValue;

use super::*;

#[test]
fn default_thickness_defers_to_built_in_thickness() {
    assert_eq!(PaneDividerThickness::default().pixels(), None);
}

#[test]
fn custom_thicknesses_increase_in_order() {
    let pixels: Vec<_> = all::<PaneDividerThickness>()
        .filter_map(PaneDividerThickness::pixels)
        .collect();
    assert_eq!(pixels, vec![4., 6.]);
}

#[test]
fn divider_color_reads_ansi_names_and_hex_colors() {
    assert_eq!(
        PaneDividerColorChoice::from_file_value(&json!("green")),
        Some(PaneDividerColorChoice::Ansi(AnsiColorIdentifier::Green))
    );
    assert_eq!(
        PaneDividerColorChoice::from_file_value(&json!("#39ff14")),
        Some(PaneDividerColorChoice::Custom(ColorU::new(
            0x39, 0xff, 0x14, 0xff
        )))
    );
    assert_eq!(
        PaneDividerColorChoice::from_file_value(&json!("chartreuse")),
        None
    );
}

#[test]
fn divider_color_round_trips_through_the_settings_file() {
    for color in [
        PaneDividerColorChoice::Ansi(AnsiColorIdentifier::Magenta),
        PaneDividerColorChoice::Custom(ColorU::new(0x39, 0xff, 0x14, 0xff)),
    ] {
        assert_eq!(
            PaneDividerColorChoice::from_file_value(&color.to_file_value()),
            Some(color)
        );
    }
    assert_eq!(
        PaneDividerColorChoice::Custom(ColorU::new(0x39, 0xff, 0x14, 0xff)).to_file_value(),
        json!("#39ff14")
    );
}
