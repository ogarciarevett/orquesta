// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
use orquesta_orchestration::{
    Envelope, Mailbox, Result,
    automations::{AutomationDraft, Schedule},
    sidebar::Preference,
    status_bar::Action,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::Arc};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
        let fixture = Self(std::env::temp_dir().join(format!("automation-{}-{nonce:?}", std::process::id())));
        let mailbox = Mailbox::open(&fixture.0)?;
        for name in ["first", "second"] {
            let folder = fixture.0.join(name);
            fs::create_dir(&folder)?;
            mailbox.sidebar_add_project(Some(folder))?;
        }
        Ok(fixture)
    }
    fn mailbox(&self) -> Result<Mailbox> {
        Mailbox::open(&self.0)
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.0.join("mailbox.json")).unwrap()
    }
    fn write(&self, state: &Value) {
        fs::write(self.0.join("mailbox.json"), serde_json::to_vec(state).unwrap()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn draft() -> AutomationDraft {
    AutomationDraft {
        name: "  Review  ".into(),
        prompt: " \nReview λ\n  preserve indentation\n".into(),
        project_id: "project-1".into(),
        workspace_id: "folder-1".into(),
        schedule: Schedule::Interval { every_seconds: 60, anchor_utc_seconds: 0 },
    }
}

#[test]
fn migration_reopen_and_replacement_preserve_mail_and_preferences() -> Result<()> {
    let fixture = Fixture::new()?;
    let mailbox = fixture.mailbox()?;
    mailbox.register("session:s")?;
    let message = mailbox.send(Envelope {
        request_id: "draft-unread".into(), to: "session:s".into(), from: "session:s".into(),
        kind: "notice".into(), subject: "Unread".into(), body: "Keep".into(),
        thread: None, metadata: Default::default(),
    })?;
    mailbox.status_bar(Action::Visibility { key: "ports".into(), visible: false })?;
    let sidebar = mailbox.sidebar_snapshot()?;
    mailbox.sidebar_preference(sidebar.revision, Preference::Select { workspace_id: "folder-1".into() })?;
    let mut old: Value = serde_json::from_slice(&fixture.bytes())?;
    old.as_object_mut().unwrap().remove("automations");
    fixture.write(&old);
    assert!(mailbox.automation_snapshot()?.definitions.is_empty());
    let saved = mailbox.automation_save(0, None, draft())?;
    assert_eq!(saved.definitions[0].id, "automation-1");
    assert_eq!(saved.definitions[0].draft.prompt, draft().prompt);
    assert_eq!(saved.definitions[0].draft.name, "Review");
    assert_eq!(fixture.mailbox()?.automation_snapshot()?, saved);
    let projection = serde_json::to_string(&saved)?;
    assert!(!projection.contains("root"));
    assert!(!projection.contains(fixture.0.to_str().unwrap()));
    assert!(mailbox.automation_save(0, None, draft()).is_err());
    let mut changed = draft();
    changed.prompt.push_str("Updated\n");
    let replaced = mailbox.automation_save(1, Some("automation-1".into()), changed.clone())?;
    assert_eq!(replaced.revision, 2);
    assert_eq!(replaced.definitions.len(), 1);
    assert_eq!(replaced.definitions[0].draft.prompt, changed.prompt);
    let mut after: Value = serde_json::from_slice(&fixture.bytes())?;
    after.as_object_mut().unwrap().remove("automations");
    assert_eq!(after, old);
    assert_eq!(mailbox.inbox(Some("session:s"))?, vec![message]);
    Ok(())
}

#[test]
fn concurrent_handles_commit_only_one_revision() -> Result<()> {
    let fixture = Fixture::new()?;
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2).map(|_| {
        let mailbox = fixture.mailbox().unwrap();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            mailbox.automation_save(0, None, draft()).is_ok()
        })
    }).collect();
    let successes = handles.into_iter().map(|handle| handle.join().unwrap()).filter(|ok| *ok).count();
    assert_eq!(successes, 1);
    let snapshot = fixture.mailbox()?.automation_snapshot()?;
    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.definitions.len(), 1);
    Ok(())
}

