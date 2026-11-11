# Development Rules for bogita

## Commands

```bash
cargo nextest run                                       # unit tests only (fast)
cargo nextest run --features integration                # unit + integration
cargo nextest run --features integration --profile ci   # full suite, CI output
cargo build --workspace
cargo clippy --workspace --all-features --tests -- -D warnings
cargo fmt -- --check
```

Clippy warnings are build failures.

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

### External State Isolation

Debug and production state are separated at compile time by the `cfg(debug_assertions)` gate, which
is compiled into exactly one place: the `APP_NAME` const
(`crates/core/src/storage/config.rs:18-22`). Every other external-state touchpoint - vault database,
identity file, config, logs, sockets - derives its identifying string from that one site, so a debug
binary cannot open the production database or socket by default - only an explicit path override in
`config.toml` can cross the line.
`[profile.release-with-debug]`
(`Cargo.toml:21-24`) inherits `release`, which leaves `debug_assertions` off, so a
release-with-debug build runs production state by design, not as a dev build. The keychain that was
the second gate site is gone with the daemon-held lock: no credential store carries state now.

- Any new external-state touchpoint - file path, socket path, temp artifact - derives its
  identifying string from the `APP_NAME`-gated consts in `crates/core/src/storage/config.rs:18-22`.
  Never hardcode `bogita` or `bogita-dev` elsewhere.

The agent socket is the cautionary precedent: a first version never passed through the gate, was
dead stub wiring, and was deleted in commit 72ca50f. The reinstated socket derives its path from
the gated consts (`crates/core/src/agent/socket.rs`, `default_socket_path`).

### SSH Agent Lock Interlock

The daemon owns the lock, and the lock is a control request rather than a file on disk. `bogita
vault unlock` sends the passphrase over the agent socket (`UNLOCK_EXTENSION`); the daemon derives
the identity from the encrypted identity file, rebuilds the key ring from the vault, and keeps the
identity in memory. `bogita vault lock` sends `LOCK_EXTENSION`; the daemon drops the identity, clears
the ring, and forbids a rebuild. The TUI drives the same service in-process and sends the lock
request on its way out. Nothing is staged, nothing is deleted, and no process reads a credential
store.

- Locked: the ring is empty and rebuilds are refused, so every request answers FAILURE(5).
- Unlock: the ring is rebuilt from the vault and the derived identity stays as the service's
  unlocked state, which is what entry reads and writes need. That residency lasts from unlock to
  lock and is the recorded exposure change: the identity is not on disk, but it is live in the
  daemon process for that window.
- Unlock is the only way into the ring. A lock request, the idle cap, and every TUI exit all reach
  the same `VaultService::lock`, and a rebuild after a lock needs a new unlock request.
- `lock_timeout = Some(n)`: idle eviction is the same lock, not a ring-only reset. After `n` seconds
  without a served request `IdleWatch` (`crates/core/src/agent/unlock.rs`) calls
  `VaultService::lock`, so an idle-evicted daemon holds no key material in memory. The cap is
  defeat-proof against request-driven re-population: only an unlock request permits a rebuild, so a
  request storm cannot revive an evicted ring. `lock_timeout = None` never evicts on idleness.
- A request that already holds the ring finishes and returns its signature. Requests that arrive
  after a lock answer FAILURE(5), with no detection delay: the lock is a state change, not a
  polled observation.
- `grep -rn "store_identity\|delete_identity\|KeychainStore\|KeychainAdapter\|keychain_service\|keyring::Entry" crates/` must stay empty.

## SSH Agent daemon

Bogita serves SSH keys held in the vault over its own RFC 9987 unix-socket daemon. Keys are
never exported: IDENTITIES_ANSWER carries public blobs only, and the daemon answers list
and sign requests from an in-memory ring built at unlock time.

