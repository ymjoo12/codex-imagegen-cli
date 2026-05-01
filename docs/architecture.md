# Architecture

`codex-imagegen-cli` is intentionally small. It mirrors the Codex Responses
hosted-tool path without depending on Codex's internal Rust workspace.

## Modules

- `args`: CLI shape, prompt source selection, output format enum.
- `auth`: Reads Codex credentials, applies bearer headers, refreshes ChatGPT
  OAuth tokens, and persists refreshed auth JSON.
- `client`: Builds the Responses request body and sends `POST /responses`.
- `response`: Extracts `image_generation_call.result` from the response JSON.
- `output`: Decodes base64 and writes the image file.
- `security`: Redacts known token values from error text.

## Data Flow

1. Parse CLI args and resolve prompt text from `--prompt`, `--prompt-file`, or
   the positional prompt.
2. Build a Responses request with the `image_generation` hosted tool and forced
   `tool_choice`.
3. Load Codex auth from `--codex-home`, `$CODEX_HOME`, or `~/.codex`.
4. Send the request to `{base_url}/responses` with bearer auth.
5. If ChatGPT/Codex auth receives `401`, refresh tokens and retry once.
6. Decode the first `image_generation_call.result` base64 payload.
7. Save the image to `--output` or `./generated/image-<timestamp>.<ext>`.

## Compatibility Notes

The public Codex Rust source currently registers an image tool with:

```json
{"type":"image_generation","output_format":"png"}
```

This CLI uses the same minimum tool shape by default, then adds optional image
tool parameters only when CLI flags provide them.

The official Responses API supports forcing the hosted image tool with:

```json
{"tool_choice":{"type":"image_generation"}}
```

The tool result is returned as base64 in:

```json
{"type":"image_generation_call","result":"..."}
```
