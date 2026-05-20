use anyhow::{Context, Result};
use clap::Parser;
use ksni::blocking::TrayMethods;
use monitor_switcher::config::Config;
use monitor_switcher::ddc::DdcUtilClient;
use monitor_switcher::state::SharedState;
use monitor_switcher::switcher::{SwitchOutcome, Switcher};
use monitor_switcher::tray::AppTray;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "monitor-switcher",
    about = "Auto-switch secondary monitors based on Dell P3425WE active input via DDC/CI"
)]
struct Cli {
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,

    #[arg(long, help = "Run one poll cycle and exit (no tray)")]
    once: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)
        .with_context(|| format!("loading config from {}", cli.config.display()))?;
    info!(path = %cli.config.display(), profiles = cfg.profiles.len(), "config loaded");

    let ddc = Arc::new(DdcUtilClient::new());
    let switcher = Arc::new(Switcher::new(cfg.clone(), ddc));
    let state = SharedState::new(20);

    if cli.once {
        run_tick(&switcher, &state);
        return Ok(());
    }

    let tray = AppTray::new(switcher.clone(), state.clone());
    let _tray_handle = tray
        .spawn()
        .map_err(|e| anyhow::anyhow!("tray spawn failed: {e}"))?;

    let interval = Duration::from_secs(cfg.poll_interval_seconds.max(1));
    info!(interval_s = interval.as_secs(), "polling started");
    loop {
        if !state.paused() {
            run_tick(&switcher, &state);
        }
        std::thread::sleep(interval);
    }
}

fn run_tick(switcher: &Arc<Switcher>, state: &Arc<SharedState>) {
    match switcher.tick() {
        Ok(SwitchOutcome::NoChange) => {}
        Ok(SwitchOutcome::Applied { profile, writes }) => {
            state.set_last_profile(Some(profile.clone()));
            state.push_event(format!("auto → {profile} ({} writes)", writes.len()));
            info!(profile = %profile, "applied");
        }
        Ok(SwitchOutcome::UnknownMasterValue(v)) => {
            warn!(
                value = format!("0x{v:02X}"),
                "master value has no matching profile"
            );
            state.push_event(format!("unknown master 0x{v:02X}"));
        }
        Err(e) => {
            error!(error = %e, "tick failed");
            state.push_event(format!("error: {e}"));
        }
    }
}