- Code: `crates/core/src/agent/` holds eleven files: `mod.rs` (exports), `client.rs`
  (blocking control-channel and identity-list client), `control.rs` (control-extension
  names, status payload, stop-and-drain handle), `error.rs` (`SshAgentError`), `keyring.rs`
  (unlock-time cache, zeroize on drop, signing), `server.rs` (serve loop, frame cap, peer
  gate, control extensions, termination watch), `server/session.rs` (one connection: list,
  sign, and the four control extensions), `signals.rs` (SIGINT and SIGHUP handlers
  that raise the stop flag), `socket.rs` (path derivation, 0700/0600 hardening, the parent
  heal of a too-open data dir, the peer-uid gate), `source.rs` (where a ring rebuild reads
  entries from), `unlock.rs` (the `VaultService` trait, the `VaultLock` service, the
  `NoLock` stub, and the `IdleWatch` idle cap). CLI:
  `crates/cli/src/handlers/agent.rs` holds the four lifecycle commands and the detached
  spawn; `crates/main/src/main.rs` holds the hidden `--internal` child entry point, the
  git-helper intercept, and the dispatch to the CLI handlers; `crates/main/src/agent.rs`
  composes the daemon: it builds the `VaultLock` service, spawns `IdleWatch`, and serves.
  The SIGINT/SIGHUP mapping
  does not live in `main.rs`: `signals.rs` installs the handlers and `watch_termination`
  in `server.rs` maps the flag onto the stop path, both started by `serve_with_signals`,
  which the child calls.
- Deep dive: the c5t research notes under 42e9c530 (tagged `ssh-agent`).
- Algorithm support matrix — serve: Ed25519 and RSA (rsa-sha2-256/512); ECDSA unsupported — roadmap.
  `algorithm_supported` in `crates/core/src/agent/keyring.rs` owns the matrix, and
  `ALGORITHM_MATRIX` beside it is the sentence README and AGENTS quote. `bogita entry serve
  <name> --on` refuses an off-matrix key at opt-in and prints the entry, the algorithm, and
  the matrix. `load_entries` skips an off-matrix key that another client opted in, and its
  warning names the entry and the algorithm. Skips are never silent.
- RSA contract: rsa-sha2-256 and rsa-sha2-512 only; plain `ssh-rsa` (SHA-1) will never be
  supported, because OpenSSH removed it and bogita offers no legacy fallback.
- RSA signing: `KeyRing::sign` dispatches per algorithm. The RSA arm maps the SIGN_REQUEST
  flags with `rsa_hash_alg` (`0x4` ⇒ rsa-sha2-512, everything else ⇒ rsa-sha2-256) and signs
  through `rsa::pkcs1v15::SigningKey`, because `ssh-key` 0.6.7 has no hash-parameterized RSA
  signer and its own conversion passes `[p, p]` as the primes, which fails validation.
  `ssh-key` is built with the `ed25519` and `rsa` features; `bogita entry keygen` stays
  Ed25519-only.

Wire surface, as shipped (`ssh-agent-lib` 0.6 decodes exactly these numbers):

| Message | Reply |
| --- | --- |
| REQUEST_IDENTITIES (11) | IDENTITIES_ANSWER (12): one public blob plus comment per served key |
| SIGN_REQUEST (13) | SIGN_RESPONSE (14); FAILURE (5) while locked or when the key is not cached |
| EXTENSION (27) `bogita-status@bogita` | EXTENSION_RESPONSE (29) with `<N> keys, <locked\|unlocked>` |
| EXTENSION (27) `bogita-unlock@bogita` | SUCCESS (6) once the ring serves the vault; FAILURE (5) on a refused passphrase. The details carry the passphrase and are never echoed |
| EXTENSION (27) `bogita-lock@bogita` | SUCCESS (6); the daemon drops the identity, clears the ring, and forbids a rebuild |
| EXTENSION (27) `bogita-stop@bogita` | SUCCESS (6), then a bounded drain and socket unlink |
| any other EXTENSION (27) name | EXTENSION_FAILURE (28) |
| any other decodable number (17-23, 25, 26) | FAILURE (5) |

A number the codec cannot decode closes the connection without a reply: that is the
library's decode-time behaviour, not a handler decision.

