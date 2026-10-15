# Running JARVIS

The current implementation is a local Rust core, Bun model gateway, PostgreSQL,
and two clients: an interactive terminal and a responsive browser app. The browser
app works as a home-screen shortcut on Android and supports browser voice input
and spoken replies where the device/browser permits it. It is not a native Android
app and does not control other Android apps, desktop applications, or devices.

## Start on your computer

Prerequisites: the Rust toolchain in `rust-toolchain.toml`, Bun as pinned in
`gateway/package.json`, and an existing local PostgreSQL database. The database
user needs migration privileges; automated database tests also require permission
to create scratch databases. No model credentials are needed for the explicit demo.

```bash
cargo build --workspace --locked
export DATABASE_URL='postgres://YOUR_USER:YOUR_PASSWORD@127.0.0.1:5432/jarvis'
python3 scripts/run.py --demo --serve
```

Open `http://127.0.0.1:8790`. In another terminal run:

```bash
target/debug/jarvis-cli pair
```

Open the returned one-time URL or paste its code in the login form with “This is
a pairing code” checked. Codes expire after two minutes and work once. Each paired
browser gets a separate token, kept in that tab's session storage. Closing the tab
may require pairing again. Use `device.list` and `device.revoke` to revoke access.
The owner CLI reads its separate credential from the private config directory.

For a terminal session, omit `--serve`, or run `target/debug/jarvis-cli chat` while
the services are running. `/help` lists chat, memory, recovery, and connection
commands. `chat SESSION_ID` resumes a conversation created on either interface.

`init` generates owner and gateway tokens and a 32-byte encryption key under
`~/.config/jarvis/`, mode 0600, inside a mode 0700 directory. It preserves existing
files. `JARVIS_CONFIG_DIR` overrides this directory. Keep the encryption key safe:
losing it prevents decryption of existing conversations and memory. Do not commit
it or place it in database backups. This release has no automatic backup system.

## Connect a real model

The fake provider is a test fixture, clearly labeled in both clients. It cannot
answer everyday questions. To use the existing hosted transport, set
`JARVIS_PROVIDER=anthropic`, `JARVIS_MODEL` to an available model in your account,
and supply `ANTHROPIC_API_KEY` privately to the gateway. For the existing local
transport, enable `JARVIS_ENABLE_OLLAMA=1`, set `JARVIS_PROVIDER=ollama`, and set
`JARVIS_MODEL` to an already-installed model. `OLLAMA_HOST` is optional.

Then run `python3 scripts/run.py --serve` without `--demo`. The core requires an
explicit provider/model. Credentials, access, model quality, hardware, billing,
and live tool behavior must be verified with your chosen setup; adapter tests
alone do not establish a live model connection. The core does not silently switch
providers or send local context to a different hosted service.

## Android and voice

Android uses the same browser client and durable sessions as the computer. To
reach the core from your phone, configure a trusted private HTTPS reverse proxy
on your computer or private network, forwarding to `127.0.0.1:8790`. Set
`JARVIS_PUBLIC_ORIGIN=https://YOUR_PRIVATE_HOST` with no path or trailing slash,
and restart the core. Preserve the original Host and Origin headers, or forward
the configured origin. The core remains bound to loopback and requires a bearer
token for every private API command. This setting does not create a proxy, issue
a certificate, establish a VPN, or publish the service.

Run `jarvis-cli pair` and open its HTTPS URL on the phone within two minutes.
Add it to the home screen through your browser menu. The service worker caches
only the public app shell; private conversations, tokens and API responses are
never cached by it. A working connection to the core is required for chat.

“Talk” is push-to-talk dictation, with a disclosure before enabling it. Some
browsers send audio to their own speech-recognition service. Review the transcript
and press Send. Spoken replies use the device/browser speech service and are
opt-in. Stop voice cancels recording/playback. Hiding the page also stops them.
Keyboard dictation remains usable when browser recognition is unavailable.
Microphone permission, speech accuracy, audio playback and real Android hardware
have not been verified in the development container. There is no wake word,
always-on microphone, background service, or full-duplex voice guarantee.

## Read exported files

Put only deliberately exported nonsensitive UTF-8 text in
`~/.local/share/jarvis/export/`, or set `JARVIS_EXPORT_DIR` to another dedicated
export directory. Ask the model to read a named file. The client displays exact
arguments and data-egress implications before approval. The read grants one use,
expires after 60 seconds, and reads contents as they exist at dispatch.

The core requires Linux `openat2`; it rejects traversal, symlinks, hardlinks,
hidden/credential paths, special files, non-UTF-8 and files above 64 KiB. It has
no shell tool and cannot browse your general home directory.

## Connections to your services

The implementation supports owner-configured read-only MCP Streamable HTTP
connections. No external account is connected by default. Session plugins and
accounts in T3/ChatGPT do not automatically transfer into this independent bot.
Create `connections.json` inside the private config directory, mode 0600.

