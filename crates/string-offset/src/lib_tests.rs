use num_traits::SaturatingSub;

use crate::{ByteOffset, CharCounter, CharOffset, StringRange};

#[test]
fn test_char_counter_sequential() {
    let mut counter = CharCounter::new("It’s a 🌯!");
    assert_eq!(counter.char_offset(0), Some(0.into())); // I
    assert_eq!(counter.char_offset(1), Some(1.into())); // t
    assert_eq!(counter.char_offset(2), Some(2.into())); // ’ (3 bytes)
    assert_eq!(counter.char_offset(5), Some(3.into())); // s
    assert_eq!(counter.char_offset(6), Some(4.into())); // <space>
    assert_eq!(counter.char_offset(7), Some(5.into())); // a
    assert_eq!(counter.char_offset(8), Some(6.into())); // <space>
    assert_eq!(counter.char_offset(9), Some(7.into())); // 🌯 (4 bytes)
    assert_eq!(counter.char_offset(13), Some(8.into())); // !
}

#[test]
fn test_char_counter_out_of_order_fails() {
    let mut counter = CharCounter::new("words");
    assert_eq!(counter.char_offset(2), Some(2.into()));
    // Because the iterator only advances forwards, using an earlier offset is unsupported.
    assert_eq!(counter.char_offset(1), None);
}

#[test]
fn test_char_counter_non_boundary() {
    let mut counter = CharCounter::new("It’s a 🌯!");
    // Byte 11 is in the middle of the emoji.
    assert_eq!(counter.char_offset(11), None);
}

#[test]
fn test_char_counter_out_of_bounds() {
    let mut counter = CharCounter::new("short string");
    assert_eq!(counter.char_offset(7), Some(7.into()));
    assert_eq!(counter.char_offset(100), None);
}

#[test]
fn test_char_counter_nonsequential() {
    let mut counter = CharCounter::new("It’s a 🌯!");

    // Skip ahead to the t.
    assert_eq!(counter.char_offset(1), Some(1.into()));

    // Now, skip past the multi-byte apostrophe to the first space.
    assert_eq!(counter.char_offset(6), Some(4.into()));

    // Skip ahead to the emoji.
    assert_eq!(counter.char_offset(9), Some(7.into()));
}

#[test]
fn test_offset_arithmetic() {
    let mut offset = CharOffset::from(5);
    assert_eq!(offset + CharOffset::from(3), CharOffset::from(8));
    assert_eq!(offset + 2, CharOffset::from(7));
    assert_eq!(offset - CharOffset::from(2), CharOffset::from(3));
    assert_eq!(offset - 5, CharOffset::zero());

    offset += 4;
    offset -= CharOffset::from(1);
    assert_eq!(offset.as_usize(), 8);
    assert_eq!(offset.to_string(), "8");
}

#[test]
fn test_offset_add_signed() {
    assert_eq!(ByteOffset::from(10).add_signed(-4), ByteOffset::from(6));
    assert_eq!(ByteOffset::from(10).add_signed(4), ByteOffset::from(14));
    assert_eq!(ByteOffset::from(3).add_signed(-3), ByteOffset::zero());
}

#[test]
fn test_char_offset_add_assign_i32() {
    let mut offset = CharOffset::from(5);
    offset += 3;
    assert_eq!(offset, CharOffset::from(8));
    offset += -2i32;
    assert_eq!(offset, CharOffset::from(6));
}

#[test]
fn test_offset_saturating_sub() {
    let small = ByteOffset::from(2);
    let large = ByteOffset::from(7);
    assert_eq!(large.saturating_sub(&small), ByteOffset::from(5));
    assert_eq!(small.saturating_sub(&large), ByteOffset::zero());
}

#[test]
fn test_offset_ranges() {
    assert_eq!(
        CharOffset::range(2..5),
        CharOffset::from(2)..CharOffset::from(5)
    );
    let empty = ByteOffset::from(4).empty_range();
    assert_eq!(empty, ByteOffset::from(4)..ByteOffset::from(4));
    assert!(empty.is_empty());
}

#[test]
fn test_extend_range_end() {
    let mut range = StringRange {
        char_range: 0..4,
        byte_range: 0..6,
    };
    range.extend_range_end(&StringRange {
        char_range: 2..7,
        byte_range: 3..10,
    });
    assert_eq!(range.char_range, 0..7);
    assert_eq!(range.byte_range, 0..10);

    // A range that ends earlier must not shrink the current one.
    range.extend_range_end(&StringRange {
        char_range: 1..3,
        byte_range: 1..4,
    });
    assert_eq!(range.char_range, 0..7);
    assert_eq!(range.byte_range, 0..10);
}
