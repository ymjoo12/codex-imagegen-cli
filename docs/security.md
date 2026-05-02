# Security

This repository must never contain real Codex credentials.

## Credential Handling

- The CLI reads `auth.json` from the user's Codex home at runtime.
- The CLI can also read the OS keyring entry used by Codex when
  `--auth-store keyring`, `--auth-store auto`, or Codex config selects that
  store.
- The CLI can read provider bearer auth from Codex config, provider `env_key`,
  or provider command-backed `auth`, matching Codex provider auth precedence.
- The CLI reads profile `model` and `model_provider` settings from Codex config
  but does not persist those values outside normal docs or test fixtures.
- Provider `http_headers`, `env_http_headers`, and `query_params` are read at
  runtime and are not written to repository files.
- It does not copy `auth.json` into the project.
- It does not print keyring contents, access tokens, refresh tokens, API keys,
  provider bearer tokens, or full auth JSON.
- `--input-image` reads local image bytes only for the current request and does
  not write those bytes to repository files.
- `--dry-run` redacts local image data URLs instead of printing full base64
  image payloads.
- Known token values are redacted from HTTP error bodies before display.
- Token refresh writes back only to the selected Codex credential store mode.
- The npm wrapper verifies downloaded release archives with the adjacent
  `.sha256` file before installing the cached binary.

## Git Hygiene

`.gitignore` excludes:

- `.codex/`
- `auth.json`
- `.env`
- generated images
- Rust build output

Before pushing, verify:

```bash
git status --short
git grep -n "access_token\\|refresh_token\\|OPENAI_API_KEY\\|sk-" -- .
```

Only dummy tokens may appear in tests or documentation.

Before making the repository public, also search for local-only identifiers such
as private profile names, usernames, hostnames, and gateway names:

```bash
PRIVATE_TERMS='private-profile|local-user|internal-host' git grep -n -E "$PRIVATE_TERMS" -- .
```

Use neutral fixture names such as `custom_gateway`, `corp`, or `example` in
tests and documentation.

The npm wrapper stores downloaded binaries in the user cache directory, not in
the repository. Set `CODEX_IMAGEGEN_CACHE_DIR` only to a directory where cached
executables are acceptable.

## Network Scope

Default network destinations:

- `https://chatgpt.com/backend-api/codex/responses`
- `https://auth.openai.com/oauth/token` for ChatGPT/Codex refresh
- `https://api.openai.com/v1/responses` only when auth uses `OPENAI_API_KEY`
- The configured Codex provider `base_url` when a model provider sets one

Do not add telemetry, analytics, or third-party destinations.
