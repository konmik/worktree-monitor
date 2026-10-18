use super::content_rows;

#[test]
fn attached_rows_share_a_grid_boundary_and_other_rows_have_one_blank_row() {
    let (offsets, total) = content_rows(&[1, 2, 3, 1], &[true, false, true]).unwrap();
    assert_eq!(offsets, vec![0, 1, 4, 7]);
    assert_eq!(total, 8);
}

#[test]
fn separate_rows_have_one_blank_row_without_a_trailing_gap() {
    let (offsets, total) = content_rows(&[2, 3, 1], &[]).unwrap();
    assert_eq!(offsets, vec![0, 3, 7]);
    assert_eq!(total, 8);
}

#[test]
fn empty_content_has_no_rows() {
    let (offsets, total) = content_rows(&[], &[]).unwrap();
    assert!(offsets.is_empty());
    assert_eq!(total, 0);
}
