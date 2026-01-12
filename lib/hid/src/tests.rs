use crate::layouts::*;

#[track_caller]
fn layout_test(layout: &dyn KeyboardLayout) {
    assert_eq!(layout.estimate_key_stroke_from_char('\0'), None);

    // Keystroke estimation from ASCII codes isn't perfect.
    // However, we guarantee that the estimated keystrokes can be reversed using the current layout.
    for ch in 1..128 {
        let ch = ch as u8 as char;
        println!("Testing character {:?}", ch);

        let key_stroke = layout.estimate_key_stroke_from_char(ch).unwrap();

        let translated_char = layout.translate(key_stroke).unwrap();

        assert_eq!(
            ch, translated_char,
            "Character translation mismatch: expected {:?}, got {:?}",
            ch, translated_char
        );
    }

    // for ch in 128..256 {
    //     assert_eq!(layout.estimate_key_stroke_from_char(ch as u8 as char), None);
    // }
}

#[test]
fn test_us101() {
    layout_test(&us101::Us101);
}

#[test]
fn test_jp109() {
    layout_test(&jp109::Jp109);
}
