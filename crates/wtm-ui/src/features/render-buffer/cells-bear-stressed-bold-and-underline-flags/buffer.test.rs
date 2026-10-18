use super::{Color, RenderBuffer};

#[test]
fn flags_survive_unicode_composition_and_plain_writes_reset_them() {
    use super::CellFlags;
    let color = Color { red: 1, green: 2, blue: 3 };
    let flags = CellFlags { stressed: true, bold: true, underline: true };
    let mut source = RenderBuffer::new(6, 1, color);
    assert!(source.cells().iter().all(|cell| cell.flags == CellFlags::default()));
    source.write_styled(0, 0, "\u{754c}e\u{301}", color, flags);
    source.put_styled(3, 0, 'x', color, CellFlags { bold: true, ..Default::default() });
    let mut target = RenderBuffer::new(6, 2, color);
    target.blit(1, 1, &source);
    assert!(target.row(1).unwrap()[1..4].iter().all(|cell| cell.flags == flags));
    assert!(target.row(1).unwrap()[4].flags.bold);
    assert!(!target.row(1).unwrap()[4].flags.stressed);
    target.put(2, 1, 'y', color);
    assert_eq!(target.row(1).unwrap()[1].text(), " ");
    assert!(target.row(1).unwrap()[1..3].iter().all(|cell| cell.flags == CellFlags::default()));
    assert_eq!(target.row(1).unwrap()[3].flags, flags);
}
