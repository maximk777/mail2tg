# mail2tg

A Telegram bot that forwards selected emails from IMAP mailboxes into Telegram.
It polls one or more mailboxes over IMAP and forwards a message only when **both**
conditions hold:

1. the email is addressed to one of the mailbox's configured addresses
   (`targets`) — e.g. a specific DuckDuckGo `@duck.com` alias;
2. the **sender's** domain is in the `SENDER_DOMAINS` allowlist (e.g.
   `openai.com`).

DuckDuckGo rewrites the `From` header (`noreply_at_openai.com_hash@duck.com`) —
the bot decodes the original sender back to `noreply@openai.com`. Each mailbox has
its **own** list of Telegram recipients. Ships as a single small static binary
(musl/ARM), with no OpenSSL.

---

## How it works

```
IMAP mailbox(es) ──poll──> mail2tg ──filter (recipient + sender domain)──> Telegram
```

- The **daemon** (`mail2tg run`) runs two loops: a mail poller (every
  `POLL_INTERVAL_SECS`) and a Telegram command listener (`/start`).
- State (last processed UID + a baseline date) is kept in one file per mailbox in
  `STATE_DIR`, so a message is never forwarded twice.
- On a delivery failure `last_uid` is not advanced — the message is retried on the
  next cycle (a duplicate beats a lost email).
- The daemon **does not exit** when there are no mailboxes: it idles and reloads
  the config every cycle, so mailboxes added via the CLI are picked up without a
  restart.

---

## Installation

### Option A. Download a prebuilt binary (recommended for a VPS / Raspberry Pi)

Once a release is published (a `v*` tag), GitHub Actions builds binaries for
several platforms. Pick the right one and fetch it with `wget`:

| Device | Asset |
|---|---|
| VPS x86-64 | `mail2tg-x86_64-unknown-linux-musl` |
| Raspberry Pi 3/4/5 (64-bit OS) | `mail2tg-aarch64-unknown-linux-musl` |
| Raspberry Pi 2/3 (32-bit OS) | `mail2tg-armv7-unknown-linux-musleabihf` |
| Raspberry Pi Zero/1 (ARMv6) | `mail2tg-arm-unknown-linux-gnueabihf` |

```bash
wget https://github.com/maximk777/mail2tg/releases/latest/download/mail2tg-x86_64-unknown-linux-musl -O mail2tg
chmod +x mail2tg
./mail2tg --help
```

### Option B. Build from source

```bash
git clone https://github.com/maximk777/mail2tg.git
cd mail2tg
cargo build --release            # binary at target/release/mail2tg
# static build for your platform, e.g.:
# rustup target add x86_64-unknown-linux-musl
# cargo build --release --target x86_64-unknown-linux-musl
```

---

## Configuration

Settings are split in two:

- **Environment variables** — global daemon options (only needed by `run`).
- **Config files** — the mailboxes and their recipients, managed by the
  interactive CLI commands. Mailbox passwords are stored separately from the rest
  of the config (the `~/.aws/credentials` model).

### Environment variables

| Variable | Required | Default | Meaning |
|---|---|---|---|
| `TG_BOT_TOKEN` | yes¹ | — | bot token from @BotFather |
| `SENDER_DOMAINS` | yes¹ | — | comma-separated sender domains: `openai.com,anthropic.com` |
| `POLL_INTERVAL_SECS` | no | `15` | mail poll interval, seconds |
| `BODY_PREVIEW_CHARS` | no | `1000` | how many characters of the body to send |
| `STATE_DIR` | no | `./state` | directory of per-mailbox state files |
| `MAIL2TG_CONFIG` | no | `mail2tg.json` | path to the mailbox config (no passwords) |
| `MAIL2TG_CREDENTIALS` | no | `mail2tg.credentials` | path to the secrets file (mode `0600`) |
| `MAIL2TG_PIDFILE` | no | `mail2tg.pid` | daemon pid file |
| `DDG_FROM_REGEX` | no | built-in | override the DuckDuckGo `From` parser |
| `RUST_LOG` | no | `info` | log level (`error`/`warn`/`info`/`debug`) |

¹ `TG_BOT_TOKEN` and `SENDER_DOMAINS` are required **only** by `mail2tg run`. The
configuration commands (`mailbox …`, `tgid …`) and `mail2tg stop` work without
them.

A template lives in [`.env.example`](.env.example).

### Config files

