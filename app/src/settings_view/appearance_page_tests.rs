use super::*;

const BRIGHT_GREEN: ColorU = ColorU {
    r: 0x39,
    g: 0xff,
    b: 0x14,
    a: 0xff,
};

#[test]
fn pane_divider_hex_color_accepts_a_leading_hash() {
    assert_eq!(parse_pane_divider_hex_color("#39ff14"), Some(BRIGHT_GREEN));
}

#[test]
fn pane_divider_hex_color_accepts_a_missing_hash() {
    assert_eq!(parse_pane_divider_hex_color("39ff14"), Some(BRIGHT_GREEN));
}

#[test]
fn pane_divider_hex_color_expands_shorthand() {
    let expected = Some(ColorU::new(0x33, 0xff, 0x11, 0xff));
    assert_eq!(parse_pane_divider_hex_color("#3f1"), expected);
    assert_eq!(parse_pane_divider_hex_color("3f1"), expected);
}

#[test]
fn pane_divider_hex_color_rejects_invalid_input() {
    for input in [
        "",
        "#",
        "zzz",
        "#39ff1",
        "#39ff14ff",
        "##39ff14",
        "green",
        "+1+2+3",
        "€",
        "€€",
    ] {
        assert_eq!(parse_pane_divider_hex_color(input), None, "{input:?}");
    }
}
