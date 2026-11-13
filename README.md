# Veil

**A local redacting reverse proxy between your tools and model APIs. Your apps keep their existing keys and login state; sensitive body values are replaced with placeholders before forwarding and restored on the way back.**

Veil is for cases such as sending configuration, logs, or `.env` contents to a coding assistant while keeping the upstream model from seeing the real password, token, connection string, or personal information.

[Download](https://github.com/guangcheng9/Veil/releases/latest) · [中文说明](README.zh-CN.md) · Apache-2.0 · no automatic telemetry

> Veil is not a complete DLP system. It only redacts body content matched by the configured rules. Headers, query strings, paths, JSON keys, unrecognized formats, and Base64 or binary content are outside the redaction boundary. Read “Security boundary” below before relying on it.

## Current capabilities

- Anthropic Messages, OpenAI Chat Completions, and OpenAI Responses upstream protocols.
- JSON request/response handling and SSE streaming restoration, including placeholders split across chunks.
- Built-in rules for private keys, API keys, tokens, database connection strings, passwords, mainland China mobile numbers, national ID numbers, and IPs. Email is disabled by default.
- A separate local port and upstream for each supported client.
- Management pages for Dashboard, Rules, Upstream, Keys, and Traffic.
- Rule toggling/editing, custom regex and dictionary rules, import/export, and reset.
- Optional HTTP/HTTPS upstream proxy. Empty means direct connection; system proxy settings are not inherited.
- Single-file Windows, Linux, and macOS releases with the management UI embedded.

## How it works

Veil runs as a reverse proxy between the client and the upstream model API. Point the client’s Base URL at the local gateway:

- Anthropic: `http://127.0.0.1:18787`
- OpenAI-compatible: `http://127.0.0.1:18787/v1`

String values in the request body are replaced with `{{TYPE_ULID}}` placeholders. On the response path, Veil restores placeholders for the same Creator.

Illustration; all values are synthetic:

```text
Client sends:
connect postgres://demo:demo@db.example.com/app

Upstream model receives:
connect {{CONNSTR_01ARZ3NDEKTSV4RRFFQ69G5FAV}}
```

If the model copies the placeholder into its answer, the client receives the connection string again.

### Mapping scope

Mappings are stored by **Creator + plaintext**, not by a single chat:

- Creator is derived from an existing `x-api-key`, `Authorization`, or `api-key` header. These headers are still forwarded upstream unchanged; Veil persists a hash of the selected credential, not the raw credential as its identity.
- Under one Creator, the same plaintext reuses the same placeholder.
- Changing the API key or refreshing an OAuth token creates a new Creator; old mappings are not automatically reused.
- If none of those credentials is present, requests use one shared anonymous Creator.
- Mappings remain local and are not sent upstream as extra headers or parameters.

## Quick start

### Windows

Run in PowerShell:

```powershell
irm https://raw.githubusercontent.com/guangcheng9/Veil/master/scripts/install.ps1 | iex
```

The installer downloads `veil-windows-x64.exe` to `%LOCALAPPDATA%\veil\bin`, adds it to the user PATH, and starts Veil. Open a new terminal and run:

```powershell
veil setup
```

Choose the apps by number. Veil tries to read each app’s current API URL, assigns it a dedicated local port, and points the app at Veil.

If a new dedicated port was added, restart both Veil and the app. If only an existing upstream changed, restart forwarding.

You can also download `veil-windows-x64.exe` from [Releases](https://github.com/guangcheng9/Veil/releases/latest) and double-click it.

### Linux / macOS

Download the matching binary from [Releases](https://github.com/guangcheng9/Veil/releases/latest):

| Platform | Asset |
|---|---|
| Windows x64 | `veil-windows-x64.exe` |
| Linux x64 | `veil-linux-x64` |
| Linux ARM64 | `veil-linux-arm64` |
| macOS Apple Silicon | `veil-macos-arm64` |
| macOS Intel | `veil-macos-x64` |

Make it executable, then run it:

```bash
chmod +x veil-linux-x64
./veil-linux-x64
```

Releases include `SHA256SUMS`; verify the checksum before redistributing binaries inside an organization.

## Supported clients and protocols

### Supported by `veil setup`

| Client | Protocol kind |
|---|---|
| Claude Code | Anthropic |
| Codex | OpenAI |
| PI Agent | Auto |
| CodeBuddy | Auto |
| Grok Builder | OpenAI |
| Hermes | Auto |
| Trae | Auto |

You can also select clients explicitly:

```powershell
veil setup --clients claude,codex
veil setup --clients all
```

Auto means the client protocol families share its configured upstream root. A readable existing client URL or existing route takes precedence; `--upstream` is mainly a fallback for unrecognized clients and general upstreams.

### Manual setup for other tools

Any tool that can change its Base URL and uses one of the supported protocols can connect manually:

| Protocol | Base URL |
|---|---|
| Anthropic Messages | `http://127.0.0.1:18787` |
| OpenAI Chat Completions / Responses | `http://127.0.0.1:18787/v1` |

Recognized paths currently include:

- `/v1/messages`
- `/v1/messages/count_tokens`
- `/v1/chat/completions`
- `/v1/responses`
- `/v1/models`
- `/v1/embeddings`

The general port `18787` recognizes only the paths above; unrecognized paths return `404`. Anthropic/OpenAI per-client ports treat other paths as belonging to that port's protocol family.

Set the real upstream root in the console under **Upstream → General**:

- Official APIs, company relays, and LiteLLM roots are supported.
- Veil strips a trailing `/v1` and appends the request path itself, avoiding `/v1/v1/...`.
- Upstream changes require restarting forwarding.
- Keep the relay’s key in the client. Veil does not ask for an additional model API key.

## Security boundary and failure behavior

### What is processed

- JSON string values and non-JSON text request bodies.
- JSON string values in responses.
- Text payloads in SSE responses.
- Placeholders split across streaming chunks.

### What is not processed

- Request headers, response headers, query strings, paths, HTTP methods, and JSON keys are not redacted. The forwarding layer still rewrites required transport headers such as `Host`, `Accept-Encoding`, `Content-Length`, and `Transfer-Encoding`.
- Base64, data URLs, and multimodal binary fields skip redaction so image, audio, and file payloads are not corrupted.
- `multipart/form-data` requests are rejected and not forwarded.
- Secret formats not covered by the configured rules are not detected.
- Secrets inside Base64 or binary attachments are not scanned.

### Common status codes

| Status | Meaning |
|---|---|
| `404` | The request path was not recognized by the current listener |
| `413` | Request body exceeds the limit; default 32 MiB, configurable, hard cap 128 MiB |
| `415` | The request uses multipart/form-data |
| `502` | Invalid JSON, redaction failure, mapping write failure, upstream connection failure, or JSON response restoration failure |

When redaction or mapping persistence cannot be completed, the request is not forwarded. A placeholder belonging to another Creator is never decrypted.

## Rules and mappings

Open the console → **Rules**:

- Built-in rules cover PEM private keys, API keys, tokens, database/middleware connection strings, password assignments, mainland China mobile numbers, national ID numbers, and parseable IPv4/IPv6 addresses.
- The IP rule handles every parseable address, including public addresses. The default allowlist contains `127.0.0.1`, `localhost`, `0.0.0.0`, and `::1`; these values are not replaced.
- Email is disabled by default to reduce false positives.
- Custom regex and dictionary rules are supported. Dictionaries suit project codenames and fixed internal names.
- Custom rules can be exported/imported; built-in defaults can be restored.
- Allowlist matching is exact: matching strings are never replaced.

Rule, allowlist, and upstream changes currently require **restarting forwarding**. The Dashboard marks the restart button when saved configuration has not been applied.

### Mapping storage

Mappings are stored in local SQLite:

- Sensitive mapping cells use AES-256-GCM.
- Equality lookups use HMAC-SHA256 rather than plaintext hash indexes.
- Placeholder IDs, Creator hashes, and other required metadata are not encrypted as part of a whole-database encryption scheme.
- The default master key file is `master.key` in the data directory.

If both `mappings.db` and `master.key` are copied, the holder can decrypt the mappings. Veil does not protect against an attacker who can read the complete data directory.

## Management console

The default console address is `http://127.0.0.1:18788`. The launcher and `veil ui` add the admin token automatically. Opening the bare URL manually may allow read-only status access but cause write actions to return `401`.

| Page | Current behavior |
|---|---|
| Dashboard | Service controls, the latest 512 cached traffic events, a 20-minute request chart, and update checks |
| Rules | Built-in/custom rule controls, allowlist, import, export, reset |
| Upstream | General upstreams for three protocol families, per-client upstreams, optional HTTP/HTTPS proxy, body limit, graceful shutdown timeout |
| Keys | Master-key rotation, mapping-purge entry point, admin-token reset |
| Traffic | Recent cached requests without bodies, query strings, or plaintext secrets |

Dashboard metrics come from an in-memory buffer of the latest 512 events and refresh every 2 seconds; they are not cumulative totals. The 20-minute chart uses the same buffer and may be incomplete when the cache covers a shorter period.

The Traffic table shows method, path, status, streaming flag, hit rule types, upstream time-to-first-byte, error class, and an 8-character Creator prefix.

## Data directory and privacy

Default data directories:

- Windows: `%LOCALAPPDATA%\veil`
- macOS: `~/Library/Application Support/veil`
- Linux: `${XDG_DATA_HOME:-~/.local/share}/veil`

If an old `dgw` directory exists and no `veil` directory exists, Veil continues using the old directory. Override it with `VEIL_DATA_DIR`.

The data directory typically contains:

- `config.toml`
- `mappings.db`
- `master.key`
- `admin.token`
- runtime metadata and optional debug logs

Veil has no automatic telemetry or crash reporting. Normal chat traffic goes only to the configured upstream API. Installation, manual update checks, and update downloads contact GitHub.

Debug logs are written locally only when `VEIL_DEBUG=1`; they do not contain request bodies, query strings, plaintext secrets, or complete credentials.

## CLI

```text
veil                         Start Veil and open the console
veil start --foreground      Run in the foreground with logs in the terminal
veil stop                    Stop the Veil process
veil status                  Show control-plane and forwarding state
veil setup                   Interactively wire supported clients
veil setup --clients LIST    Wire the listed clients
veil setup --upstream URL    Set the fallback/global upstream root
veil ui                      Print and open the tokenized console URL
veil update --check          Check GitHub Releases
veil update                  Download and install the latest release
veil help                    Show help
```

Common environment variables:

```text
VEIL_DATA_DIR
VEIL_MASTER_KEY
VEIL_BIND
VEIL_MODE=desktop|server
VEIL_UPSTREAM_PROXY
VEIL_DEBUG
VEIL_GITHUB_MIRROR
```

Legacy `DGW_*` variables remain compatible.

## Known limitations

- Only the protocol paths listed above are supported; Veil is not a general transparent proxy.
- Multipart requests and attachment scanning are not supported.
- Base64, data URLs, and multimodal binary content pass through unchanged.
- Headers, query strings, paths, and JSON keys are not redacted.
- Rule, allowlist, and upstream changes require restarting forwarding; request-body-limit changes also take effect after restarting forwarding.
- In the current code, the Upstream page TTL field is saved to configuration but is not connected to the SQLite mapping store. The by-Creator purge endpoint also does not yet delete database records. Do not rely on either as a complete cleanup mechanism.
- Master-key rotation replaces `master.key`, but the running mapping store does not hot-reload it; restart Veil to reopen the store with the new key.
- Unsigned release binaries may trigger a macOS Gatekeeper warning.

## More documentation

- [Usage notes (Chinese)](docs/使用.md)
- [Build, test, and packaging](LAUNCH.md)
- [Claude Code plugin](plugin/claude-code/README.md)

## License

Apache-2.0
