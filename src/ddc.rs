use std::process::Command;
use thiserror::Error;

pub const VCP_INPUT_SOURCE: u8 = 0x60;

#[derive(Debug, Error)]
pub enum DdcError {
    #[error("ddcutil exited with status {0}: {1}")]
    Exit(i32, String),
    #[error("ddcutil spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("unparseable ddcutil output: {0:?}")]
    Parse(String),
}

pub trait DdcClient: std::fmt::Debug + Send + Sync {
    fn read_input(&self, bus: u8) -> Result<u8, DdcError>;
    fn write_input(&self, bus: u8, value: u8) -> Result<(), DdcError>;
}

#[derive(Debug, Default, Clone)]
pub struct DdcUtilClient {
    pub binary: String,
}

impl DdcUtilClient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            binary: "ddcutil".to_owned(),
        }
    }
}

impl DdcClient for DdcUtilClient {
    fn read_input(&self, bus: u8) -> Result<u8, DdcError> {
        let out = Command::new(&self.binary)
            .args([
                "--bus",
                &bus.to_string(),
                "getvcp",
                &format!("{VCP_INPUT_SOURCE:X}"),
                "--brief",
            ])
            .output()?;
        if !out.status.success() {
            let code = out.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(DdcError::Exit(code, stderr));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        parse_brief_output(&stdout)
    }

    fn write_input(&self, bus: u8, value: u8) -> Result<(), DdcError> {
        let out = Command::new(&self.binary)
            .args([
                "--bus",
                &bus.to_string(),
                "setvcp",
                &format!("{VCP_INPUT_SOURCE:X}"),
                &format!("0x{value:02X}"),
            ])
            .output()?;
        if !out.status.success() {
            let code = out.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            return Err(DdcError::Exit(code, stderr));
        }
        Ok(())
    }
}

fn parse_brief_output(stdout: &str) -> Result<u8, DdcError> {
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[0] == "VCP" {
            let token = parts.last().copied().unwrap_or("");
            let hex = token.trim_start_matches('x').trim_start_matches('X');
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                return Ok(v);
            }
        }
    }
    Err(DdcError::Parse(stdout.to_owned()))
}

#[cfg(test)]
pub mod mock {
    use super::{DdcClient, DdcError};
    use std::sync::Mutex;

    #[derive(Debug)]
    pub struct MockDdc {
        pub reads: Mutex<std::collections::HashMap<u8, u8>>,
        pub writes: Mutex<Vec<(u8, u8)>>,
        pub fail_write: Mutex<Option<DdcError>>,
    }

    impl MockDdc {
        #[must_use]
        pub fn new(initial: &[(u8, u8)]) -> Self {
            Self {
                reads: Mutex::new(initial.iter().copied().collect()),
                writes: Mutex::new(Vec::new()),
                fail_write: Mutex::new(None),
            }
        }

        pub fn set_read(&self, bus: u8, value: u8) {
            self.reads.lock().unwrap().insert(bus, value);
        }

        pub fn writes(&self) -> Vec<(u8, u8)> {
            self.writes.lock().unwrap().clone()
        }
    }

    impl DdcClient for MockDdc {
        fn read_input(&self, bus: u8) -> Result<u8, DdcError> {
            self.reads
                .lock()
                .unwrap()
                .get(&bus)
                .copied()
                .ok_or_else(|| DdcError::Parse(format!("no mock value for bus {bus}")))
        }

        fn write_input(&self, bus: u8, value: u8) -> Result<(), DdcError> {
            let pending = self.fail_write.lock().unwrap().take();
            if let Some(err) = pending {
                return Err(err);
            }
            self.writes.lock().unwrap().push((bus, value));
            self.reads.lock().unwrap().insert(bus, value);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_brief() {
        assert_eq!(parse_brief_output("VCP 60 SNC x11\n").unwrap(), 0x11);
    }

    #[test]
    fn parse_usbc_value() {
        assert_eq!(parse_brief_output("VCP 60 SNC x1B").unwrap(), 0x1B);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(matches!(
            parse_brief_output("nope nope"),
            Err(DdcError::Parse(_))
        ));
    }

    #[test]
    fn mock_round_trip() {
        let m = mock::MockDdc::new(&[(5, 0x11)]);
        assert_eq!(m.read_input(5).unwrap(), 0x11);
        m.write_input(5, 0x1B).unwrap();
        assert_eq!(m.read_input(5).unwrap(), 0x1B);
        assert_eq!(m.writes(), vec![(5, 0x1B)]);
    }
}
