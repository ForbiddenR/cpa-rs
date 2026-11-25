use cpa_rs::config::{CodexApiKey, Config};
use cpa_rs::routing::{RouteError, resolve_codex};

#[test]
fn resolves_the_example_alias_after_a_key_is_configured() {
    let mut config = Config::from_yaml(include_str!("../config.example.yaml")).unwrap();

    assert_eq!(
        resolve_codex(&config, "codex-test").err(),
        Some(RouteError::NoCredentials {
            model: "codex-test".into()
        })
    );

    config.api_keys.codex[0].keys = vec![
        CodexApiKey {
            api_key: "first-key".into(),
            proxy_url: Some("direct".into()),
            models: None,
        },
        CodexApiKey {
            api_key: "second-key".into(),
            proxy_url: None,
            models: None,
        },
    ];

    let route = resolve_codex(&config, "codex-test").unwrap();
    assert_eq!(route.base_url, "https://api.openai.com/v1");
    assert_eq!(route.upstream_model, "replace-with-responses-model");
    assert_eq!(route.api_key, "first-key");
    assert_eq!(route.proxy_url, Some("direct"));
}

#[test]
fn uses_each_keys_effective_model_list() {
    let config = Config::from_yaml(
        r#"
api-keys:
  codex:
    - base-url: https://responses.example.com/v1
      models:
        - {name: shared-upstream, alias: shared}
        - {name: inherited-only}
      keys:
        - api-key: empty-override
          models: []
        - api-key: custom-models
          models:
            - {name: custom-upstream, alias: shared}
            - {alias: upstream-id}
        - api-key: inherits-models
"#,
    )
    .unwrap();

    let override_route = resolve_codex(&config, "shared").unwrap();
    assert_eq!(override_route.upstream_model, "custom-upstream");
    assert_eq!(override_route.api_key, "custom-models");

    let alias_route = resolve_codex(&config, " upstream-id ").unwrap();
    assert_eq!(alias_route.upstream_model, "upstream-id");

    let inherited_route = resolve_codex(&config, "inherited-only").unwrap();
    assert_eq!(inherited_route.upstream_model, "inherited-only");
    assert_eq!(inherited_route.api_key, "inherits-models");
}

#[test]
fn distinguishes_missing_model_and_missing_eligible_key() {
    let config = Config::from_yaml(
        r#"
api-keys:
  codex:
    - base-url: https://responses.example.com/v1
      models: [{name: upstream, alias: public}]
      keys: [{api-key: override, models: []}]
  openai-compatibility:
    - base-url: https://chat.example.com/v1
      models: [{name: chat, alias: chat-only}]
      keys: []
"#,
    )
    .unwrap();

    assert_eq!(
        resolve_codex(&config, "public").err(),
        Some(RouteError::NoCredentials {
            model: "public".into()
        })
    );
    for model in ["upstream", "chat-only"] {
        assert_eq!(
            resolve_codex(&config, model).err(),
            Some(RouteError::ModelNotFound {
                model: model.into()
            })
        );
    }
    assert_eq!(
        resolve_codex(&config, "  ").err(),
        Some(RouteError::EmptyModel)
    );
}
