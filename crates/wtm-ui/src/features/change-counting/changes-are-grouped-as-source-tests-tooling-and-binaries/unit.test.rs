#[test]
fn rename_unicode_and_binary_numstat() {
    let bytes = "2\t3\t\0旧 name.txt\0tests/new\tname.test.js\0-\t-\timage.bin\x000\t0\tunchanged.txt\0";
    assert_eq!(numstat(bytes.as_bytes()).unwrap(), Counts {
        test: LineCounts { additions: 2, deletions: 3 }, binaries: 1, ..Counts::default()
    });
}

#[test]
fn classifier_matches_plugin_rules_and_test_precedence() {
    for path in ["src/main.rs", "src/testing/foo.rs", "src/scripts.rs", "src/foo.test", "test.rs",
        "contest_foo.rs", "vitest.config", "vitest.configured.js", "src/foo.sh.txt", "script/build.rs"] {
        assert_eq!(category(Path::new(path)), Category::Source, "{path}");
    }
    for path in ["src\\Tests\\fixture.rs", "src/foo.test.ts", "src/foo.spec.ts", "src/foo_test.ts", "test-foo.rs",
        "test_foo.rs", "tests.config.js", "test.setup.ts", "tests.setup.ts", "tests/tools/build.ps1", "scripts/unit.spec.sh"] {
        assert_eq!(category(Path::new(path)), Category::Test, "{path}");
    }
    for directory in ["test", "tests", "__tests__", "e2e", "spec", "testdata", "__mocks__", "__fixtures__", "fixtures", "__snapshots__"] {
        assert_eq!(category(Path::new(&format!("scripts/{directory}/file.sh"))), Category::Test);
    }
    for prefix in ["ava", "cypress", "jest", "karma", "playwright", "vitest"] {
        assert_eq!(category(Path::new(&format!("{prefix}.config.ts"))), Category::Test);
    }
    for directory in ["tools", "scripts", "development_tools", "development-tools", "tooling"] {
        assert_eq!(category(Path::new(&format!("{directory}/build.rs"))), Category::Extra);
    }
    for extension in ["ps1", "sh", "bash", "bat", "cmd"] {
        assert_eq!(category(Path::new(&format!("src/build.{extension}"))), Category::Extra);
        assert_eq!(category(Path::new(&format!(".{extension}"))), Category::Extra);
    }
    assert_eq!(category(Path::new("src/build.PS1")), Category::Extra);
}

#[test]
fn rejects_incomplete_rename_and_count_overflow() {
    assert_eq!(numstat(b"1\t0\t\0old\0").unwrap_err().kind, ErrorKind::InvalidOutput);
    assert_eq!(numstat(b"18446744073709551615\t0\ta\x001\t0\tb\0").unwrap_err().kind, ErrorKind::InvalidOutput);
    assert_eq!(numstat(b"1\t0\ta").unwrap_err().kind, ErrorKind::InvalidOutput);
}
