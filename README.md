# bogita

## Vault unlock

`bogita vault unlock` prompts on the terminal with echo off. Automation supplies the
passphrase from exactly one source instead:

```sh
bogita vault unlock --password '<passphrase>'        # visible in shell history and `ps`
cat pass.txt | bogita vault unlock --password-stdin  # one line from stdin
bogita vault unlock --password-file ~/.pass-bogita   # a file the owner alone can read
```

- `-p`, `--password` — the value is on the command line, so shell history and process
  listings (`ps`) show it. Prefer `--password-stdin` or `--password-file`.
- `--password-stdin` — reads one line; the trailing newline (and a preceding carriage
  return) is stripped, and nothing is echoed.
- `--password-file` — reads the file; one trailing newline is stripped and inner newlines
  are kept. Any group- or other-readable mode bit refuses the unlock and names the fix:
  `chmod 600 <path>`.

Only one source at a time: two are a usage error. A flag source never prompts, and every
failure goes to stderr with a non-zero exit code. The unlocked identity is staged in the
keychain exactly as the prompt path stages it, so `bogita agent start` works afterwards.

## SSH agent

Bogita serves vault-held SSH keys over its own agent socket (RFC 9987); keys are never
exported. The daemon reads the staged identity from the keychain, so unlock first.

```sh
bogita unlock
bogita agent start            # prints SSH_AUTH_SOCK=<path>
export SSH_AUTH_SOCK=<path>
```

- `bogita agent start` — Start the agent daemon and print its socket as SSH_AUTH_SOCK
- `bogita agent stop` — Stop the agent daemon and remove its socket
- `bogita agent status` — Show the agent status line
- `bogita agent keys` — List the keys the agent serves

```console
$ bogita agent start
SSH_AUTH_SOCK=/home/you/.local/share/bogita/agent.sock
$ bogita agent status
/home/you/.local/share/bogita/agent.sock: running, 2 keys, unlocked
$ bogita agent keys
SHA256:xTmBTxbyf0XAHfPxScS+E9CU1oeW+xg+a9fy8ZM/Yjc work@example.com
$ bogita agent stop
agent stopped
```

`agent status` exits 0 in both states (report command); `agent stop` and `agent keys` exit
1 with `agent not running` when nothing answers. `agent start --socket <path>` serves
elsewhere; the other commands use the default path. Only agent-servable keys are served
(`bogita entry serve <name> --on`).

**Git commit signing**
git execs one program for SSH signatures; point it at the bogita binary:

```sh
git config --global gpg.format ssh              # prerequisite: SSH signatures
git config --global commit.gpgsign true         # prerequisite: sign every commit
git config --global gpg.ssh.program /path/to/bogita
git config --global user.signingKey "ssh-ed25519 AAAA... work@example.com"
git config --global gpg.ssh.defaultKeyCommand "/path/to/bogita ssh-default-key"
```

- `gpg.ssh.program` holds one pathname: git execs the whole value, so the binary answers
  git's `-Y` line itself. `user.signingKey` takes the line `bogita entry pubkey` prints.
- `gpg.ssh.defaultKeyCommand` is optional; git runs it through a shell, so quote a path
  with spaces. It prints the only agent-servable key, else fails with `no agent-servable
  keys` or `multiple agent-servable keys; set user.signingKey`.
- Signing uses the matching vault key; verify is forwarded to `ssh-keygen` unchanged, so
  `allowed_signers` stays OpenSSH's. `git log --show-signature` shows the SSHSIG; a locked
  vault never prompts and exits 1 with `vault is locked; run bogita unlock`.

Daemon running: SSH logins AND git signing via SSH_AUTH_SOCK. Daemon off but vault unlocked: git commits via the gpg.ssh.program helper. Vault locked: neither — run bogita unlock first.

**Upgrade note**
The agent takes care of its data dir: it creates it 0700, narrows a pre-existing mode that
another user can reach to 0700 (one line names the dir and the modes), and refuses only
when the narrowing does not take hold. No manual `chmod` is needed.

**Limits (v1)**
- Algorithm support matrix — serve: Ed25519 and RSA (rsa-sha2-256/512); ECDSA unsupported — roadmap.
  `bogita entry serve <name> --on` refuses a key outside the matrix and prints the entry and
  the algorithm; a key another client marked servable is skipped when the daemon loads it,
  with a warning that names the algorithm. An RSA SIGN_REQUEST picks the digest through its
  flags: `0x4` answers rsa-sha2-512, any other value answers rsa-sha2-256.
- `bogita entry keygen` creates Ed25519 keys only; an imported RSA key is served, and other
  imported types (ECDSA) are stored but never served. SSH keys are unencrypted OpenSSH PEM
  inside the age-encrypted vault; the passphrase-protected form is rejected in v1.
- No launchd/systemd socket activation, no Windows named pipes, no agent forwarding, no
  per-use confirmation; the helper needs an unlocked vault and signs only, verify stays
  with `ssh-keygen`.
- Keys live in a local, age-encrypted vault with no subscription. Bogita is not a
  feature-parity replacement for 1Password.
- `SIGKILL` (`kill -9`) and equivalent cannot lock the vault — use `bogita lock` or the
  auto-lock timer. Every catchable exit locks: the TUI deletes the staged identity when it
  quits (q, Ctrl-C, or SIGINT/SIGHUP/SIGTERM), and exits 3 with a stderr warning when that
  lock itself fails.

Architecture, protocol and lock semantics: see AGENTS.md, "SSH Agent daemon".
