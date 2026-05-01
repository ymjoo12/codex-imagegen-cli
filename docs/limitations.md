# Limitations

## Hosted Tool Boundary

The CLI uses the Responses hosted `image_generation` tool. Official API
behavior requires a mainline `model` field such as `gpt-5.5`, even when the
image tool call is forced. This CLI therefore avoids free-form text output, but
it cannot prove that the backend performs no model-side orchestration.

## Codex Backend Stability

The ChatGPT/Codex backend URL is based on observed Codex CLI behavior:

```text
https://chatgpt.com/backend-api/codex
```

That backend is not documented as a public stable API. If Codex changes headers,
request compression, auth requirements, or endpoint paths, this CLI may need an
update.

## Live Tests

Automated tests do not call the image service. They cover local request
construction, auth parsing, response extraction, and base64 file writing.

Manual live test:

```bash
cargo run -- \
  --prompt "Draw a tiny orange robot watering a cactus, no text." \
  --output ./generated/live-test.png \
  --json
```
