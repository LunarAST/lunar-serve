use lunar_serve::{ProjectIndex, ReposConfig};

#[test]
fn test_repos_config_deserialization_and_normalization() {
    let json_str = r#"{
      "version": "0.5.0",
      "projects": [
        {
          "name": "cellrix",
          "displayName": "Cellrix",
          "source": {
            "type": "github",
            "github": {
              "owner": "Jasonmilk",
              "repo": "Cellrix",
              "branch": "rs2"
            }
          },
          "visibility": "public",
          "path": "/opt/cellrix"
        }
      ]
    }"#;
    let config: Result<ReposConfig, _> = serde_json::from_str(json_str);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.version, "0.5.0");
    
    let index = ProjectIndex::from_config(&config);
    let name = index.get_name_by_github("Jasonmilk", "Cellrix", "rs2");
    assert_eq!(name, Some("cellrix"));
    
    // [ADDED v3.0] Test case-insensitive resolution of HTTP GitHub Coordinates
    let name_lower = index.get_name_by_github("jasonmilk", "cellrix", "rs2");
    assert_eq!(name_lower, Some("cellrix"));
}