G2 (external state): the socket path is `default_socket_path()` =
`$XDG_DATA_HOME/bogita[-dev]/agent.sock`, derived from the `APP_NAME` gate at
`crates/core/src/storage/config.rs:18-22`. Debug builds serve a `-dev` socket only; never
hardcode the path. `--socket <path>` is an explicit operator override on `agent start`;
`stop`, `status` and `keys` always use the derived path.

Lock and timeout semantics: see SSH Agent Lock Interlock above. In one line each: lock
drops the held identity, clears the ring, and forbids a rebuild, so every later request
answers FAILURE (5); unlock derives the identity, rebuilds the ring from the vault, and is the
only request that permits a rebuild; `lock_timeout = Some(n)` applies the same lock after `n`
idle seconds, so revival needs a new unlock request.

Git signing helper (S7). `bogita` doubles as git's `gpg.ssh.program`. The binary answers
git's `-Y` argument line before clap and forwards every verify shape to the system
`ssh-keygen`, so `allowed_signers` handling stays OpenSSH's. Hidden subcommands:
`ssh-sign-helper` (for wrapper scripts) and `ssh-default-key` (for
`gpg.ssh.defaultKeyCommand`). The helper is a CLIENT of the daemon socket: the lock gate
runs first, then the sign shape reads the requested public key from git's key file, asks
the daemon for its identities, matches on the wire form, relays one SIGN_REQUEST, and
writes the SshSig armor git expects to `<data-file>.sig`. `ssh-default-key` answers from
the same identities reply: zero served keys prints `no agent-servable keys`, several
prints `multiple agent-servable keys; set user.signingKey`, one prints its public line.
RSA requests carry flag `0x0000_0004` so the reply is rsa-sha2-512; Ed25519 sends zero.
Code: `crates/cli/src/handlers/ssh_relay.rs` (the relay and its `AgentRelay` port),
`crates/cli/src/handlers/ssh_sign.rs` (gate, router, forward, messages), and
`crates/main/src/ssh_helper.rs` (the socket glue). Four rules hold: the helper NEVER
prompts, it NEVER writes state other than the signature file git asked for, it NEVER holds
private key material, and it NEVER reaches the vault. A locked vault or an absent daemon
exits 1 with `vault is locked; run bogita unlock`; a key the ring does not serve exits 1
with `no vault key matches user.signingKey`.

TUI: v1 ships no agent surface - no status pane, no leader-mode binding. The daemon is a
separate process whose state the TUI does not own; the TUI holds its own vault service. The
one crossing is the exit lock, which sends the daemon's `LOCK_EXTENSION` so a TUI quit stops
the daemon serving too. Deferred, revisit after v1.

## Testing Philosophy

### TDD: RED, GREEN, REFACTOR

1. **RED** - Write a failing test first
2. **GREEN** - Write minimal code to make the test pass
3. **REFACTOR** - Clean up and improve the code

Never write production code without a failing test first.

### Test Organization

**No inline tests.** Tests live in a `test/` directory at each crate root, mirroring the
`src/` module hierarchy:

```
crates/<crate>/
  src/
    lib.rs
    vault.rs
    storage/
      mod.rs
      sqlite.rs
  test/
    lib.rs
    vault.rs
    storage/
      sqlite.rs
```

- Test files in `src/` are forbidden - no `*_test.rs` or `test.rs` file may sit next to
  production code
- File naming follows the production module shape:
  - Single-file module: `src/foo.rs` with `test/foo.rs`
  - File inside a directory module: `src/dir/foo.rs` with `test/dir/foo.rs`
  - Multi-file module: `src/foo/mod.rs` with `test/foo/test.rs`
  - Forbidden: the mixed pairing `src/foo.rs` + `test/foo/test.rs`
- Test support files (`support.rs`, `helpers.rs`, `fixtures.rs`) live in `test/`, never in `src/`
- Each production file declares its own test module with `#[cfg(test)]` and `#[path]`.
  `#[path]` resolves relative to the declaring file, so the path must escape `src/`:

