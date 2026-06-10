use lunar_serve::{ProjectIndex, ReposConfig};

fn make_repos_json() -> String {
    r#"{
        "version": "0.5.0",
        "projects": [
            {
                "name": "public-svc",
                "source": { "type": "local", "archiveUrl": "https://r2.example.com/archives/public-svc.tar.gz" },
                "visibility": "public"
            },
            {
                "name": "private-svc",
                "source": { "type": "local", "archiveUrl": "https://r2.example.com/archives/private-svc.tar.gz" },
                "visibility": "private"
            },
            {
                "name": "no-archive-svc",
                "source": { "type": "local" },
                "visibility": "public"
            }
        ]
    }"#.to_string()
}

#[test]
fn test_archive_url_parsed_correctly() {
    let config: ReposConfig = serde_json::from_str(&make_repos_json()).unwrap();
    let index = ProjectIndex::from_config(&config);

    let public_meta = index.get_meta("public-svc").unwrap();
    assert_eq!(public_meta.visibility, "public");
    assert!(public_meta.archive_url.is_some());

    let private_meta = index.get_meta("private-svc").unwrap();
    assert_eq!(private_meta.visibility, "private");
    assert!(private_meta.archive_url.is_some());

    let no_archive_meta = index.get_meta("no-archive-svc").unwrap();
    assert!(no_archive_meta.archive_url.is_none());
}
