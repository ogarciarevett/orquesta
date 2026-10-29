# Coding standards

Judgement rules for reviewers. Implementers do not need to read this file; reviewers apply it to every diff. Mechanical checks are `cargo test --workspace --locked` and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

## 1. Reuse first

Before writing new code, look in the shared reusable core (`crates/orquesta-orchestration`; the native app and the `ui/` code call it instead of re-implementing mailbox, run or delivery logic) and reuse or extend it. Duplicating logic that already lives there is a review finding.

## 2. Branching

The nested-ternary and `else if` rule targets TypeScript. Rust code: the clippy lint `comparison_chain` is enabled in the workspace `Cargo.toml`; prefer `match` over `else if` chains. The plain JavaScript under `ui/` follows the same human rule (no nested ternaries, `switch` or a lookup map for one-value branching) in new code. It is not linted yet because older code there still nests ternaries; reviewers check it by eye.

## 3. Validate first, at the boundary

Parse the whole input against a schema once, at the entry point, and fail with one typed validation error. Reject a function that checks fields line by line and returns or throws a different ad hoc error for each.

## 4. Schema libraries

Rust uses `serde` plus a validation crate (`garde` or `validator`) or typed newtypes whose constructors validate. Parse into the validated type at the boundary and pass only that type inward. Do not add a crate just to satisfy this rule: a newtype with a checked constructor is enough for a small shape. The IPC `Request` and `Response` are versioned `serde` types; new commands validate into typed input before dispatch. The `ui/` JavaScript has no build step and no dependencies, so Zod is skipped there; the Rust side validates what the UI sends.

## 5. Glossary

`GLOSSARY.md` is required and kept current. Every domain term with a collision risk is defined there. In this codebase "contract" means a DTO or schema (a request or response shape), not a smart contract. Reviewers flag any undefined term a diff introduces.

## 6. Unit tests ship with the code

Every added piece of code ships with unit tests, using fixtures or mocks where needed. Rust tests live in the same file, in `#[cfg(test)] mod tests`. UI tests are separate Python files next to `ui/` (`test_*.py`).

A diff that adds logic without a unit test is a review FAIL.