```rust
// region:    --- Tests
#[cfg(test)]
#[path = "../test/foo.rs"]
mod foo_test;
// endregion: --- Tests
```

- From a subdirectory file (`src/dir/foo.rs`) add one more `../`:
  `#[path = "../../test/dir/foo.rs"] mod foo_test;`
- From a module file (`src/foo/mod.rs`) declare the entry point:
  `#[path = "../../test/foo/test.rs"] mod test;`
- Do not write `#[path = "test/foo.rs"]` from a file in `src/` - that resolves to
  `src/test/foo.rs`, not the crate-root `test/` directory
- The `mod` name keeps the `_test` suffix and is independent of the file path
- **Test file size limit:** no test file exceeds 500 lines. When a file reaches the limit, split it
  into topical subfiles in a subdirectory of the same name (e.g. `test/parser.rs` ->
  `test/parser/lifecycle.rs`, `test/parser/errors.rs`). Shared support code stays in
  `test/parser/support.rs`; the production module declares each split file as its own
  `#[cfg(test)]` module

### Test Categorization

Tests split into **unit** and **integration** categories.

- **Unit tests** are pure logic: no network, no real server, no database, no keychain, no config
  read from disk
- **Every unit test completes in under 1 second**
- A test is an integration test - regardless of which file it lives in - when it binds a TCP port,
  performs network I/O, opens a database, loads a system keychain, resolves config from disk, or
  exceeds the 1-second limit
- Integration test modules are gated behind the `integration` cargo feature:

```rust
#[cfg(all(test, feature = "integration"))]
#[path = "../../test/storage/sync/test.rs"]
mod test;
```

- The `integration` feature is defined in `crates/core/Cargo.toml`, `crates/cli/Cargo.toml`, and
  `crates/tui/Cargo.toml`. Enabling it on `bogita-cli` or `bogita-tui` propagates to `bogita-core`
- Unit tests MUST NOT call integration-level constructors. A function that opens a database, builds
  a real HTTP client, or probes a keychain is integration-level; a unit test that reaches it
  directly or transitively is an integration test in disguise. Inject a mock builder instead

`cargo nextest` is the test runner. Profiles live in `.config/nextest.toml`:

| Profile | Purpose |
|---------|---------|
| `default` | Unit tests only - fast feedback, high parallelism |
| `integration` | All tests - lower parallelism, one retry for flaky cases |
| `ci` | Full suite for CI - no retries, JUnit XML output |

Test-gate invocations in specs, dev lanes, CI and hooks use cargo nextest — never
cargo test, not even as a first pass; the default and .#ci nix shells both provide
cargo-nextest; a missing runner fails the gate loudly instead of degrading.
Verification probes of nix shells must pass --ignore-environment (ambient PATH
pollution masks missing tools).

### No test-only code in production

**`#[cfg(test)]` on a `fn` or on an `impl` block in a production file is banned.**

- NO: `#[cfg(test)]` on any `fn` in a `.rs` file outside `test/`
- NO: `#[cfg(test)]` on any `impl` block in a `.rs` file outside `test/`
- YES: `#[cfg(test)]` on `mod` declarations only, e.g.
  `#[cfg(test)] #[path = "../test/foo.rs"] mod foo_test;`
- NO: test-only accessor methods that expose private fields solely for tests
- NO: making a field or method `pub(crate)` just so tests can peek at internal state
- YES: design the public API so tests use the same production methods

If a test needs private state, one of these is wrong: the test is asserting implementation details,
or the design hides behavior that should be observable. Fix the test or the API.

**Pre-existing violations are still violations.** Do not fix them inline while working on an
unrelated task - create a separate task for the cleanup.

### Test Through the Public Boundary

- Tests verify behavior through the real public API, not through internal types or methods exposed
  for testing
- No private helper made public just for tests - shared logic stays private and is tested indirectly
  through observable output
- `pub(crate)` on a field or method so tests can read it is the same smell as a `#[cfg(test)]`
  accessor

