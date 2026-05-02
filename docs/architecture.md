# Architecture

`codex-imagegen-cli` is intentionally small. It mirrors the Codex Responses
hosted-tool path without depending on Codex's internal Rust workspace.

## Modules

- `args`: CLI shape, prompt source selection, local image input paths, output
  format enum.
- `auth`: Resolves Codex profile overrides, model-provider auth, and credential
  stores, applies bearer headers, refreshes ChatGPT OAuth tokens, and persists
  refreshed auth JSON.
- `client`: Builds the Responses request body and sends `POST /responses`.
- `response`: Extracts `image_generation_call.result` from JSON or SSE output.
- `output`: Decodes base64 and writes the image file.
- `security`: Redacts known token values from error text.

## Data Flow

1. Parse CLI args and resolve prompt text from `--prompt`, `--prompt-file`, or
   the positional prompt.
2. Resolve `$CODEX_HOME/config.toml` profile settings. CLI `--model` overrides
   the selected profile model, matching Codex's `-m` behavior.
3. Build a Responses request with the `image_generation` hosted tool and
   Codex-compatible `tool_choice: "auto"` by default. The request always
   includes non-empty `instructions`, matching the Codex backend requirement.
4. If `--input-image` is present, read each local image, encode it as a base64
   data URL, and append an `input_image` content item next to the prompt text.
5. Resolve Codex auth from `--codex-home`, `$CODEX_HOME`, or `~/.codex`.
6. Read Codex model-provider routing and bearer auth first, then the same
   credential store mode Codex uses. `--auth-source managed` skips
   model-provider auth, and `--auth-store` overrides the managed store mode.
7. Before the request, perform Codex-style guarded reload plus refresh when the
   managed ChatGPT access token is already stale.
8. Send the request to `{base_url}/responses` with bearer auth and Codex
   identity metadata.
9. If managed ChatGPT/Codex auth receives `401`, perform guarded reload plus
   refresh and retry once.
10. Decode the first `image_generation_call.result` base64 payload.
11. Save the image to `--output` or `./generated/image-<timestamp>.<ext>`.

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

Codex profile selection is applied before provider routing. The profile
resolution order is CLI `--profile`, then top-level `profile`; within that
profile, `model` and `model_provider` override the top-level values. CLI
`--model` overrides both profile and top-level `model`.

Codex's `AuthManager::auth()` proactively refreshes stale managed ChatGPT auth.
This CLI mirrors the same safety shape: it reloads the selected credential store
and compares the account plus token snapshot before calling the OAuth refresh
endpoint. That avoids consuming an old refresh token after another Codex process
already persisted newer credentials.

Codex's public Rust source serializes Responses requests with
`tool_choice: "auto"` and a `prompt_cache_key` equal to the conversation id.
This CLI uses a fresh UUIDv7 conversation id per invocation and sends it as
`x-client-request-id`, `session_id`, and `prompt_cache_key`.

The official Responses API also supports forcing the hosted image tool with:

```json
{"tool_choice":{"type":"image_generation"}}
```

This CLI exposes that non-default mode with `--tool-choice image-generation`.

The official Responses API accepts images in message content as `input_image`
items with a URL, a base64 data URL, or a Files API file id. This CLI uses
base64 data URLs for local `--input-image` files so it does not need a separate
upload step.

The official Responses `image_generation` tool supports an `action` parameter.
This CLI passes `--action edit` through unchanged, which forces editing when an
input image is in the request context.

The tool result is returned as base64 in:

```json
{"type":"image_generation_call","result":"..."}
```

Codex streams Responses requests. This CLI sends `stream: true` by default,
parses `response.completed` and `response.output_item.done` SSE events, and
preserves image items that arrive before a completed response with an empty
`output` array.
