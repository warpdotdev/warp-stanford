use enum_iterator::all;

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