### Banned Test Anti-Patterns

These patterns verify what the compiler already enforces. They inflate the test count and test no
behavior.

- NO: compile-time trait-bound assertions - `fn assert_send<T: Send>() {}` /
  `assert_send::<MyType>()`. If the type were not `Send`, the crate would not compile
- NO: `assert_sync` / `assert_sink` / `assert_<Trait>` probe tests - same tautology; the compiler
  enforces trait conformance at every call site that uses the bound
- NO: tests that only assert a type implements a trait, without calling a method and checking the
  result
- NO: tests with empty or trivial bodies - every test executes at least one function call and
  asserts at least one observable result
- NO: tests that re-verify compiler guarantees - `Send`, `Sync`, `Unpin`, trait bounds, `Clone`,
  `Default`

What to write instead:

- YES: call the function under test with real arguments and assert the return value or the observable
  side effect
- YES: use DI mocks (trait implementations in the test file) to inject controlled inputs and capture
  outputs
- YES: every test is behavioral - it fails when the implementation is wrong, passes when it is
  right, and is deleted when it passes regardless of the implementation

### Mocking

**Mocking is mandatory, not optional.**

- Mock every external dependency: keychain, database, clipboard, network, clock
- The mock lives in the test file, not in production code
- No `#[cfg(test)]` backdoors in production code to enable mocking
- A unit test that constructs a real database or probes a real keychain is misfiled - inject a mock
  or move the test to an integration module

### Dependency Injection for Testability

**Every external dependency MUST be injected via a trait so unit tests can substitute a mock.**
This is a hard requirement derived from the Dependency Inversion Principle.

- NO: a constructor that hardcodes a concrete function performing I/O
- YES: define a trait and store it on the struct; production uses the real impl, tests use a mock
- NO: a unit test that opens a database, probes a keychain, or touches the network
- NO: `pub(crate)` fields or test-only accessors to expose internal state
- YES: design the public API so tests use the same methods as production code

**The DI boundary is where the trait object lives.** A trait object at a crate boundary or at a
constructor injection point is the boundary, not internal code. In bogita the port traits are the
injected boundary: handlers and services are generic over them, so tests pass a mock implementation
with no dynamic dispatch.

### Test Structure (rust10x conventions)

```rust
#[test]
fn test_module_function_variant() -> Result<()> {
    // -- Setup & Fixtures

    // -- Exec

    // -- Check
    Ok(())
}
```

- `type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>` at the top of every test
  module
- Test naming: `test_[module_path_name]_[function_name]_[variant]`
- Section comments in every test: `// -- Setup & Fixtures`, `// -- Exec`, `// -- Check`
- Use `// -- Exec & Check` when exec and checks run inside a `for` loop
- No `unwrap()` - use `.ok_or("should be ...")?`
- Test support functions at the end under `// -- Test Support` (inline) or
  `// region: --- Test Support` (dedicated file)
- For temp data files use paths like `tests-data/.tmp/test_function_name/`

### Manual Verification

When a task requires manual verification (terminal behavior, TUI rendering, leader-mode dispatch),
run the binary in a real terminal and verify by hand. Do NOT:

- Write nushell scripts with `job spawn`, `job send`, `job kill` to automate terminal interaction
- Write shell scripts that spawn the binary in a PTY and poll for exit
- Invent test automation that is not in the task spec

Manual verification means you run the command, observe the result, and report it. If a behavior
cannot be verified manually, flag it - do not script around it.

### Task Scope Discipline

Do exactly what the task spec says. If the task says "write tests", write tests. If the task says
"manual verification", verify manually. Do not invent additional work, scripts, or tooling outside
the task's INCLUDED scope. Work discovered outside the scope gets flagged for the orchestrator, not
done inline.

## Code Quality

### File Size Limits

- **Source file size limit:** no production source file exceeds 500 lines. When a file reaches the
  limit, split it into topical submodules in a subdirectory of the same name (e.g. `src/vault.rs` ->
  `src/vault/create.rs`, `src/vault/unlock.rs`). Shared support code stays in
  `src/vault/support.rs`; declarations and reexports live in `src/vault/mod.rs`. The limit applies
  going forward - it is not retroactively enforced on existing modules
