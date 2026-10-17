#[test]
fn malformed_reload_preserves_last_good_and_recovers() {
    let mut registrations = Registrations::default();
    let directory = std::env::current_dir().unwrap().join("fixture/.git");
    let bytes = serde_json::to_vec(&serde_json::json!({ "repos": [directory] })).unwrap();
    assert!(registrations.reload(Ok(bytes.clone())));
    assert_eq!(registrations.repos, vec![directory]);
    assert!(registrations.reload(Ok(b"malformed".to_vec())));
    assert_eq!(registrations.repos.len(), 1);
    assert_eq!(registrations.error.as_ref().unwrap().kind, ErrorKind::Config);
    assert!(registrations.reload(Ok(bytes)));
    assert_eq!(registrations.error, None);
    assert!(registrations.reload(Ok(Vec::new())));
    assert!(registrations.repos.is_empty());
}
