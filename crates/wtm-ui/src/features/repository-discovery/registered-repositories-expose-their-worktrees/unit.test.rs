#[test]
fn parses_nul_worktree_record_framing() {
    let bytes = "worktree C:/repo\0HEAD 123\0branch refs/heads/main\0locked reason\0\0worktree C:/missing 树\0HEAD 456\0detached\0prunable gitdir missing\0\0worktree C:/bare\0bare\0\0";
    let trees = worktrees(bytes.as_bytes()).unwrap();
    assert_eq!(trees.len(), 3);
    assert_eq!(trees[0].path.to_str().unwrap(), "C:/repo");
    assert_eq!(trees[1].prunable.as_deref(), Some("prunable gitdir missing"));
    assert!(trees[2].bare);
    assert_eq!(worktrees(b"worktree C:/repo\0HEAD 123\0").unwrap_err().kind, ErrorKind::InvalidOutput);
}