- **Test file size limit:** 500 lines, split topically as described in Test Organization

### Functions, Names, State

- Keep functions small and focused
- Use meaningful test names that describe what they verify
- Each test verifies one behavior
- Refactor only when tests are green
- No hidden global mutable state - use explicit state via structs

### Constructor Patterns

Follow the rust10x fluid API conventions:

- `Default` for the empty synchronous constructor - do not write `new() -> Self` with no arguments
- `new(...)` for the primary constructor with obvious common arguments
- `from_...(...)` for secondary constructors
- `with_...(self, ...) -> Self` for fluent chainable setters (consuming pattern, not `&mut self`)
- `append_...(self, ...)` for adding single items, `append_...s(self, ...)` for `IntoIterator`
- `set_...(&mut self, ...)` for property setters
- Use `impl Into<...>` for constructor and setter parameters where appropriate
- A separate `Builder` with `.build()` only when construction is complex, requires validation, or is
  fallible (returns `Result`)

### Method & API Design

These laws bind every crate.

**Behavior lives on the owner of the data.**
- NO: free functions that hug properties - reading another type's fields, combining fields, or
  forwarding to a method (a `combine_foo(x: &X)` beside `X`)
- YES: a method on `X` (or on the trait `X` implements), unless the function is a pure standalone
  transform owning no state

**Names describe behavior, never shape.**
- NO: names encoding parameters, options, or call variants
- NO: builder seams named by origin
- YES: verb + receiver on the owning type - `run(storage, config)`; builder seams named by role
  (`with_event_sink`, `with_control_intake`). Origin words never appear in core naming

**No half-built configs or constructors.**
- NO: `Config::new(one_param)` with the rest behind builders - half-real at construction, no signal
  for what is missing
- YES: completeness at build time - every required part is a constructor parameter (omission is a
  compile error), or the constructor takes nothing: `Config::default()` with typed defaults

**Fewer functions, composable traits.**
- NO: variant-function families, second ways in, companion free-function modules beside a trait
- YES: one well-named method per behavior; a second entry point is a trait seam, not another
  function; the public function count tends to the minimum

### Code Structure (rust10x regions)

Organize source files in this order:

1. Primary public types
2. Supporting public types
3. Public functions
4. Public inherent implementations (Constructors, Chainable setters, Accessors)
5. Trait implementations (separate `impl` blocks)
6. Private support functions and types
7. Test module declarations

```rust
// region:    --- Types
// endregion: --- Types

// region:    --- Froms
// endregion: --- Froms

// region:    --- Support
// endregion: --- Support

// region:    --- Tests
// endregion: --- Tests
```

Group `From` implementations in a `Froms` region. Put private helpers in a `Support` region. Do not
create regions merely to label every `impl` block.

### Module Files

Use `mod.rs` for module declarations and intentional reexports. Put them in a `Modules` region after
any module-level documentation:

```rust
//! Module-level documentation.

// region:    --- Modules

mod support;

pub use support::*;

// endregion: --- Modules
```

Implementation details live in dedicated source files, not in `mod.rs`.

### Error Handling

- Use `thiserror` for error enums
- Define `pub type Result<T> = core::result::Result<T, Error>` in `error.rs`, re-export from
  `lib.rs`
- `Custom` variant with `#[from(String, &String, &str)]` so `.ok_or("message")?` works
- External error types below the `// -- Externals` comment
- Application-specific variants above it, with `Custom` always on top

### Modern Rust (Edition 2021)

bogita is on edition 2021 (`rust-version = "1.75"`). Two rules from the newer edition do NOT compile
yet and are deferred to the edition 2024 upgrade:

- **Deferred:** if-let chains (`if let Some(x) = foo && x > 0 { ... }`) - not available on edition
  2021
- **Deferred:** async closures (`async || {}`) - not available on edition 2021

