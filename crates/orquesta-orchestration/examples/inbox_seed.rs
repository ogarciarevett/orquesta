// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use orquesta_orchestration::{Mailbox, Result};

fn main() -> Result<()> {
    let mailbox = Mailbox::open(std::env::var("ORQUESTA_MAILBOX_DIR")?)?;
    for recipient in ["run:r", "session:s"] {
        mailbox.register(recipient)?;
    }
    Ok(())
}
