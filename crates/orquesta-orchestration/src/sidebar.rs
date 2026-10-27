// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
use crate::{Mailbox, Result, invalid};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::PathBuf};

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Catalog {
    revision: u64,
    folders: Vec<Folder>,
    selected: Option<String>,
    collapsed: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Folder {
    id: String,
    workspace_id: String,
    root: PathBuf,
    label: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub revision: u64,
    pub projects: Vec<Project>,
    pub selected: Option<String>,
    pub collapsed: BTreeSet<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub workspace_id: String,
    pub label: String,
    pub host_id: &'static str,
    pub kind: &'static str,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Preference {
    Select { workspace_id: String },
    Collapse { id: String, collapsed: bool },
}

impl Catalog {
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            revision: self.revision,
            projects: self
                .folders
                .iter()
                .map(|folder| Project {
                    id: folder.id.clone(),
                    workspace_id: folder.workspace_id.clone(),
                    label: folder.label.clone(),
                    host_id: "local",
                    kind: "folder",
                })
                .collect(),
            selected: self.selected.clone(),
            collapsed: self.collapsed.clone(),
        }
    }

    fn advance(&mut self) -> Result<()> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("sidebar revision exhausted"))?;
        Ok(())
    }
}

impl Mailbox {
    pub fn sidebar_snapshot(&self) -> Result<Snapshot> {
        self.transaction(|state| Ok(state.sidebar.snapshot()))
    }

    /// Only a trusted native folder picker may supply this root; never expose it as IPC.
    pub fn sidebar_add_project(&self, selected: Option<PathBuf>) -> Result<Option<Snapshot>> {
        let Some(selected) = selected else {
            return Ok(None);
        };
        let root = fs::canonicalize(selected)?;
        if !root.is_dir() {
            return Err(invalid("project must be an existing folder"));
        }
        fs::read_dir(&root)?;
        let label = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy()
            .into_owned();
        self.transaction(|state| {
            let catalog = &mut state.sidebar;
            if !catalog.folders.iter().any(|folder| folder.root == root) {
                let number = catalog.folders.len() + 1;
                catalog.folders.push(Folder {
                    id: format!("project-{number}"),
                    workspace_id: format!("folder-{number}"),
                    root,
                    label,
                });
                catalog.advance()?;
            }
            Ok(Some(catalog.snapshot()))
        })
    }

    pub fn sidebar_preference(&self, revision: u64, patch: Preference) -> Result<Snapshot> {
        self.transaction(|state| {
            let catalog = &mut state.sidebar;
            if revision != catalog.revision {
                return Err(invalid("sidebar changed; reload and retry"));
            }
            match patch {
                Preference::Select { workspace_id } => {
                    if !catalog
                        .folders
                        .iter()
                        .any(|f| f.workspace_id == workspace_id)
                    {
                        return Err(invalid("unknown workspace"));
                    }
                    catalog.selected = Some(workspace_id);
                }
                Preference::Collapse { id, collapsed } => {
                    if id != "local" && !catalog.folders.iter().any(|f| f.id == id) {
                        return Err(invalid("unknown sidebar group"));
                    }
                    if collapsed {
                        catalog.collapsed.insert(id);
                    } else {
                        catalog.collapsed.remove(&id);
                    }
                }
            }
            catalog.advance()?;
            Ok(catalog.snapshot())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Envelope, status_bar::Action};

    #[test]
    fn selected_folders_migrate_deduplicate_and_preserve_mail_and_preferences() -> Result<()> {
        let root = std::env::temp_dir().join(format!("sidebar-{}", std::process::id()));
        let directory = root.join("state");
        let folder = root.join("日本語 project");
        fs::create_dir_all(&folder)?;
        let mailbox = Mailbox::open(&directory)?;
        fs::write(
            directory.join("mailbox.json"),
            br#"{"recipients":[],"messages":[]}"#,
        )?;
        assert!(mailbox.sidebar_snapshot()?.projects.is_empty());
        mailbox.register("session:s")?;
        mailbox.send(Envelope {
            request_id: "sidebar-mail".into(),
            to: "session:s".into(),
            from: "session:s".into(),
            kind: "notice".into(),
            subject: "Unread".into(),
            body: "Keep unread".into(),
            thread: None,
            metadata: Default::default(),
        })?;
        mailbox.status_bar(Action::Visibility {
            key: "ports".into(),
            visible: false,
        })?;
        let bytes = fs::read(directory.join("mailbox.json"))?;
        assert!(mailbox.sidebar_add_project(None)?.is_none());
        assert_eq!(bytes, fs::read(directory.join("mailbox.json"))?);
        assert!(
            mailbox
                .sidebar_add_project(Some(directory.join("missing")))
                .is_err()
        );
        assert!(
            mailbox
                .sidebar_add_project(Some(directory.join("mailbox.json")))
                .is_err()
        );
        let snapshot = mailbox.sidebar_add_project(Some(folder.clone()))?.unwrap();
        assert_eq!(snapshot.projects[0].kind, "folder");
        assert_eq!(snapshot.projects[0].label, "日本語 project");
        let projection = serde_json::to_string(&snapshot)?;
        assert!(!projection.contains("root"));
        assert!(!projection.contains(root.to_str().unwrap()));
        assert_eq!(
            snapshot,
            mailbox
                .sidebar_add_project(Some(folder.join(".")))?
                .unwrap()
        );
        #[cfg(unix)]
        {
            let alias = root.join("alias");
            std::os::unix::fs::symlink(&folder, &alias)?;
            assert_eq!(snapshot, mailbox.sidebar_add_project(Some(alias))?.unwrap());
        }
        let selected = snapshot.projects[0].workspace_id.clone();
        let snapshot = mailbox.sidebar_preference(
            snapshot.revision,
            Preference::Select {
                workspace_id: selected.clone(),
            },
        )?;
        let snapshot = mailbox.sidebar_preference(
            snapshot.revision,
            Preference::Collapse {
                id: "local".into(),
                collapsed: true,
            },
        )?;
        let snapshot = mailbox.sidebar_preference(
            snapshot.revision,
            Preference::Collapse {
                id: snapshot.projects[0].id.clone(),
                collapsed: true,
            },
        )?;
        assert_eq!(snapshot.selected, Some(selected));
        assert!(
            mailbox
                .sidebar_preference(
                    0,
                    Preference::Select {
                        workspace_id: "foreign".into()
                    }
                )
                .is_err()
        );
        assert!(
            mailbox
                .sidebar_preference(
                    snapshot.revision,
                    Preference::Collapse {
                        id: "foreign".into(),
                        collapsed: true
                    }
                )
                .is_err()
        );
        let reopened = Mailbox::open(&directory)?;
        assert_eq!(snapshot, reopened.sidebar_snapshot()?);
        assert!(!reopened.inbox(Some("session:s"))?[0].read);
        assert_eq!(
            reopened.status_bar(Action::Snapshot)?["snapshot"]["visibility"]["ports"],
            false
        );
        fs::create_dir(directory.join("mailbox.pending"))?;
        assert!(
            reopened
                .sidebar_preference(
                    snapshot.revision,
                    Preference::Collapse {
                        id: "local".into(),
                        collapsed: false
                    }
                )
                .is_err()
        );
        fs::remove_dir(directory.join("mailbox.pending"))?;
        assert_eq!(snapshot, reopened.sidebar_snapshot()?);
        fs::write(directory.join("mailbox.json"), "corrupt")?;
        assert!(reopened.sidebar_snapshot().is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
