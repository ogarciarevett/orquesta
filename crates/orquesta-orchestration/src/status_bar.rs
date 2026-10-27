// Copyright (c) 2026 Lovecast Inc.
// Copyright (c) 2026 Omar Garcia
// SPDX-License-Identifier: MIT
use crate::{Mailbox, Result, invalid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const PROVIDERS: [&str; 10] = [
    "Claude",
    "Codex",
    "Gemini",
    "Cursor",
    "Grok",
    "Kimi",
    "MiniMax",
    "Antigravity",
    "OpenCode Go",
    "ZCode",
];
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Threshold {
    #[default]
    Normal,
    Warning,
    Urgent,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Window {
    pub label: String,
    pub percent_used: Option<f64>,
    pub reset_in_seconds: Option<u64>,
    pub reset_at_iso: Option<String>,
    #[serde(default)]
    pub threshold: Threshold,
}
impl Window {
    fn normalize(&mut self) {
        self.percent_used = self
            .percent_used
            .filter(|p| p.is_finite())
            .map(|p| p.clamp(0.0, 100.0));
        self.threshold = match self.percent_used {
            Some(p) if p >= 80.0 => Threshold::Urgent,
            Some(p) if p >= 60.0 => Threshold::Warning,
            _ => Threshold::Normal,
        };
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Loading,
    Off,
    NotSignedIn,
    Ready,
    Stale,
    ErrorNoCache,
    ErrorCached,
    Refreshing,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub phase: Phase,
    pub windows: Vec<Window>,
    pub updated_at: Option<u64>,
    #[serde(default)]
    pub plan: Option<String>,
    pending: Option<String>,
    #[serde(default)]
    lease_until: Option<u64>,
    completed: BTreeSet<String>,
}
impl Provider {
    fn failed_phase(&self) -> Phase {
        if self
            .windows
            .iter()
            .any(|window| window.percent_used.is_some())
        {
            Phase::ErrorCached
        } else {
            Phase::ErrorNoCache
        }
    }
    fn recover(&mut self, now: u64) {
        if self.pending.is_some() && self.lease_until.is_none_or(|until| now >= until) {
            self.phase = self.failed_phase();
            self.completed.insert(self.pending.take().unwrap());
            self.lease_until = None;
        }
    }
    fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            ..Default::default()
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Inventory {
    Terminals,
    Ports,
    Hosts,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    #[default]
    Compact,
    Detailed,
}
#[derive(Serialize, Deserialize)]
pub struct StatusBar {
    pub providers: Vec<Provider>,
    pub visibility: BTreeMap<String, bool>,
    pub display_mode: DisplayMode,
    pub inventory: BTreeMap<Inventory, BTreeSet<String>>,
}
impl Default for StatusBar {
    fn default() -> Self {
        let providers: Vec<_> = PROVIDERS
            .iter()
            .map(|name| Provider::new(name.to_lowercase().replace(' ', "-"), (*name).into()))
            .collect();
        let visibility = providers
            .iter()
            .map(|p| (p.id.clone(), true))
            .chain(["resources", "ports", "hosts", "terminals"].map(|s| (s.into(), true)))
            .collect();
        Self {
            providers,
            visibility,
            display_mode: DisplayMode::Compact,
            inventory: BTreeMap::new(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Snapshot,
    Configure {
        id: String,
        name: String,
    },
    Visibility {
        key: String,
        visible: bool,
    },
    Display {
        mode: DisplayMode,
    },
    Inventory {
        kind: Inventory,
        id: String,
        present: bool,
    },

    Begin {
        id: String,
        request_id: String,
    },
    Finish {
        id: String,
        token: u64,
        now: u64,
        phase: Phase,
        plan: Option<String>,
        #[serde(default)]
        windows: Vec<Window>,
    },
}
impl Mailbox {
    pub fn status_bar(&self, action: Action) -> Result<Value> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        self.transaction(|state| state.status_bar.apply(action, now))
    }
}
impl StatusBar {
    fn provider(&mut self, id: &str) -> Result<&mut Provider> {
        let provider = self.providers.iter_mut().find(|p| p.id == id);
        provider.ok_or_else(|| invalid("unknown provider"))
    }
    fn apply(&mut self, action: Action, now: u64) -> Result<Value> {
        for provider in &mut self.providers {
            provider.recover(now);
        }
        let mut started = false;
        let mut token = None;
        let mut urgent = Vec::new();
        match action {
            Action::Snapshot => (),
            Action::Display { mode } => self.display_mode = mode,
            Action::Visibility { key, visible } => {
                let preference = self.visibility.get_mut(&key);
                *preference.ok_or_else(|| invalid("unknown visibility key"))? = visible
            }
            Action::Inventory { kind, id, present } => {
                if id.is_empty() {
                    return Err(invalid("inventory ID required"));
                }
                let entries = self.inventory.entry(kind).or_default();
                if present {
                    entries.insert(id);
                } else {
                    entries.remove(&id);
                }
            }
            Action::Configure { id, name } => {
                if id.is_empty() || name.is_empty() {
                    return Err(invalid("entry identity required"));
                }
                let id = format!("decision:{id}");
                if let Some(provider) = self.providers.iter_mut().find(|p| p.id == id) {
                    provider.name = name;
                } else {
                    self.providers.push(Provider::new(id.clone(), name));
                }
                self.visibility.entry(id).or_insert(true);
            }
            Action::Begin { id, request_id } => {
                if request_id.is_empty() {
                    return Err(invalid("request ID required"));
                }
                let provider = self.provider(&id)?;
                if provider.pending.is_none() && !provider.completed.contains(&request_id) {
                    provider.pending = Some(request_id);
                    provider.lease_until = Some(now.saturating_add(30));
                    provider.phase = Phase::Refreshing;
                    started = true;
                }
                token = provider
                    .pending
                    .as_ref()
                    .map(|_| provider.completed.len() as u64 + 1);
            }
            Action::Finish {
                id,
                token,
                now,
                phase,
                plan,
                mut windows,
            } => {
                let provider = self.provider(&id)?;
                if provider.pending.is_none() || token != provider.completed.len() as u64 + 1 {
                    return Err(invalid("obsolete refresh token"));
                }
                provider.phase = match phase {
                    Phase::Ready => {
                        for window in &mut windows {
                            window.normalize();
                        }
                        let urgent_before = provider
                            .windows
                            .iter()
                            .any(|w| w.threshold == Threshold::Urgent);
                        let urgent_now = windows.iter().any(|w| w.threshold == Threshold::Urgent);
                        if urgent_now && !urgent_before {
                            urgent.push(id);
                        }
                        provider.windows = windows;
                        provider.updated_at = Some(now);
                        if plan.is_some() {
                            provider.plan = plan;
                        }
                        Phase::Ready
                    }
                    Phase::ErrorNoCache | Phase::ErrorCached => provider.failed_phase(),
                    Phase::Loading | Phase::Refreshing => {
                        return Err(invalid("refresh requires a terminal state"));
                    }
                    other => other,
                };
                provider.completed.insert(provider.pending.take().unwrap());
                provider.lease_until = None;
            }
        }
        let counts: BTreeMap<_, _> = [Inventory::Terminals, Inventory::Ports, Inventory::Hosts]
            .map(|kind| (kind, self.inventory.get(&kind).map_or(0, BTreeSet::len)))
            .into();
        Ok(
            json!({"snapshot": self, "counts": counts, "started": started, "token": token, "urgent": urgent}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_boundaries_preserve_zero_and_missing_data() {
        for (value, expected) in [
            (None, Threshold::Normal),
            (Some(0.0_f64), Threshold::Normal),
            (Some(59.9), Threshold::Normal),
            (Some(60.0), Threshold::Warning),
            (Some(79.9), Threshold::Warning),
            (Some(80.0), Threshold::Urgent),
            (Some(200.0), Threshold::Urgent),
        ] {
            let mut window: Window = serde_json::from_value(json!({"label":"5h", "percent_used": value, "reset_in_seconds": 3600, "reset_at_iso":"2026-10-27T12:00:00Z"})).unwrap();
            window.normalize();
            assert_eq!(window.threshold, expected);
            assert_eq!(window.percent_used, value.map(|p| p.min(100.0)));
            assert_eq!(window.reset_in_seconds, Some(3600));
            assert_eq!(window.reset_at_iso.as_deref(), Some("2026-10-27T12:00:00Z"));
        }
    }
    #[test]
    fn restart_and_timeout_release_claims_without_accepting_late_results() {
        let mut state = StatusBar::default();
        let begin = |key: &str| Action::Begin {
            id: "codex".into(),
            request_id: key.into(),
        };
        let first = state.apply(begin("interrupted"), 10).unwrap();
        let directory = std::env::temp_dir().join(format!("status-lease-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let stored = json!({"status_bar": state, "recipients": [], "messages": []});
        std::fs::write(
            directory.join("mailbox.json"),
            serde_json::to_vec(&stored).unwrap(),
        )
        .unwrap();
        let reopened = Mailbox::open(&directory)
            .unwrap()
            .status_bar(Action::Snapshot)
            .unwrap();
        assert_eq!(
            reopened["snapshot"]["providers"][1]["phase"],
            "error_no_cache"
        );
        std::fs::remove_dir_all(&directory).unwrap();
        let persisted = serde_json::to_vec(&state).unwrap();
        let mut state: StatusBar = serde_json::from_slice(&persisted).unwrap();
        assert_eq!(
            state.apply(begin("duplicate"), 39).unwrap()["started"],
            false
        );
        let expired = state.apply(Action::Snapshot, 40).unwrap();
        assert_eq!(
            expired["snapshot"]["providers"][1]["phase"],
            "error_no_cache"
        );
        let second = state.apply(begin("retry"), 41).unwrap();
        assert_eq!(second["started"], true);
        let finish = |token| Action::Finish {
            id: "codex".into(),
            token,
            now: 42,
            phase: Phase::Ready,
            plan: None,
            windows: vec![serde_json::from_value(json!({"label":"5h", "percent_used":0})).unwrap()],
        };
        assert!(
            state
                .apply(finish(first["token"].as_u64().unwrap()), 42)
                .is_err()
        );
        state
            .apply(finish(second["token"].as_u64().unwrap()), 42)
            .unwrap();
        state.apply(begin("cached-timeout"), 43).unwrap();
        let expired = state.apply(Action::Snapshot, 73).unwrap();
        assert_eq!(expired["snapshot"]["providers"][1]["phase"], "error_cached");
        assert_eq!(
            expired["snapshot"]["providers"][1]["windows"][0]["percent_used"],
            0.0
        );
        state.apply(begin("legacy"), 74).unwrap();
        let mut legacy = serde_json::to_value(&state).unwrap();
        legacy["providers"][1]
            .as_object_mut()
            .unwrap()
            .remove("lease_until");
        let mut legacy: StatusBar = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            legacy.apply(begin("after-restart"), 75).unwrap()["started"],
            true
        );
    }
}