Rules that apply now:

- Inline format args: `println!("{name}")`, not `println!("{}", name)`
- Avoid `ref` bindings - rely on match ergonomics
- Avoid manual pattern matches when a slice or char-set pattern works:
  `line.trim_start_matches([' ', '\t'])`, not `|c: char| c == ' ' || c == '\t'`

### Nested `if` Statements

Never nest `if` inside `if` when the conditions can be flattened. On edition 2021, use an early
`let ... else` return or a combined boolean condition:

```rust
// WRONG
if let Some(x) = foo {
    if x > 0 {
        do_thing(x);
    }
}

// CORRECT - early return
let Some(x) = foo else { return; };
if x > 0 {
    do_thing(x);
}

// CORRECT - combined condition when no binding is needed
if a && b {
    do_thing();
}
```

### No Parallel Developer Agents

NEVER run two developer agents concurrently on the same repository.
Delegate one developer task at a time. Wait for completion before the next.

### Review Before Commit

A reviewer subagent must sign off before committing. Clippy warnings are build failures.

```bash
cargo clippy --workspace --all-features --tests -- -D warnings
cargo nextest run --features integration
```

### `#[allow(...)]` Is Never a Fix

Suppressing a clippy warning is not fixing it. Fix the underlying code. The only exception is
`#[allow(dead_code)]` on a test helper that is intentionally unused - and even then, prefer deleting
the dead code.

### unwrap() / expect() Policy

- NO: `unwrap()` or `expect("...")` in production code
- NO: `unwrap()` or `expect("...")` in test or example code
- YES: `unwrap_or`, `unwrap_or_else`, `if let`, `?`, `.ok_or("should be ...")?`
- YES: `mutex.lock().unwrap()` - mutex poison is a fatal internal inconsistency and panicking is
  correct (the only exception)
- In tests use `.ok_or("should be ...")?` with
  `type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>`

### Scope Discipline

Do NOT refactor adjacent code while you are at it. Every change stays scoped to the task. If
something else needs fixing, create a new task.

## SOLID Principles

No dynamic dispatch. Use static dispatch with generics.

- NO: `Box<dyn Trait>`, `&dyn Trait`, trait objects in internal code
- YES: generics with trait bounds `T: Trait`
- EXCEPTION - exactly one: `&dyn age::Recipient` / `&dyn age::Identity` inside
  `crates/core/src/crypto/age.rs` (the same age API boundary appears in
  `crates/core/src/crypto/passphrase.rs`), required by the `age` crate API

`#[async_trait]` port-trait methods generate boxed futures internally and stay permitted until the
port traits move to native async-fn-in-trait.

- **S**ingle Responsibility: one reason to change
- **O**pen/Closed: open for extension, closed for modification
- **L**iskov Substitution: subtypes must be substitutable
- **I**nterface Segregation: many specific interfaces over one general interface
- **D**ependency Inversion: depend on abstractions, not concretions

DIP is a testability requirement, not an architecture diagram concern. If a constructor hardcodes a
concrete function that performs I/O, unit tests cannot substitute a mock - the test either becomes
an integration test (slow, fragile) or production code grows a `#[cfg(test)]` backdoor (banned). The
answer is a trait, injected.

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

### Input Keys

Text inputs insert only unmodified printable chars. SHIFT-modified chars insert too, because a
terminal reports them as the shifted char with SHIFT set. Ctrl, Alt, Super, and Meta chords insert
nothing and stay free for the app and for the input widget. Ctrl-c remains the app-global quit.

Bracketed paste is enabled while the TUI runs and disabled on every exit path, so a paste arrives
as one atomic event instead of a key storm. SshKey values and Notes keep their line structure
(normalized to LF); every single-line field (name, field key, other values, vault name, search,
passphrase) turns each line break into one space; the type slot ignores pastes.

- Vault form: the name field rejects modified chars; Alt-t flips the default-vault checkbox and
  Space is inert on that cell.
