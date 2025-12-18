use crate::BoxDrawingChar;

#[test]
fn add_test() {
    for lhs in BoxDrawingChar::all_variants().iter().copied() {
        for rhs in BoxDrawingChar::all_variants().iter().copied() {
            let result = lhs.checked_add(rhs).unwrap();
            println!(
                "{:?} ({}) + {:?} ({}) = {:?} ({})",
                lhs,
                lhs.to_char(),
                rhs,
                rhs.to_char(),
                result,
                result.to_char()
            );

            if lhs == rhs {
                assert_eq!(result, lhs);
            } else if lhs == BoxDrawingChar::CROSS || rhs == BoxDrawingChar::CROSS {
                assert_eq!(result, BoxDrawingChar::CROSS);
            } else {
                // assert_ne!(result, lhs);
                // assert_ne!(result, rhs);
            }
        }
    }
}

#[test]
fn mirror_test() {
    for ch in BoxDrawingChar::all_variants().iter().copied() {
        let mirrored = ch.mirrored().unwrap();
        println!(
            "{:?} ({}) mirrored => {:?} ({})",
            ch,
            ch.to_char(),
            mirrored,
            mirrored.to_char()
        );

        match ch {
            BoxDrawingChar::HORIZONTAL | BoxDrawingChar::VERTICAL | BoxDrawingChar::CROSS => {
                assert_eq!(mirrored, ch);
            }
            _ => {
                assert_ne!(mirrored, ch);
            }
        }
    }
}

#[test]
fn rotate_test() {
    // Test rotation consistency
    for test_data in BoxDrawingChar::all_variants().iter().copied() {
        let dirs = test_data.line_dirs();
        let rotated1 = dirs.rotated_cw();
        let rotated2 = rotated1.rotated_ccw();
        let rotated3 = dirs.rotated_ccw();
        let rotated4 = rotated3.rotated_cw();
        let rotated5 = dirs.rotated_cw().rotated_cw().rotated_cw().rotated_cw();
        let rotated6 = dirs.rotated_ccw().rotated_ccw().rotated_ccw().rotated_ccw();

        assert_eq!(dirs, rotated2);
        assert_eq!(dirs, rotated4);
        assert_eq!(dirs, rotated5);
        assert_eq!(dirs, rotated6);
    }

    // Test flip pairs
    {
        let test_data_v = '│';
        let test_data_h = '─';
        let dir_v = BoxDrawingChar::from_char(test_data_v).unwrap().line_dirs();
        let dir_h = BoxDrawingChar::from_char(test_data_h).unwrap().line_dirs();

        let rotated_v1 = dir_v.rotated_cw();
        let rotated_v2 = dir_v.rotated_ccw();
        let rotated_h1 = dir_h.rotated_cw();
        let rotated_h2 = dir_h.rotated_ccw();

        assert_eq!(rotated_v1, dir_h);
        assert_eq!(rotated_v2, dir_h);
        assert_eq!(rotated_h1, dir_v);
        assert_eq!(rotated_h2, dir_v);
    }

    // Test specific known rotations
    for template in [&['└', '┌', '┐', '┘'], &['┴', '├', '┬', '┤']] {
        let mut template = template.to_vec();
        template.push(template[0]);

        for (lhs, rhs) in template.windows(2).map(|w| (w[0], w[1])) {
            println!("Testing rotation: {} -> {}", lhs, rhs);
            let dir_lhs = BoxDrawingChar::from_char(lhs).unwrap().line_dirs();
            let dir_rhs = BoxDrawingChar::from_char(rhs).unwrap().line_dirs();
            assert_eq!(dir_rhs, dir_lhs.rotated_cw());
            assert_eq!(dir_lhs, dir_rhs.rotated_ccw());
        }
    }

    // Test rotation on complex characters
    for test_data in ['┼'] {
        let ch = BoxDrawingChar::from_char(test_data).unwrap();
        let dirs = ch.line_dirs();
        let rotated1 = dirs.rotated_cw();
        let rotated2 = rotated1.rotated_ccw();
        let rotated3 = dirs.rotated_ccw();
        let rotated4 = rotated3.rotated_cw();

        assert_eq!(dirs, rotated1);
        assert_eq!(dirs, rotated2);
        assert_eq!(dirs, rotated3);
        assert_eq!(dirs, rotated4);
    }
}
