# Security

This repository must never contain real Codex credentials.

## Credential Handling

- The CLI reads `auth.json` from the user's Codex home at runtime.
- It does not copy `auth.json` into the project.
- It does not print access tokens, refresh tokens, API keys, or full auth JSON.
- Known token values are redacted from HTTP error bodies before display.
- Token refresh writes back only to the original Codex auth file.

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

## Network Scope

Default network destinations:

- `https://chatgpt.com/backend-api/codex/responses`
- `https://auth.openai.com/oauth/token` for ChatGPT/Codex refresh
- `https://api.openai.com/v1/responses` only when auth uses `OPENAI_API_KEY`

Do not add telemetry, analytics, or third-party destinations.