#[test]
fn invalid_drafts_and_ids_leave_committed_bytes_unchanged() -> Result<()> {
    let fixture = Fixture::new()?;
    let mailbox = fixture.mailbox()?;
    let before = fixture.bytes();
    for (field, value) in [
        ("name", json!(" ")), ("name", json!("λ".repeat(61))),
        ("prompt", json!(" \n\t")), ("prompt", json!("x".repeat(32769))),
        ("project_id", json!("project-missing")), ("workspace_id", json!("folder-2")),
        ("schedule", json!({"kind":"interval","every_seconds":59,"anchor_utc_seconds":0})),
        ("schedule", json!({"kind":"interval","every_seconds":31536001,"anchor_utc_seconds":0})),
        ("schedule", json!({"kind":"interval","every_seconds":60,"anchor_utc_seconds":253402300800u64})),
    ] {
        let mut input = serde_json::to_value(draft())?;
        input[field] = value;
        assert!(mailbox.automation_save(0, None, serde_json::from_value(input)?).is_err());
        assert_eq!(fixture.bytes(), before);
    }
    assert!(mailbox.automation_save(0, Some("automation-unknown".into()), draft()).is_err());
    assert_eq!(fixture.bytes(), before);
    Ok(())
}

#[test]
fn malformed_storage_and_obstructed_write_never_reset_state() -> Result<()> {
    let fixture = Fixture::new()?;
    let mailbox = fixture.mailbox()?;
    mailbox.automation_save(0, None, draft())?;
    let before = fixture.bytes();
    let original: Value = serde_json::from_slice(&before)?;
    for pointer in ["/automations/schema", "/automations/counter", "/automations/revision",
        "/automations/definitions/0/draft/path", "/automations/definitions/0/draft/schedule/host"] {
        let mut state = original.clone();
        if pointer.ends_with("path") {
            state["automations"]["definitions"][0]["draft"]["path"] = json!("/tmp");
        } else if pointer.ends_with("host") {
            state["automations"]["definitions"][0]["draft"]["schedule"]["host"] = json!("local");
        } else {
            *state.pointer_mut(pointer).unwrap() = json!(u64::MAX);
        }
        fixture.write(&state);
        let invalid = fixture.bytes();
        assert!(mailbox.automation_save(u64::MAX, None, draft()).is_err());
        assert_eq!(fixture.bytes(), invalid);
    }
    fs::write(fixture.0.join("mailbox.json"), "corrupt")?;
    assert!(mailbox.automation_snapshot().is_err());
    assert_eq!(fixture.bytes(), b"corrupt");
    fs::write(fixture.0.join("mailbox.json"), &before)?;
    fs::create_dir(fixture.0.join("mailbox.pending"))?;
    assert!(mailbox.automation_save(1, None, draft()).is_err());
    assert_eq!(fixture.bytes(), before);
    Ok(())
}

#[test]
fn accepted_limits_and_capacity_survive_reopen() -> Result<()> {
    let fixture = Fixture::new()?;
    let mailbox = fixture.mailbox()?;
    let mut boundary = draft();
    boundary.name = "λ".repeat(60);
    boundary.prompt = "x".repeat(32768);
    boundary.schedule = Schedule::Interval { every_seconds: 31_536_000, anchor_utc_seconds: 253_402_300_799 };
    mailbox.automation_save(0, None, boundary.clone())?;
    for revision in 1..128 {
        mailbox.automation_save(revision, None, draft())?;
    }
    let before = fixture.bytes();
    assert!(mailbox.automation_save(128, None, draft()).is_err());
    assert_eq!(fixture.bytes(), before);
    let reopened = fixture.mailbox()?.automation_snapshot()?;
    assert_eq!(reopened.definitions.len(), 128);
    assert_eq!(reopened.definitions[0].draft, boundary);
    assert_eq!(reopened.definitions[127].id, "automation-128");
    Ok(())
}
