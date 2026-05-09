#[test]
fn usage_page_filter_predicate() {
    let cases = [
        (0x01_u16, 0x05_u16, true),
        (0x01, 0x06, false),
        (0x0C, 0x01, false),
        (0x00, 0x00, true),
    ];
    for (up, u, expected) in cases {
        let accepted = up == 0 && u == 0 || (up == 0x01 && u == 0x05);
        assert_eq!(accepted, expected, "up={up:#X}, u={u:#X}");
    }
}
