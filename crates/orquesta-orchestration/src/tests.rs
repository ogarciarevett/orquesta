// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use super::*;

#[test]
fn durable_fifo_dedup_reply_and_human_draft() -> Result<()> {
    let directory = std::env::temp_dir().join(format!("orquesta-{}", std::process::id()));
    let mailbox = Mailbox::open(&directory)?;
    for address in ["run:r", "dispatch:d", "session:s"] {
        mailbox.register(address)?;
    }
    for address in ["invalid", "run:", "run:a:b", "unknown:x"] {
        assert!(mailbox.register(address).is_err());
    }
    let draft = TerminalDraftBuffer {
        text: "first line\nsecond λ line\nlast line".into(),
        cursor: 14,
        selection: Some((12, 17)),
    };
    let before = format!("{draft:?}");
    let envelope = Envelope {
        request_id: "request-1".into(),
        to: "session:s".into(),
        from: "run:r".into(),
        kind: "question".into(),
        subject: "Ready?".into(),
        body: "Review this".into(),
        thread: None,
    };
    let first = mailbox.send(envelope.clone())?;
    assert_eq!(mailbox.send(envelope.clone())?, first);
    let mut second = envelope.clone();
    second.body = "Conflict".into();
    assert!(mailbox.send(second.clone()).is_err());
    second.request_id = "request-2".into();
    let second = mailbox.send(second)?;
    let mut unauthorized = envelope;
    unauthorized.to = "session:unknown".into();
    assert!(mailbox.send(unauthorized).is_err());
    let ids = |messages: Vec<Message>| messages.iter().map(|m| m.id).collect::<Vec<_>>();
    assert_eq!(
        ids(mailbox.check("session:s", true)?),
        vec![first.id, second.id]
    );
    assert_eq!(
        ids(mailbox.check("session:s", true)?),
        vec![first.id, second.id]
    );
    assert_eq!(
        ids(mailbox.check("session:s", false)?),
        vec![first.id, second.id]
    );
    assert!(mailbox.check("session:s", true)?.is_empty());
    drop(mailbox);
    let mailbox = Mailbox::open(&directory)?;
    assert_eq!(
        ids(mailbox.check("session:s", false)?),
        vec![first.id, second.id]
    );
    assert!(mailbox.ack("dispatch:d", first.id).is_err());
    mailbox.ack("session:s", first.id)?;
    mailbox.ack("session:s", first.id)?;
    let reply = mailbox.reply("session:s", first.id, "answer-1", "Yes")?;
    assert_eq!(reply.envelope.to, "run:r");
    assert_eq!(reply.envelope.thread, Some(first.id));
    assert!(mailbox.reply("dispatch:d", first.id, "bad", "No").is_err());
    assert_eq!(ids(mailbox.check("session:s", false)?), vec![second.id]);
    assert_eq!(mailbox.inbox(Some("session:s"))?.len(), 2);
    assert_eq!(mailbox.inbox(None)?.len(), 3);
    assert_eq!(before, format!("{draft:?}"));
    drop(mailbox);
    assert_eq!(
        ids(Mailbox::open(&directory)?.check("session:s", false)?),
        vec![second.id]
    );
    fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn concurrent_retries_are_one_durable_message() -> Result<()> {
    let directory = std::env::temp_dir().join(format!("orquesta-race-{}", std::process::id()));
    let mailbox = Mailbox::open(&directory)?;
    mailbox.register("run:r")?;
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let directory = directory.clone();
            std::thread::spawn(move || {
                Mailbox::open(directory)
                    .unwrap()
                    .send(Envelope {
                        request_id: "same".into(),
                        to: "run:r".into(),
                        from: "run:r".into(),
                        kind: "status".into(),
                        subject: "Concurrent".into(),
                        body: String::new(),
                        thread: None,
                    })
                    .unwrap()
                    .id
            })
        })
        .collect();
    for worker in workers {
        assert_eq!(worker.join().unwrap(), 1);
    }
    assert_eq!(mailbox.inbox(None)?.len(), 1);
    fs::write(directory.join("mailbox.pending"), b"interrupted write")?;
    assert_eq!(Mailbox::open(&directory)?.inbox(None)?.len(), 1);
    fs::write(directory.join("mailbox.json"), b"corrupt committed state")?;
    assert!(mailbox.inbox(None).is_err());
    assert_eq!(
        fs::read(directory.join("mailbox.json"))?,
        b"corrupt committed state"
    );
    fs::remove_dir_all(directory)?;
    Ok(())
}
