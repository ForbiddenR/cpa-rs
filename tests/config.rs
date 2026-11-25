use cpa_rs::config::{Config, ConfigError};

#[test]
fn loads_the_example_config() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/config.example.yaml");
    let config = Config::load(path).expect("example config should load");

    assert_eq!(config.config_version, 8);
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.server.port, 8317);
    assert!(config.management.secret_key.is_empty());
    assert!(config.access.api_keys.is_empty());
    assert_eq!(config.api_keys.codex[0].models[0].alias, "codex-test");
    assert!(config.api_keys.codex[0].keys.is_empty());
    let compatible = &config.api_keys.openai_compatibility[0];
    assert!(compatible.disabled);
    assert_eq!(compatible.models[0].alias, "codex-via-chat");
}

#[test]
fn preserves_group_settings_and_key_overrides() {
    let config = Config::from_yaml(
        r#"
server:
  server-id: 8d562b7f-8784-44e9-9895-8915b473829e
access:
  api-keys: [proxy-client]
api-keys:
  codex:
    - base-url: https://responses.example.com/v1
      models:
        - &model {name: main, alias: shared}
        - <<: *model
          name: backup
      keys:
        - api-key: test-upstream
        - api-key: test-override
          models: []
          proxy-url: direct
  openai-compatibility:
    - base-url: https://chat.example.com/v1
      keys: []
      models:
        - name: chat
"#,
    )
    .expect("valid CPA v8 subset should load");

    assert_eq!(config.config_version, 8);
    assert_eq!(config.access.api_keys, ["proxy-client"]);
    let codex = &config.api_keys.codex[0];
    assert_eq!(codex.models[1].name, "backup");
    assert_eq!(codex.models[1].alias, "shared");
    assert!(codex.keys[0].models.is_none());
    assert_eq!(codex.keys[1].models.as_ref().map(Vec::len), Some(0));
    assert_eq!(codex.keys[1].proxy_url.as_deref(), Some("direct"));
    let compatible = &config.api_keys.openai_compatibility[0];
    assert!(!compatible.disabled);
    assert!(compatible.keys.is_empty());
}

#[test]
fn allows_unconfigured_groups_and_a_null_server_id() {
    let config = Config::from_yaml(
        r#"
config-version: 8
server: {server-id: null}
api-keys:
  codex: [{keys: []}]
  openai-compatibility: [{keys: [], disabled: true}]
"#,
    )
    .expect("inactive groups can be configured later");
    assert!(config.server.server_id.is_empty());
}

#[test]
fn rejects_malformed_or_unsupported_yaml() {
    let cases = [
        ("", "expected a YAML mapping"),
        ("[]", "expected a YAML mapping"),
        ("server: []", "server"),
        ("server: {port: '8317'}", "server.port"),
        ("server: {port: 65536}", "server.port"),
        ("server: {port: 1, port: 2}", "duplicate"),
        ("server: {port: [}", "invalid YAML"),
        ("server: {}\n---\naccess: {}", "more than one document"),
        ("api-keys: [legacy-key]", "api-keys"),
        ("api-keys: {codex: [{}]}", "missing field `keys`"),
        (
            "api-keys: {codex: [{keys: null}]}",
            "api-keys.codex[0].keys",
        ),
        ("api-keys: {codex: null}", "api-keys.codex"),
        (
            "api-keys: {codex: [{base-url: https://host, keys: [null]}]}",
            "expected a mapping at index 0",
        ),
        (
            "api-keys: {codex: [{keys: [], protocol: responses}]}",
            "unknown field `protocol`",
        ),
        (
            "api-keys: {codex: [{keys: [{base-url: https://example.com}]}]}",
            "unknown field `base-url`",
        ),
    ];

    for (input, expected) in cases {
        let error = Config::from_yaml(input)
            .err()
            .unwrap_or_else(|| panic!("accepted invalid config: {input:?}"));
        assert!(
            error.to_string().contains(expected),
            "expected {expected:?} for {input:?}, got {error}"
        );
    }
}

#[test]
fn reports_the_field_path_for_invalid_values() {
    let cases = [
        ("config-version: 7", "config-version"),
        (
            "server: {server-id: 00000000-0000-0000-0000-000000000000}",
            "server.server-id",
        ),
        (
            "api-keys: {openai-compatibility: [{keys: []}]}",
            "api-keys.openai-compatibility[0].base-url",
        ),
        (
            "api-keys: {codex: [{keys: [{}], base-url: https:///bad}]}",
            "api-keys.codex[0].base-url",
        ),
        (
            "api-keys: {codex: [{keys: [], base-url: 'https://host/v1?key=test'}]}",
            "api-keys.codex[0].base-url",
        ),
        (
            "api-keys: {codex: [{base-url: 'https://host', keys: [{proxy-url: 'ftp://host'}]}]}",
            "api-keys.codex[0].keys[0].proxy-url",
        ),
        (
            "api-keys: {codex: [{keys: [], models: [{}]}]}",
            "api-keys.codex[0].models[0]",
        ),
    ];

    for (input, expected) in cases {
        let error = Config::from_yaml(input).err().expect("config should fail");
        assert!(
            matches!(error, ConfigError::InvalidField { ref field, .. } if field == expected),
            "expected field {expected} for {input:?}, got {error}"
        );
    }
}
