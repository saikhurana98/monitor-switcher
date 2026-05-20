use crate::config::{Config, Profile};
use crate::ddc::{DdcClient, DdcError};
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum SwitchError {
    #[error("ddc: {0}")]
    Ddc(#[from] DdcError),
    #[error("unknown profile '{0}'")]
    UnknownProfile(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchOutcome {
    NoChange,
    Applied {
        profile: String,
        writes: Vec<(u8, u8)>,
    },
    UnknownMasterValue(u8),
}

#[derive(Debug)]
pub struct Switcher {
    cfg: Config,
    ddc: Arc<dyn DdcClient>,
    last_master_value: std::sync::Mutex<Option<u8>>,
}

impl Switcher {
    #[must_use]
    pub fn new(cfg: Config, ddc: Arc<dyn DdcClient>) -> Self {
        Self {
            cfg,
            ddc,
            last_master_value: std::sync::Mutex::new(None),
        }
    }

    pub const fn config(&self) -> &Config {
        &self.cfg
    }

    pub fn tick(&self) -> Result<SwitchOutcome, SwitchError> {
        let master_value = self.ddc.read_input(self.cfg.master.bus)?;
        let mut last = self.last_master_value.lock().unwrap();
        if Some(master_value) == *last {
            return Ok(SwitchOutcome::NoChange);
        }
        *last = Some(master_value);
        drop(last);
        let Some((name, profile)) = self.cfg.profile_for_trigger(master_value) else {
            warn!(
                value = format!("0x{master_value:02X}"),
                "no profile for master value"
            );
            return Ok(SwitchOutcome::UnknownMasterValue(master_value));
        };
        let name = name.to_owned();
        let writes = self.apply_profile_targets(profile)?;
        info!(profile = %name, count = writes.len(), "auto-switch applied");
        Ok(SwitchOutcome::Applied {
            profile: name,
            writes,
        })
    }

    pub fn force(&self, profile_name: &str) -> Result<SwitchOutcome, SwitchError> {
        let profile = self
            .cfg
            .profiles
            .get(profile_name)
            .ok_or_else(|| SwitchError::UnknownProfile(profile_name.to_owned()))?;
        self.ddc.write_input(self.cfg.master.bus, profile.trigger)?;
        *self.last_master_value.lock().unwrap() = Some(profile.trigger);
        let mut writes = vec![(self.cfg.master.bus, profile.trigger)];
        writes.extend(self.apply_profile_targets(profile)?);
        info!(profile = %profile_name, count = writes.len(), "force switch applied");
        Ok(SwitchOutcome::Applied {
            profile: profile_name.to_owned(),
            writes,
        })
    }

    fn apply_profile_targets(&self, profile: &Profile) -> Result<Vec<(u8, u8)>, SwitchError> {
        let mut writes = Vec::with_capacity(profile.targets.len());
        for t in &profile.targets {
            match self.ddc.read_input(t.bus) {
                Ok(current) if current == t.value => continue,
                Ok(_) | Err(_) => {}
            }
            self.ddc.write_input(t.bus, t.value)?;
            writes.push((t.bus, t.value));
        }
        Ok(writes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddc::mock::MockDdc;

    fn cfg() -> Config {
        let yaml = r#"
master: { bus: 5, name: "Dell" }
profiles:
  pc:
    label: "PC"
    trigger: 0x11
    targets:
      - { bus: 4, value: 0x11, name: "Acer" }
      - { bus: 3, value: 0x11, name: "Lenovo" }
  laptop:
    label: "Laptop"
    trigger: 0x1B
    targets:
      - { bus: 4, value: 0x12, name: "Acer" }
      - { bus: 3, value: 0x0F, name: "Lenovo" }
"#;
        Config::parse(yaml).unwrap()
    }

    #[test]
    fn tick_applies_profile_when_master_changes() {
        let mock = Arc::new(MockDdc::new(&[(5, 0x1B), (4, 0x11), (3, 0x11)]));
        let sw = Switcher::new(cfg(), mock.clone());
        let out = sw.tick().unwrap();
        match out {
            SwitchOutcome::Applied { profile, writes } => {
                assert_eq!(profile, "laptop");
                assert_eq!(writes, vec![(4, 0x12), (3, 0x0F)]);
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }

    #[test]
    fn tick_no_change_on_second_call() {
        let mock = Arc::new(MockDdc::new(&[(5, 0x1B), (4, 0x12), (3, 0x0F)]));
        let sw = Switcher::new(cfg(), mock.clone());
        let _first = sw.tick().unwrap();
        assert_eq!(sw.tick().unwrap(), SwitchOutcome::NoChange);
    }

    #[test]
    fn tick_skips_redundant_target_writes() {
        let mock = Arc::new(MockDdc::new(&[(5, 0x1B), (4, 0x12), (3, 0x11)]));
        let sw = Switcher::new(cfg(), mock.clone());
        let out = sw.tick().unwrap();
        match out {
            SwitchOutcome::Applied { writes, .. } => {
                assert_eq!(writes, vec![(3, 0x0F)]);
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }

    #[test]
    fn tick_unknown_master_value() {
        let mock = Arc::new(MockDdc::new(&[(5, 0xAA), (4, 0x11), (3, 0x11)]));
        let sw = Switcher::new(cfg(), mock.clone());
        assert_eq!(sw.tick().unwrap(), SwitchOutcome::UnknownMasterValue(0xAA));
        assert!(mock.writes().is_empty());
    }

    #[test]
    fn force_writes_master_and_targets() {
        let mock = Arc::new(MockDdc::new(&[(5, 0x11), (4, 0x11), (3, 0x11)]));
        let sw = Switcher::new(cfg(), mock.clone());
        let out = sw.force("laptop").unwrap();
        match out {
            SwitchOutcome::Applied { profile, writes } => {
                assert_eq!(profile, "laptop");
                assert!(writes.contains(&(5, 0x1B)));
                assert!(writes.contains(&(4, 0x12)));
                assert!(writes.contains(&(3, 0x0F)));
            }
            other => panic!("expected Applied, got {other:?}"),
        }
    }

    #[test]
    fn force_rejects_unknown_profile() {
        let mock = Arc::new(MockDdc::new(&[(5, 0x11)]));
        let sw = Switcher::new(cfg(), mock);
        assert!(matches!(
            sw.force("nope"),
            Err(SwitchError::UnknownProfile(_))
        ));
    }
}
