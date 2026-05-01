# Architecture

`codex-imagegen-cli` is intentionally small. It mirrors the Codex Responses
hosted-tool path without depending on Codex's internal Rust workspace.

## Modules

- `args`: CLI shape, prompt source selection, output format enum.
- `auth`: Resolves Codex model-provider auth and credential stores, applies
  bearer headers, refreshes ChatGPT OAuth tokens, and persists refreshed auth
  JSON.
- `client`: Builds the Responses request body and sends `POST /responses`.
- `response`: Extracts `image_generation_call.result` from JSON or SSE output.
- `output`: Decodes base64 and writes the image file.
- `security`: Redacts known token values from error text.

## Data Flow

1. Parse CLI args and resolve prompt text from `--prompt`, `--prompt-file`, or
   the positional prompt.
2. Build a Responses request with the `image_generation` hosted tool and
   Codex-compatible `tool_choice: "auto"` by default.
3. Resolve Codex auth from `--codex-home`, `$CODEX_HOME`, or `~/.codex`.
4. Read Codex model-provider routing and bearer auth first, then the same
   credential store mode Codex uses. `--auth-source managed` skips
   model-provider auth, and `--auth-store` overrides the managed store mode.
5. Send the request to `{base_url}/responses` with bearer auth and Codex
   identity metadata.
6. If managed ChatGPT/Codex auth receives `401`, refresh tokens and retry once.
7. Decode the first `image_generation_call.result` base64 payload.
8. Save the image to `--output` or `./generated/image-<timestamp>.<ext>`.

## Compatibility Notes

The public Codex Rust source currently registers an image tool with:

```json
{"type":"image_generation","output_format":"png"}
```

This CLI uses the same minimum tool shape by default, then adds optional image
tool parameters only when CLI flags provide them.

The public Codex source stores CLI auth in the `Codex Auth` keyring service when
`cli_auth_credentials_store` is `keyring` or `auto`. The keyring account is
`cli|` plus the first 16 hex characters of SHA-256 over the canonical
`CODEX_HOME` path. This CLI uses that same service and account derivation.

Codex provider auth takes precedence over managed ChatGPT/API-key auth when a
configured provider supplies `experimental_bearer_token`, `env_key`, or
command-backed `auth`. This CLI uses the configured provider `base_url`,
`http_headers`, `env_http_headers`, and `query_params` even when the actual
bearer comes from managed ChatGPT/API-key auth.

Codex's public Rust source serializes Responses requests with
`tool_choice: "auto"` and a `prompt_cache_key` equal to the conversation id.
This CLI uses a fresh UUIDv7 conversation id per invocation and sends it as
`x-client-request-id`, `session_id`, and `prompt_cache_key`.

The official Responses API also supports forcing the hosted image tool with:

```json
{"tool_choice":{"type":"image_generation"}}
```

This CLI exposes that non-default mode with `--tool-choice image-generation`.

The tool result is returned as base64 in:

```json
{"type":"image_generation_call","result":"..."}
```

Codex streams Responses requests. This CLI sends `stream: true` by default and
parses `response.completed` or `response.output_item.done` SSE events.
