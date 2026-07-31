# Development Rules for bogita

## Commands

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --tests -- -D warnings
cargo fmt -- --check
```

## Architecture

See the architecture note in c5t (tagged architecture) for Mermaid diagrams.

### Workspace Layout

crates/core - bogita-core: domain, ports, adapters, services, vault, app
crates/cli - bogita-cli: clap args + command handlers (generic over port traits)
crates/tui - bogita-tui: ratatui app, views, context
crates/main - bogita binary: thin dispatch to cli and tui

### Port Traits (Hexagonal Architecture)

- Crypto - encrypt/decrypt (implemented by AgeCrypto)
- VaultStore - vault metadata CRUD (implemented by SqliteStorage)
- EntryStore - entry CRUD with field-level encryption (implemented by SqliteStorage)
- Storage - supertrait combining VaultStore + EntryStore (backward compat)
- SyncBackend - push/pull sync (not yet implemented)

CLI handlers are generic over S: Storage, C: Crypto + Clone.
App struct in crates/core/src/app.rs is the composition root.

## Testing Philosophy

### TDD: RED, GREEN, REFACTOR

Never write production code without a failing test first.

### Test Organization

No inline tests - Tests must be in separate _test.rs files in src/.
Single-file module: foo.rs with sibling foo_test.rs
Multi-file module: foo/mod.rs with foo/test.rs

### No Parallel Developer Agents

NEVER run two developer agents concurrently on the same repository.
Delegate one developer task at a time. Wait for completion before the next.

### Review Before Commit

A reviewer subagent must sign off before committing.
Clippy warnings are build failures. #[allow(...)] is never acceptable.

### unwrap() Policy

NO unwrap() in production code. Use unwrap_or, unwrap_or_else, if let, ?, expect(reason).
EXCEPTION: mutex.lock().unwrap() - mutex poison is fatal, panicking is correct.

### Scope Discipline

Do NOT refactor adjacent code while you are at it. Create a new task instead.

## SOLID Principles

No dynamic dispatch! Use static dispatch with generics.
NO: Box<dyn Trait>, &dyn Trait, trait objects in internal code
YES: Generics with trait bounds T: Trait

## SQLx Migrations

CRITICAL: sqlx wraps each migration in a transaction by default.
PRAGMA foreign_keys is a no-op inside a SQLite transaction.
BEGIN/COMMIT inside a migration will fail.
DROP TABLE with ON DELETE CASCADE will cascade-delete related data.

To run a migration outside a transaction, start the file with -- no-transaction
as the very first line. sqlx checks sql.starts_with for this marker.

### Migration Checklist

1. Never modify an already-applied migration (checksum will fail)
2. Back up dependent tables before rebuilding a table with FK references
3. Use -- no-transaction when you need PRAGMA control
4. Always preserve existing data - back up, rebuild, restore
5. Test migrations against a database with real data before committing

## TUI Architecture

Column-based layout: vault list, entry list, detail view or active form.

### Leader Mode

[Space] enters leader mode, then a single key dispatches an action.
Actions must be context-aware based on which column has focus:

- Vaults column focused: a = add vault, d = delete vault
- Entries column focused: a = add entry, e = edit entry, d = delete entry
- Detail column focused: c = copy field, s = reveal secret

### ActiveView State Machine

Main, Form(EntryForm), VaultForm(VaultForm), ConfirmSave, ConfirmDelete, PasswordGen.
All key handling is synchronous. Async mutations deferred to pending and flushed after each keypress.

## Examples

### Good Test Structure

Tests go in sibling _test.rs files, never inline.

### Bad Test Structure (Don't do this)

Never use #[cfg(test)] mod tests inside a source file.
