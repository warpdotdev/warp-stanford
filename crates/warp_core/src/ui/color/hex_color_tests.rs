use super::*;

#[test]
fn parses_full_and_shorthand_colors() {
    let expected = ColorU {
        r: 0x33,
        g: 0xff,
        b: 0x11,
        a: OPAQUE,
    };
    assert_eq!(coloru_from_hex_string("#33ff11"), Ok(expected));
    assert_eq!(coloru_from_hex_string("#3f1"), Ok(expected));
    assert_eq!(coloru_from_hex_string("#33FF11"), Ok(expected));
}

#[test]
fn rejects_malformed_colors() {
    assert_eq!(
        coloru_from_hex_string("33ff11"),
        Err(HexColorError::HashPrefix)
    );
    assert_eq!(
        coloru_from_hex_string("#33ff1"),
        Err(HexColorError::InvalidLength)
    );
    assert_eq!(
        coloru_from_hex_string("#zzzzzz"),
        Err(HexColorError::InvalidValue)
    );
}

#[test]
fn rejects_signs_and_non_ascii_text() {
    // "€" is three bytes long, so these have a valid byte length without being valid colors.
    for input in ["#+1+2+3", "#€", "#€€"] {
        assert_eq!(
            coloru_from_hex_string(input),
            Err(HexColorError::InvalidValue),
            "{input:?}"
        );
    }
}