`connections.example.json` in the repository root is a working, verified
starting point: **GitHub**. It uses GitHub's official remote MCP server
(`https://api.githubcopilot.com/mcp/`, verified against github/github-mcp-server)
with a personal access token and a read-only allowlist.

```bash
cp connections.example.json ~/.config/jarvis/connections.json
chmod 600 ~/.config/jarvis/connections.json
# put a fine-grained, read-only PAT in a private file and point token_file at it
```

Generic shape for a service you configure yourself:

```json
[
  {
    "id": "my_service",
    "url": "https://YOUR_TRUSTED_SERVICE/mcp",
    "token_file": "/absolute/path/to/private/service-token",
    "tools": {
      "search": {
        "description": "Search my service records",
        "read_only": true,
        "schema": {
          "type": "object",
          "properties": {"query": {"type": "string", "maxLength": 500}},
          "required": ["query"],
          "additionalProperties": false
        }
      }
    }
  }
]
```

Use the service's actual endpoint, token and schema. Token files must be owned by
this user, mode 0600; credentials are never put in URLs or sent to the model.
Public unauthenticated endpoints may set `token_file` to null. Non-loopback
endpoints require HTTPS. Config is loaded at startup and cannot be changed by a
model. The owner reviews and allowlists each tool; server-provided “read-only”
annotations do not grant authority. Do not label a write tool read-only.

### Status of the five requested services

| Service | Status | What it needs from you |
| --- | --- | --- |
| GitHub | **Verified and ready.** Official remote MCP server, PAT bearer auth, read-only allowlist in `connections.example.json`. | A fine-grained read-only PAT in a private file. |
| Gmail / Google Calendar | **Not connected.** Google publishes no MCP server for these; the registry lists community servers, and those that exist speak stdio or a hosted endpoint. | Choose: run a bridge yourself, or tell me the specific hosted server you accept. |
| Discord | **Not connected.** No official server. The registry's strongest candidate (`app.lasers.guildcontrol/discord`, read-safe by design) is stdio-only. | Run an HTTP bridge for it on loopback, and a bot token. |
| Spotify | **Not connected.** Registry results for “spotify” are third-party trend APIs, not your own library; no official server. | Same choice as Gmail. |
| Home Assistant | **Not connected.** Home Assistant ships an MCP *client* integration, not a control server; community options exist. | Your Home Assistant URL (loopback HTTPS works here) plus a long-lived token. |

Two decisions gate the remaining four, and both are yours, not mine:

1. **Where the bridge runs.** stdio servers need a local HTTP proxy (for example
   `mcp-proxy`) on loopback, which this core accepts; that keeps credentials and
   message content on your machine. Hosted remote servers send your data to a
   third party you would be trusting with it.
2. **Credentials.** Tell me when each is ready and use the private token-file
   flow; do not paste tokens into chat.

`/connections` and `/check my_service` inspect configuration and negotiate/list
MCP tools. Model-proposed `connection.call` shows exact arguments, target service,
and model disclosure before dispatch. Config changes invalidate old connection
proposals. Results are untrusted data. Responses are bounded to 64 KiB, calls have
20-second deadlines, and there are no automatic retries. This release supports
MCP protocol 2025-06-18/2025-03-26 over Streamable HTTP, JSON and SSE replies,
including session IDs. It does not yet implement stdio, OAuth consent/refresh,
server-initiated requests, or write actions.

## Memory and recovery

In the terminal, `/remember preferred_name=Tony` asks you to confirm exact content;
in the browser, use Memory. Model text cannot confirm a memory. Confirmed facts
expire after 30 days; pending candidates after 24 hours. Exact confirmation of a
new value supersedes the old value. Retrieval is owner-scoped and bounded.

Forgetting a fact erases its ciphertext and tracked derived conversation payloads,
including subsequent conversations that used those histories. It also cancels
pending tasks that depended on that fact. It cannot erase information already
sent to a configured provider or independently re-entered elsewhere. Metadata
keeps only IDs and outcomes, without private bodies. Retention pruning runs at
startup; until a restart, expired records remain in storage but expired memories
are excluded from retrieval. Audit metadata expires after 90 days on pruning.

Provider failures remain `waiting_provider`; `/resume TASK_ID` explicitly retries
within the four-turn budget. Startup never silently repeats provider calls or
reads. A dispatched operation with no committed outcome is `waiting_review`,
with outcome unknown. `/review TASK_ID` creates a fresh proposal; approve it
separately. Expired proposals can also be refreshed this way. Cancellation blocks
later response publication; already-dispatched operations may still finish.

## Verification

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bun test gateway/src
node scripts/validate-phase0.mjs
node --check clients/web/app.js
# DATABASE_URL must point at an empty disposable database for the process suite:
python3 scripts/e2e.py
```

The process suite runs real core/gateway processes and PostgreSQL, with a fake
model and local MCP fixture. It covers approval binding/replay, provider
continuation, restart, memory confirmation/erasure, device pairing/revocation,
MCP allowlisting and metadata-only events. It does not qualify a live provider,
real service credentials, real Android hardware, or voice recognition.
