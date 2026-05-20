use crate::state::SharedState;
use crate::switcher::Switcher;
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip, Tray};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{error, info};

#[derive(Debug)]
pub struct AppTray {
    pub switcher: Arc<Switcher>,
    pub state: Arc<SharedState>,
    profile_order: Vec<String>,
}

impl AppTray {
    #[must_use]
    pub fn new(switcher: Arc<Switcher>, state: Arc<SharedState>) -> Self {
        let profile_order: Vec<String> = switcher.config().profiles.keys().cloned().collect();
        Self {
            switcher,
            state,
            profile_order,
        }
    }

    fn selected_index(&self) -> Option<usize> {
        let last = self.state.last_profile()?;
        self.profile_order.iter().position(|n| n == &last)
    }

    fn force_to_index(&self, idx: usize) {
        let Some(name) = self.profile_order.get(idx).cloned() else {
            return;
        };
        match self.switcher.force(&name) {
            Ok(_) => {
                self.state.set_last_profile(Some(name.clone()));
                self.state.push_event(format!("force → {name}"));
                info!(profile = %name, "force switch from tray");
            }
            Err(e) => {
                self.state.push_event(format!("force failed: {e}"));
                error!(error = %e, "force switch failed");
            }
        }
    }
}

impl Tray for AppTray {
    fn icon_name(&self) -> String {
        "video-display".to_owned()
    }

    fn title(&self) -> String {
        "Monitor Switcher".to_owned()
    }

    fn id(&self) -> String {
        "monitor-switcher".to_owned()
    }

    fn tool_tip(&self) -> ToolTip {
        let profile = self
            .state
            .last_profile()
            .unwrap_or_else(|| "unknown".into());
        let paused = if self.state.paused() { " (paused)" } else { "" };
        ToolTip {
            icon_name: "video-display".to_owned(),
            icon_pixmap: Vec::new(),
            title: "Monitor Switcher".to_owned(),
            description: format!("Profile: {profile}{paused}"),
        }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        Vec::new()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = Vec::new();

        let radio_options: Vec<RadioItem> = self
            .profile_order
            .iter()
            .map(|n| {
                let label = self
                    .switcher
                    .config()
                    .profiles
                    .get(n)
                    .map_or_else(|| n.clone(), |p| p.label.clone());
                RadioItem {
                    label,
                    ..RadioItem::default()
                }
            })
            .collect();

        items.push(
            RadioGroup {
                selected: self.selected_index().unwrap_or(usize::MAX),
                select: Box::new(|t: &mut Self, idx: usize| t.force_to_index(idx)),
                options: radio_options,
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        items.push(build_events_submenu(&self.state).into());

        items.push(MenuItem::Separator);

        let paused = self.state.paused();
        items.push(
            CheckmarkItem {
                label: "Pause auto-switch".to_owned(),
                checked: paused,
                activate: Box::new(|t: &mut Self| {
                    let now = t.state.toggle_paused();
                    t.state.push_event(if now { "paused" } else { "resumed" });
                }),
                ..CheckmarkItem::default()
            }
            .into(),
        );

        items.push(
            StandardItem {
                label: "Quit".to_owned(),
                icon_name: "application-exit".to_owned(),
                activate: Box::new(|_| std::process::exit(0)),
                ..StandardItem::default()
            }
            .into(),
        );

        items
    }
}

fn build_events_submenu(state: &Arc<SharedState>) -> SubMenu<AppTray> {
    let events = state.events_snapshot();
    let now = SystemTime::now();
    let mut submenu: Vec<MenuItem<AppTray>> = Vec::new();

    if events.is_empty() {
        submenu.push(
            StandardItem {
                label: "(no events yet)".to_owned(),
                enabled: false,
                ..StandardItem::default()
            }
            .into(),
        );
    } else {
        for ev in events.iter().rev() {
            submenu.push(
                StandardItem {
                    label: format!("{} — {}", ev.age_string(now), ev.message),
                    enabled: false,
                    ..StandardItem::default()
                }
                .into(),
            );
        }
        submenu.push(MenuItem::Separator);
        submenu.push(
            StandardItem {
                label: "Clear".to_owned(),
                icon_name: "edit-clear".to_owned(),
                activate: Box::new(|t: &mut AppTray| t.state.clear_events()),
                ..StandardItem::default()
            }
            .into(),
        );
    }

    SubMenu {
        label: format!("Events ({})", events.len()),
        icon_name: "view-list-symbolic".to_owned(),
        submenu,
        ..SubMenu::default()
    }
}
