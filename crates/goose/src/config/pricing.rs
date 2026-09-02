use super::{Config, ConfigError};
use goose_providers::canonical::Pricing;
use serde::Deserialize;
use std::collections::HashSet;

pub const PRICING_OVERRIDES_CONFIG_KEY: &str = "GOOSE_PRICING_OVERRIDES";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingOverride {
    pub provider: String,
    pub model: String,
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_currency() -> String {
    "USD".to_string()
}

impl PricingOverride {
    pub fn pricing(&self) -> Pricing {
        Pricing {
            input: Some(self.input),
            output: Some(self.output),
            cache_read: self.cache_read,
            cache_write: self.cache_write,
        }
    }

    fn validate(&self, index: usize) -> Result<(), String> {
        if self.provider.is_empty() {
            return Err(format!("entry {index} has an empty provider"));
        }
        if self.model.is_empty() {
            return Err(format!("entry {index} has an empty model"));
        }
        if !self.currency.eq_ignore_ascii_case("USD") {
            return Err(format!(
                "entry {index} uses unsupported currency {:?}; only USD is supported",
                self.currency
            ));
        }
        for (name, rate) in [
            ("input", Some(self.input)),
            ("output", Some(self.output)),
            ("cache_read", self.cache_read),
            ("cache_write", self.cache_write),
        ] {
            if let Some(rate) = rate {
                if !rate.is_finite() || rate < 0.0 {
                    return Err(format!(
                        "entry {index} has invalid {name} rate {rate}; rates must be finite and non-negative"
                    ));
                }
            }
        }
        Ok(())
    }
}

pub fn get_pricing_overrides(config: &Config) -> Result<Vec<PricingOverride>, ConfigError> {
    let overrides: Vec<PricingOverride> = match config.get_param(PRICING_OVERRIDES_CONFIG_KEY) {
        Ok(overrides) => overrides,
        Err(ConfigError::NotFound(_)) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut keys = HashSet::new();
    for (index, entry) in overrides.iter().enumerate() {
        entry
            .validate(index)
            .map_err(ConfigError::DeserializeError)?;
        if !keys.insert((&entry.provider, &entry.model)) {
            return Err(ConfigError::DeserializeError(format!(
                "duplicate GOOSE_PRICING_OVERRIDES entry for provider {:?} and model {:?}",
                entry.provider, entry.model
            )));
        }
    }
    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(contents: &str) -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, contents).unwrap();
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();
        (dir, config)
    }

    #[test]
    fn loads_typed_per_million_usd_rates() {
        let (_dir, config) = config_with(
            r#"GOOSE_PRICING_OVERRIDES:
  - provider: openai
    model: gpt-negotiated
    input: 1.25
    output: 4.5
    cache_read: 0.125
"#,
        );
        let entries = get_pricing_overrides(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].currency, "USD");
        assert_eq!(entries[0].pricing().cache_write, None);
    }

    #[test]
    fn rejects_duplicates() {
        let (_dir, config) = config_with(
            r#"GOOSE_PRICING_OVERRIDES:
  - { provider: azure_foundry, model: deployment-a, input: 1, output: 2 }
  - { provider: azure_foundry, model: deployment-a, input: 3, output: 4 }
"#,
        );
        assert!(get_pricing_overrides(&config)
            .unwrap_err()
            .to_string()
            .contains("duplicate"));
    }

    #[test]
    fn rejects_partial_non_usd_and_invalid_rates() {
        for entry in [
            "{ provider: openai, model: m, input: 1 }",
            "{ provider: openai, model: m, input: 1, output: 2, currency: EUR }",
            "{ provider: openai, model: m, input: -1, output: 2 }",
            "{ provider: openai, model: m, input: .inf, output: 2 }",
        ] {
            let (_dir, config) = config_with(&format!(
                "GOOSE_PRICING_OVERRIDES:
  - {entry}
"
            ));
            assert!(get_pricing_overrides(&config).is_err(), "accepted {entry}");
        }
    }
}
