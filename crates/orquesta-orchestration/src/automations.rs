// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
use crate::{Mailbox, Result, invalid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schedule {
    Interval {
        every_seconds: u64,
        anchor_utc_seconds: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AutomationDraft {
    pub name: String,
    pub prompt: String,
    pub project_id: String,
    pub workspace_id: String,
    pub schedule: Schedule,
}

impl AutomationDraft {
    fn validate(&self) -> Result<()> {
        let Schedule::Interval {
            every_seconds,
            anchor_utc_seconds,
        } = self.schedule;
        if self.name.trim().is_empty()
            || self.name.trim().len() > 120
            || self.prompt.trim().is_empty()
            || self.prompt.len() > 32 * 1024
            || !(60..=31_536_000).contains(&every_seconds)
            || anchor_utc_seconds > 253_402_300_799
        {
            return Err(invalid(
                "invalid automation draft; check name, prompt and interval limits",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AutomationDefinition {
    pub id: String,
    pub draft: AutomationDraft,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AutomationSnapshot {
    pub revision: u64,
    pub definitions: Vec<AutomationDefinition>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Catalog {
    schema: u8,
    revision: u64,
    counter: u64,
    definitions: Vec<AutomationDefinition>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            schema: 1,
            revision: 0,
            counter: 0,
            definitions: Vec::new(),
        }
    }
}

impl Catalog {
    fn snapshot(&self) -> Result<AutomationSnapshot> {
        if self.schema != 1
            || self.definitions.len() > 128
            || self.counter != self.definitions.len() as u64
        {
            return Err(invalid("unsupported or invalid automation catalog"));
        }
        for (index, definition) in self.definitions.iter().enumerate() {
            if definition.id != format!("automation-{}", index + 1) {
                return Err(invalid("invalid stored automation ID"));
            }
            definition.draft.validate()?;
        }
        Ok(AutomationSnapshot {
            revision: self.revision,
            definitions: self.definitions.clone(),
        })
    }
}

impl Mailbox {
    pub fn automation_snapshot(&self) -> Result<AutomationSnapshot> {
        self.transaction(|state| state.automations.snapshot())
    }

    pub fn automation_save(
        &self,
        expected_revision: u64,
        id: Option<String>,
        mut draft: AutomationDraft,
    ) -> Result<AutomationSnapshot> {
        self.transaction(|state| {
            let catalog = &mut state.automations;
            catalog.snapshot()?;
            if catalog.revision != expected_revision {
                return Err(invalid(
                    "automations changed; reload snapshot before retrying",
                ));
            }
            draft.validate()?;
            if !state.sidebar.snapshot().projects.iter().any(|project| {
                project.id == draft.project_id && project.workspace_id == draft.workspace_id
            }) {
                return Err(invalid("unknown automation project/workspace pair"));
            }
            draft.name = draft.name.trim().into();
            let revision = catalog
                .revision
                .checked_add(1)
                .ok_or_else(|| invalid("automation revision exhausted"))?;
            if let Some(id) = id {
                let definition = catalog
                    .definitions
                    .iter_mut()
                    .find(|definition| definition.id == id)
                    .ok_or_else(|| invalid("unknown automation ID"))?;
                definition.draft = draft;
            } else {
                if catalog.definitions.len() >= 128 {
                    return Err(invalid("automation draft capacity reached"));
                }
                let counter = catalog
                    .counter
                    .checked_add(1)
                    .ok_or_else(|| invalid("automation ID counter exhausted"))?;
                catalog.definitions.push(AutomationDefinition {
                    id: format!("automation-{counter}"),
                    draft,
                });
                catalog.counter = counter;
            }
            catalog.revision = revision;
            catalog.snapshot()
        })
    }
}