- `mail2tg.json` — the list of mailboxes (host/port/user/folder/targets/whitelist),
  **without passwords**. Safe to inspect and back up.
- `mail2tg.credentials` — passwords only (`mailbox name → password`), mode `0600`.
  If the permissions are too open, the bot logs a warning.

Both files are created and edited by the `mailbox …` / `tgid …` commands — you do
not need to edit them by hand.

---

## Managing the bot (CLI)

All management commands are **interactive**: run one and the bot prompts for the
data step by step, showing the current state. The password is entered hidden and
never lands in your shell history.

```
mail2tg run                 # start the daemon (poll mail + listen for Telegram commands)
mail2tg stop                # gracefully stop a running daemon (SIGTERM via the pid file)

mail2tg mailbox add         # add a mailbox (name/host/port/user/password/folder/targets)
mail2tg mailbox list        # list mailboxes (passwords masked ****)
mail2tg mailbox remove      # remove a mailbox (with confirmation)

mail2tg tgid add            # add a Telegram recipient to a chosen mailbox
mail2tg tgid list           # list a mailbox's recipients
mail2tg tgid remove         # remove a recipient from a mailbox
```

### Typical first-run setup

1. **Create a bot** with [@BotFather](https://t.me/BotFather) and get the token.
2. **Add a mailbox** (no env needed for this):
   ```bash
   ./mail2tg mailbox add
   # Name: gmail-duck
   # IMAP host: imap.gmail.com
   # IMAP port [993]:
   # IMAP user: me@gmail.com
   # IMAP password: ********        (an app-password, not your main password)
   # Folder [INBOX]:
   # Target addresses (one per line, blank to finish):
   #   > scpccomz@duck.com
   #   >
   ```
   This creates `mail2tg.json` (no password) and `mail2tg.credentials` (`0600`).
3. **Find your Telegram ID and register.** Start the daemon (`mail2tg run` with
   the env set) and send `/start` to the bot:
   - if you are not whitelisted, the bot replies with your `<tgid>`;
   - add it: `./mail2tg tgid add` → pick the mailbox → enter `<tgid>`;
   - send `/start` again → "✅ You are registered…".
   > The daemon reloads its config every cycle, so no restart is needed after
   > `tgid add`.
4. Emails from the allowed domains addressed to a `targets` address start arriving
   as a card.

### Multiple mailboxes

Just run `mailbox add` several times. Each mailbox has its own credentials, its
own `targets`, and its own list of `tgid`s. `SENDER_DOMAINS` is shared across all
of them.

---

## Running

### Manually (for testing)

```bash
export TG_BOT_TOKEN=123456:ABC...
export SENDER_DOMAINS=openai.com,anthropic.com
./mail2tg run
# stop: Ctrl-C, or from another terminal: ./mail2tg stop
```

### As a systemd service (recommended)

A ready-made unit lives in [`deploy/mail2tg.service`](deploy/mail2tg.service).

```bash
# 1. place the binary and config
sudo mkdir -p /opt/mail2tg
sudo cp mail2tg /opt/mail2tg/
# create /opt/mail2tg/.env from .env.example (at minimum TG_BOT_TOKEN, SENDER_DOMAINS)
sudo cp .env.example /opt/mail2tg/.env && sudoedit /opt/mail2tg/.env

# 2. configure mailboxes (from /opt/mail2tg so the files land next to the binary)
cd /opt/mail2tg && sudo ./mail2tg mailbox add

# 3. install and start the service
sudo cp deploy/mail2tg.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now mail2tg

# 4. view logs / stop
journalctl -u mail2tg -f
sudo systemctl stop mail2tg     # invokes `mail2tg stop` — a graceful shutdown
```

The unit restarts the bot on failure (`Restart=on-failure`) and starts after the
network is up. You can change mailboxes/recipients live (`./mail2tg mailbox add`,
`tgid add`) — the daemon picks them up without a restart.

---

## Cutting a release

Push a `v*` tag and the [`.github/workflows/release.yml`](.github/workflows/release.yml)
workflow builds binaries for all four platforms and attaches them to a GitHub
Release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

---

## Security

- Mailbox passwords are stored separately (`mail2tg.credentials`, mode `0600`),
  are never printed, and never written into `mail2tg.json`.
- The bot token never reaches the logs (network-layer errors are redacted).
- TLS is pure Rust (rustls + ring), no OpenSSL.