- Lock screen: the passphrase rejects modified chars. Esc quits, Enter unlocks, Backspace deletes.
- Search box: the query rejects modified chars. Esc closes, Enter commits, Backspace deletes.
- Entry form: every printable char and Space types into the focused name/key/value/notes input and
  fires no action. Management actions sit behind Alt, so a focused input owns the whole bare
  keyboard.

  | Chord   | Action                                                    |
  |---------|-----------------------------------------------------------|
  | Alt-s   | Save from any slot, including the type slot and its popup  |
  | Alt-n   | Append a field row (any slot)                             |
  | Alt-d   | Remove the focused field row (key/value/type slots)        |
  | Alt-g   | Password generator (obscured value slot, non-SshKey entry) |
  | Alt-r   | Toggle reveal of the focused obscured value                |
  | Alt-v   | Open the vault picker                                      |
  | Alt-t   | Flip the agent-servable/boolean value cell                 |

  Enter submits the form from a text slot and opens the type dropdown from the type slot. On the
  type slot, j/k change the type with the popup closed, and Space changes nothing. Because Enter
  belongs to the dropdown there, the type slot advertises Alt-s as its save key. Esc in edit mode
  opens the save/discard modal (`s` save, `d` discard, `b`/Esc back); in add mode a dirty Esc still
  asks before discarding the typed rows. On both modals Enter repeats the primary action: `s` on
  the save modal, `b` on the discard modal. The delete modals keep `d`/`c`/Esc only.

### Keybindings Are Generated

Every footer hint and the modal hint rows come from the per-surface tables in
`crates/tui/src/app/bindings.rs`. Hand-editing a rendered hint string is a defect: change the
table. `?` opens the keybindings overlay for the focused surface (the whole registry on Main),
with `j`/`k` to scroll and `?`/Esc/`q` to close; the drift test
(`crates/tui/test/app/hints_drift.rs`) fails when a hint stops matching its table or when a hint
literal reappears in `render.rs` or `modals.rs`.

The predicate lives in `crates/tui/src/views/mod.rs` (`inserts_text`); every hand-rolled input
calls it.

Value rules validate in realtime: an SshKey value must parse as an OpenSSH PEM (a closed armor is
judged immediately, half-typed armor once the slot is left) and a Totp secret must be base32. An
invalid value marks its value cell with ⚠ and blocks every save chord (Enter, Alt-s, `s` in the
modal); the form summary names the field and the fix, and nothing reaches storage.

### ActiveView State Machine

Main, Form(EntryForm), VaultForm(VaultForm), ConfirmSave, ConfirmDelete, PasswordGen.
All key handling is synchronous. Async mutations deferred to pending and flushed after each keypress.

### Exit Path

Every exit locks the vault — the loop end is the only exit tail:

- Quit keys: `q`/`Q` in the main view and Esc on the lock screen set `RunState::Quit`. Ctrl-C
  arrives as a key event, not SIGINT, because raw mode owns the terminal; `keys.rs` maps it to
  the same state.
- Signals: `crates/tui/src/app/signals.rs` installs record-only handlers for SIGINT, SIGHUP and
  SIGTERM (the daemon's `signals.rs` shape plus SIGTERM). `run()` installs them and the event
  loop polls the flag on every tick, so the 250 ms `event::poll` bounds the delay.
- Order: `run()` calls `ratatui::restore()` first and then `lock_at_exit`, so a failing lock
  still leaves a usable terminal. `lock_at_exit` does two things. It calls `VaultService::lock`,
  which drops the held identity and evicts the local ring and cannot fail. It then sends the
  daemon's `LOCK_EXTENSION` over `default_socket_path()`, best effort: a missing or dead daemon
  is ignored, and the same-uid gate on the socket keeps the request from reaching another
  user's daemon.
- The lock does not depend on `lock_timeout`: the idle timer is optional, the exit lock is not.
  After the request the daemon holds no identity and answers FAILURE(5) to the next request;
  there is no poll to wait for.
- SIGKILL (`kill -9`) cannot be caught — `bogita lock` and the auto-lock timer cover that case.
