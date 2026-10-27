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
    pending: Option<String>,
    completed: BTreeSet<String>,
}
impl Provider {
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
        #[serde(default)]
        windows: Vec<Window>,
    },
}
impl Mailbox {
    pub fn status_bar(&self, action: Action) -> Result<Value> {
        self.transaction(|state| state.status_bar.apply(action))
    }
}
impl StatusBar {
    fn provider(&mut self, id: &str) -> Result<&mut Provider> {
        let provider = self.providers.iter_mut().find(|p| p.id == id);
        provider.ok_or_else(|| invalid("unknown provider"))
    }
    fn apply(&mut self, action: Action) -> Result<Value> {
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
                        Phase::Ready
                    }
                    Phase::ErrorNoCache | Phase::ErrorCached
                        if provider.windows.iter().any(|w| w.percent_used.is_some()) =>
                    {
                        Phase::ErrorCached
                    }
                    Phase::ErrorNoCache | Phase::ErrorCached => Phase::ErrorNoCache,
                    Phase::Loading | Phase::Refreshing => {
                        return Err(invalid("refresh requires a terminal state"));
                    }
                    other => other,
                };
                provider.completed.insert(provider.pending.take().unwrap());
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
}
