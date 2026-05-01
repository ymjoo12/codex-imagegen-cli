# codex-imagegen-cli

Rust CLI for generating one image through the Codex Responses `image_generation`
hosted tool, using the same local Codex credential file that the Codex CLI uses.

This tool does not copy or store credentials in the repository. It reads
`$CODEX_HOME/auth.json`, or `~/.codex/auth.json` when `CODEX_HOME` is unset.

## Install

```bash
cargo build --release
```

The binary is available at:

```bash
./target/release/codex-imagegen
```

## Usage

```bash
cargo run -- \
  --prompt "Draw a cat in a spacesuit standing on the moon. No text." \
  --output ./generated/moon-cat.png
```

With image options:

```bash
cargo run -- \
  --prompt "Draw a compact isometric desk setup, no text." \
  --output ./generated/desk.png \
  --model gpt-5.5 \
  --image-model gpt-image-2 \
  --format png \
  --size 1024x1024 \
  --quality low \
  --background opaque \
  --action generate
```

Inspect the request body without using credentials or making a network request:

```bash
cargo run -- --prompt "Draw a test image" --dry-run
```

Print machine-readable output:

```bash
cargo run -- --prompt "Draw a small robot" --json
```

Pass a future or experimental image tool parameter:

```bash
cargo run -- \
  --prompt "Draw a small robot" \
  --tool-param 'moderation="low"'
```

## Authentication

Default auth path resolution:

1. `--codex-home <path>`
2. `$CODEX_HOME`
3. `~/.codex`

Supported credential shapes:

- ChatGPT/Codex auth: `tokens.access_token`, `tokens.refresh_token`, optional
  `tokens.account_id`
- API key auth: top-level `OPENAI_API_KEY`

For ChatGPT/Codex auth, the default base URL is:

```text
https://chatgpt.com/backend-api/codex
```

For API key auth, the default base URL is:

```text
https://api.openai.com/v1
```

When a ChatGPT/Codex request returns `401 Unauthorized`, the CLI performs one
OAuth refresh against `https://auth.openai.com/oauth/token`, persists the
updated tokens to `auth.json`, and retries the image request once.

## Request Shape

The CLI sends a Responses request shaped like this:

```json
{
  "model": "gpt-5.5",
  "input": [
    {
      "type": "message",
      "role": "user",
      "content": [
        {
          "type": "input_text",
          "text": "Draw a small robot"
        }
      ]
    }
  ],
  "tools": [
    {
      "type": "image_generation",
      "output_format": "png"
    }
  ],
  "tool_choice": {
    "type": "image_generation"
  },
  "parallel_tool_calls": false,
  "store": false,
  "stream": false,
  "include": []
}
```

The image bytes come from `output[].type == "image_generation_call"` and that
item's base64 `result` field.

## Limits

- The Codex backend is not a public stability contract. A future Codex release
  may change required headers, request fields, endpoint behavior, or auth flow.
- The Responses hosted image tool still requires a mainline `model` field.
  The tool call is forced with `tool_choice`, but the server may still use the
  mainline model for orchestration or prompt revision.
- Live image generation is not part of `cargo test`, because it consumes account
  quota and depends on external service state.

## Development

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

See `docs/architecture.md` and `docs/security.md` for implementation details.
