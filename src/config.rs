use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] serde_yml::Error),
    #[error("invalid: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: u64,
    pub master: Master,
    pub profiles: BTreeMap<String, Profile>,
}

const fn default_poll_interval() -> u64 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Master {
    pub bus: u8,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub label: String,
    #[serde(deserialize_with = "de_hex_u8")]
    pub trigger: u8,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Target {
    pub bus: u8,
    #[serde(deserialize_with = "de_hex_u8")]
    pub value: u8,
    pub name: String,
}

fn de_hex_u8<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let v = serde_yml::Value::deserialize(deserializer)?;
    match v {
        serde_yml::Value::Number(n) => n
            .as_u64()
            .and_then(|x| u8::try_from(x).ok())
            .ok_or_else(|| D::Error::custom("expected u8")),
        serde_yml::Value::String(s) => {
            let trimmed = s.trim();
            let stripped = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
                .unwrap_or(trimmed);
            u8::from_str_radix(stripped, 16).map_err(D::Error::custom)
        }
        other => Err(D::Error::custom(format!(
            "expected number or hex string, got {other:?}"
        ))),
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let cfg: Self = serde_yml::from_str(text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.profiles.is_empty() {
            return Err(ConfigError::Invalid("no profiles defined".into()));
        }
        let mut seen_triggers = BTreeMap::new();
        for (name, profile) in &self.profiles {
            if profile.targets.is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "profile '{name}' has no targets"
                )));
            }
            if let Some(other) = seen_triggers.insert(profile.trigger, name) {
                return Err(ConfigError::Invalid(format!(
                    "trigger 0x{:02X} duplicated between '{}' and '{}'",
                    profile.trigger, other, name
                )));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn profile_for_trigger(&self, value: u8) -> Option<(&str, &Profile)> {
        self.profiles
            .iter()
            .find(|(_, p)| p.trigger == value)
            .map(|(k, v)| (k.as_str(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_YAML: &str = r#"
poll_interval_seconds: 3
master:
  bus: 5
  name: "Dell P3425WE"
profiles:
  pc:
    label: "PC"
    trigger: 0x11
    targets:
      - { bus: 4, value: 0x11, name: "Acer HDMI-1" }
      - { bus: 3, value: 0x11, name: "Lenovo HDMI-1" }
  laptop:
    label: "Laptop"
    trigger: 0x1B
    targets:
      - { bus: 4, value: 0x12, name: "Acer HDMI-2" }
"#;

    #[test]
    fn parse_valid_yaml() {
        let cfg = Config::parse(VALID_YAML).unwrap();
        assert_eq!(cfg.poll_interval_seconds, 3);
        assert_eq!(cfg.master.bus, 5);
        assert_eq!(cfg.profiles.len(), 2);
        let pc = &cfg.profiles["pc"];
        assert_eq!(pc.trigger, 0x11);
        assert_eq!(pc.targets.len(), 2);
    }

    #[test]
    fn default_poll_interval_when_missing() {
        let yaml = r#"
master: { bus: 5, name: "x" }
profiles:
  a: { label: "A", trigger: 1, targets: [{ bus: 4, value: 2, name: "t" }] }
"#;
        let cfg = Config::parse(yaml).unwrap();
        assert_eq!(cfg.poll_interval_seconds, 2);
    }

    #[test]
    fn rejects_duplicate_triggers() {
        let yaml = r#"
master: { bus: 5, name: "x" }
profiles:
  a: { label: "A", trigger: 0x11, targets: [{ bus: 4, value: 1, name: "t" }] }
  b: { label: "B", trigger: 0x11, targets: [{ bus: 4, value: 2, name: "u" }] }
"#;
        let err = Config::parse(yaml).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(ref m) if m.contains("duplicated")));
    }

    #[test]
    fn rejects_empty_profiles() {
        let yaml = r#"
master: { bus: 5, name: "x" }
profiles: {}
"#;
        assert!(matches!(Config::parse(yaml), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn rejects_empty_targets() {
        let yaml = r#"
master: { bus: 5, name: "x" }
profiles:
  a: { label: "A", trigger: 1, targets: [] }
"#;
        assert!(matches!(Config::parse(yaml), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn hex_string_trigger_accepted() {
        let yaml = r#"
master: { bus: 5, name: "x" }
profiles:
  a:
    label: "A"
    trigger: "0x1B"
    targets: [{ bus: 4, value: "0x0F", name: "t" }]
"#;
        let cfg = Config::parse(yaml).unwrap();
        assert_eq!(cfg.profiles["a"].trigger, 0x1B);
        assert_eq!(cfg.profiles["a"].targets[0].value, 0x0F);
    }

    #[test]
    fn profile_for_trigger_match() {
        let cfg = Config::parse(VALID_YAML).unwrap();
        let (name, _) = cfg.profile_for_trigger(0x1B).unwrap();
        assert_eq!(name, "laptop");
        assert!(cfg.profile_for_trigger(0xAA).is_none());
    }

    #[test]
    fn load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.yaml");
        std::fs::write(&path, VALID_YAML).unwrap();
        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.master.name, "Dell P3425WE");
    }
}
