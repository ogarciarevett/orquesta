# orquesta

The language of agent orchestration with durable mailboxes.

## Language

**Run**: One orchestration session that owns an inbox address (`ORQUESTA_RUN_ID`). Not a process run.

**Mailbox**: Durable storage of addressed messages for a run, independent of any terminal.

**Inbox**: The ordered messages addressed to one recipient in a mailbox.

**Recipient**: A registered, trusted address that can receive messages; sending to an unregistered one fails.

**Delivery**: Recording a message in a recipient's inbox. Not typing it into a terminal.

**Acknowledgement**: A recipient's explicit confirmation that it handled a delivery; replay resends unacknowledged ones.

**Session composer**: The draft editing surface for a human's terminal input; it keeps drafts and submission separate from inbox notifications.

**MailEvent**: A notification that something changed in a mailbox, returned separately from a command's response.

**IPC request**: A versioned JSON request read from stdin by `orquesta ipc`; unsupported versions fail explicitly.

**Contract**: A DTO or schema: the shape of a request, response, record or message that crosses a boundary. Not a smart contract.
_Avoid_: using "contract" for an on-chain program, or for a promise of behavior (say "guarantee"). The IPC `Request` and `Response` shapes are contracts.
