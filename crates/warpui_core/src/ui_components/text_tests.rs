use super::*;
use crate::color::ColorU;
use crate::fonts::FamilyId;

#[test]
fn default_styles_supply_missing_label_font_properties() {
    let styles = UiComponentStyles {
        font_family_id: Some(FamilyId(0)),
        font_size: Some(14.),
        font_color: Some(ColorU::white()),
        ..Default::default()
    };

    let label = Span::new("Don't show again.", Default::default()).with_default_style(styles);

    assert_eq!(label.text.styles, styles);
}

#[test]
fn default_styles_preserve_explicit_label_font_properties() {
    let label_styles = UiComponentStyles {
        font_family_id: Some(FamilyId(1)),
        font_size: Some(16.),
        font_color: Some(ColorU::white()),
        ..Default::default()
    };
    let defaults = UiComponentStyles {
        font_family_id: Some(FamilyId(0)),
        font_size: Some(12.),
        font_color: Some(ColorU::black()),
        ..Default::default()
    };

    let label = Span::new("Stash changes", label_styles).with_default_style(defaults);

    assert_eq!(label.text.styles, label_styles);
}

#[test]
fn explicit_state_styles_override_inherited_label_styles() {
    let defaults = UiComponentStyles {
        font_family_id: Some(FamilyId(0)),
        font_size: Some(14.),
        font_color: Some(ColorU::white()),
        ..Default::default()
    };
    let disabled_color = ColorU::new(128, 128, 128, 255);

    let label = Span::new("Label", Default::default())
        .with_default_style(defaults)
        .with_style(UiComponentStyles {
            font_color: Some(disabled_color),
            ..Default::default()
        });

    assert_eq!(label.text.styles.font_color, Some(disabled_color));
    assert_eq!(label.text.styles.font_family_id, Some(FamilyId(0)));
    assert_eq!(label.text.styles.font_size, Some(14.));
}
