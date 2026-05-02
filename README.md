# codex-imagegen-cli

Rust CLI for generating or editing one image through the Codex Responses
`image_generation` hosted tool, using the same local Codex credential stores
that the Codex CLI uses.

This tool does not copy or store credentials in the repository. It resolves
`$CODEX_HOME`, or `~/.codex` when `CODEX_HOME` is unset, then reads the
configured Codex credential store.

## Use From Claude Code (gpt-imagegen Skill)

The repo bundles a Claude Code skill named `gpt-imagegen` (`skills/gpt-imagegen/`)
and a plugin manifest under `.claude-plugin/`. Once installed, asking Claude
Code things like "draw a cat in Joseon-era ink style and save it to my
Downloads folder" or "edit this image: replace the background with a sunlit
kitchen counter" routes through the bundled wrapper, which calls
`codex-imagegen` with sane defaults (PNG output, JSON response, no
implicit profile/model overrides) and reports the absolute output path.

Install as a Claude Code plugin (recommended):

```text
/plugin marketplace add ymjoo12/codex-imagegen-cli
/plugin install gpt-imagegen@codex-imagegen-cli
```

Or copy the skill manually:

```bash
mkdir -p "$HOME/.claude/skills"
cp -r skills/gpt-imagegen "$HOME/.claude/skills/"
```

The skill runs the bundled `scripts/imagegen.sh`, which auto-resolves the
binary in this order: `$CODEX_IMAGEGEN_BIN` → `codex-imagegen` on PATH →
`npx codex-imagegen-cli` → `bunx codex-imagegen-cli`. If none are
available it exits with installation guidance.

## Run With npx or bunx

Use this path when you do not want to visit the GitHub release page manually.
The npm package downloads the matching prebuilt binary on first run, verifies
its `.sha256` checksum, caches it locally, and then forwards all arguments to
the Rust CLI.

```bash
npx codex-imagegen-cli \
  --prompt "Draw a cat in a spacesuit standing on the moon. No text." \
  --output ./generated/moon-cat.png
```

```bash
bunx codex-imagegen-cli \
  --prompt "Draw a cat in a spacesuit standing on the moon. No text." \
  --output ./generated/moon-cat.png
```

After installation, the package exposes both `codex-imagegen-cli` and
`codex-imagegen` binaries.

## Install From Releases

Download a prebuilt binary from the latest GitHub release. Cargo is not
required for this path.

| Platform | Asset |
| --- | --- |
| macOS Apple Silicon | `codex-imagegen-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `codex-imagegen-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `codex-imagegen-x86_64-unknown-linux-gnu.tar.gz` |
| Windows x86_64 | `codex-imagegen-x86_64-pc-windows-msvc.zip` |

macOS Apple Silicon:

```bash
mkdir -p "$HOME/.local/bin"
curl -L \
  https://github.com/ymjoo12/codex-imagegen-cli/releases/latest/download/codex-imagegen-aarch64-apple-darwin.tar.gz |
  tar -xz -C "$HOME/.local/bin" codex-imagegen
codex-imagegen --help
```

macOS Intel:

```bash
mkdir -p "$HOME/.local/bin"
curl -L \
  https://github.com/ymjoo12/codex-imagegen-cli/releases/latest/download/codex-imagegen-x86_64-apple-darwin.tar.gz |
  tar -xz -C "$HOME/.local/bin" codex-imagegen
codex-imagegen --help
```

Linux x86_64:

```bash
mkdir -p "$HOME/.local/bin"
curl -L \
  https://github.com/ymjoo12/codex-imagegen-cli/releases/latest/download/codex-imagegen-x86_64-unknown-linux-gnu.tar.gz |
  tar -xz -C "$HOME/.local/bin" codex-imagegen
codex-imagegen --help
```

Windows x86_64 PowerShell:

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\bin"
Invoke-WebRequest `
  -Uri "https://github.com/ymjoo12/codex-imagegen-cli/releases/latest/download/codex-imagegen-x86_64-pc-windows-msvc.zip" `
  -OutFile "$env:TEMP\codex-imagegen.zip"
Expand-Archive -Force "$env:TEMP\codex-imagegen.zip" "$env:USERPROFILE\bin"
& "$env:USERPROFILE\bin\codex-imagegen.exe" --help
```

Each release also includes a `.sha256` file next to each archive. Verify a
download on macOS or Linux with:

```bash
shasum -a 256 -c codex-imagegen-aarch64-apple-darwin.tar.gz.sha256
```

Make sure `$HOME/.local/bin` or the chosen install directory is in `PATH`.

## Install From npm

Use npm, pnpm, or Bun when you want a persistent command without manually
downloading an archive:

```bash
npm install -g codex-imagegen-cli
codex-imagegen --help
```

```bash
bun install -g codex-imagegen-cli
codex-imagegen --help
```

## Maintainer Publishing

npm publishing is configured for Trusted Publishing from GitHub Actions. Do not
store a long-lived npm automation token in repository secrets. See
[`docs/publishing.md`](docs/publishing.md) for first-publish and release
requirements.

## Build From Source

Use this path when you already have Rust and Cargo installed.

```bash
cargo build --release
```

The binary is available at:

```bash
./target/release/codex-imagegen
```

## Usage

```bash
codex-imagegen \
  --prompt "Draw a cat in a spacesuit standing on the moon. No text." \
  --output ./generated/moon-cat.png
```

With image options:

```bash
codex-imagegen \
  --prompt "Draw a compact isometric desk setup using the image_generation tool, no text." \
  --output ./generated/desk.png \
  --profile openai \
  --model gpt-5.5 \
  --image-model gpt-image-2 \
  --format png \
  --size 1024x1024 \
  --quality low \
  --background opaque \
  --action generate
```

Edit a local image:

```bash
codex-imagegen \
  --input-image ./input/photo.png \
  --action edit \
  --prompt "Change the background to a rainy night street. Preserve the main subject." \
  --output ./generated/photo-edited.png
```

Use `--input-image` more than once to provide multiple edit/reference images:

```bash
codex-imagegen \
  --input-image ./input/subject.png \
  --input-image ./input/style-reference.webp \
  --action edit \
  --prompt "Apply the reference image's lighting and color treatment to the subject image." \
  --output ./generated/subject-restyled.png
```

Local image inputs are sent as base64 data URLs in the Responses request.
Supported file extensions are `.png`, `.jpg`, `.jpeg`, and `.webp`.

Codex-compatible request shape is the default: `tool_choice` remains `auto`.
For reliable live generation, phrase the prompt so the selected model calls the
`image_generation` tool:

```bash
codex-imagegen \
  --profile openai \
  --prompt "Draw a compact news app avatar using the image_generation tool, no text."
```

Some gateways also support forcing the hosted image tool instead of leaving
`tool_choice` as Codex's `auto` value:

```bash
codex-imagegen \
  --prompt "Draw a compact news app avatar, no text." \
  --tool-choice image-generation
```

Override the default request instructions:

```bash
codex-imagegen \
  --prompt "Draw a compact news app avatar using the image_generation tool, no text." \
  --instructions "You are Codex. Use the image_generation tool for image requests."
```

Force a credential store while debugging:

```bash
codex-imagegen \
  --prompt "Draw a compact news app avatar, no text." \
  --profile openai \
  --auth-source managed \
  --auth-store auto
```

Inspect the request body without using credentials or making a network request:

```bash
codex-imagegen --prompt "Draw a test image" --dry-run
```

For local image inputs, `--dry-run` redacts the base64 payload so the preview is
readable and does not print the image bytes.

Print machine-readable output:

```bash
codex-imagegen --prompt "Draw a small robot" --json
```

Pass a future or experimental image tool parameter:

```bash
codex-imagegen \
  --prompt "Draw a small robot" \
  --tool-param 'moderation="low"'
```

## Authentication

Codex profile selection matches the Codex CLI:

- `--profile <name>` or `-p <name>` selects `[profiles.<name>]` from
  `$CODEX_HOME/config.toml`.
- `--model <name>` or `-m <name>` overrides the selected profile's `model`.
- When `--profile` is omitted, the top-level `profile` value from
  `config.toml` is used when present.
- Effective model resolution is `--model`, then selected profile `model`, then
  top-level `model`, then `gpt-5.5`.
- Effective provider resolution is selected profile `model_provider`, then
  top-level `model_provider`, then `openai`.

Default auth path resolution:

1. `--codex-home <path>`
2. `$CODEX_HOME`
3. `~/.codex`

Supported credential shapes:

- Codex `model_provider` config with `experimental_bearer_token` or `env_key`
  and `base_url`
- Codex command-backed provider auth at `model_providers.<name>.auth`
- `CODEX_API_KEY` or `OPENAI_API_KEY` environment variables
- ChatGPT/Codex auth from the configured store: `tokens.access_token`, optional
  `tokens.refresh_token`, optional `tokens.account_id`
- API key auth: top-level `OPENAI_API_KEY`

Provider auth is resolved the same way Codex resolves model-provider auth: a
provider bearer token takes precedence over the managed ChatGPT/API-key store.
This supports private OpenAI-compatible gateways configured in
`$CODEX_HOME/config.toml`. Provider `base_url`, `http_headers`,
`env_http_headers`, and `query_params` are applied even when the bearer comes
from the managed ChatGPT/API-key store.

Auth source selection:

- `--auth-source codex` matches Codex provider precedence.
- `--auth-source provider` uses only the configured model provider bearer auth.
- `--auth-source managed` skips model-provider auth and uses environment/API-key
  or ChatGPT credential stores.

Credential store selection:

- `--auth-store codex` reads `cli_auth_credentials_store` from
  `$CODEX_HOME/config.toml`, matching Codex. When the setting is absent, Codex's
  default is `file`.
- `--auth-store auto` reads the Codex OS keyring entry first, then falls back to
  `auth.json`.
- `--auth-store keyring` reads the OS keyring entry named `Codex Auth`.
- `--auth-store file` reads `$CODEX_HOME/auth.json`.

For ChatGPT/Codex auth, the default base URL is:

```text
https://chatgpt.com/backend-api/codex
```

For API key auth, the default base URL is:

```text
https://api.openai.com/v1
```

When managed ChatGPT/Codex auth returns `401 Unauthorized`, the CLI performs one
guarded reload from the same credential store before refreshing. If another
Codex process already updated the file or keyring entry, the CLI uses the
updated credentials and retries without consuming the old refresh token. If the
stored auth is unchanged, it performs one OAuth refresh against
`https://auth.openai.com/oauth/token`, persists the updated tokens back to the
same Codex credential store mode, and retries the image request once. The same
guarded refresh is attempted before the first request when the stored ChatGPT
access token is already stale.

## Request Shape

The CLI sends a Responses request shaped like this:

```json
{
  "model": "gpt-5.5",
  "instructions": "You are Codex. Generate or edit images by using the provided image_generation tool.",
  "input": [
    {
      "type": "message",
      "role": "user",
      "content": [
        {
          "type": "input_text",
          "text": "Draw a small robot"
        },
        {
          "type": "input_image",
          "image_url": "data:image/png;base64,<base64>"
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
  "tool_choice": "auto",
  "parallel_tool_calls": false,
  "store": false,
  "stream": true,
  "include": []
}
```

The `input_image` item appears only when `--input-image` is provided. To force
editing an input image, pass `--action edit`; otherwise the hosted tool's
default action remains `auto`.

The image bytes come from `output[].type == "image_generation_call"` and that
item's base64 `result` field.

`--tool-choice image-generation` changes only `tool_choice`:

```json
{"tool_choice":{"type":"image_generation"}}
```

Codex sends Responses requests as streams. This CLI does the same by default and
extracts the final `response.completed` or `response.output_item.done` event.
When `response.completed` carries an empty output array but an earlier
`response.output_item.done` contains the image item, the CLI preserves the image
item and writes it to disk. Pass `--no-stream` only for providers that support
non-streaming Responses calls.

## Limits

- The Codex backend is not a public stability contract. A future Codex release
  may change required headers, request fields, endpoint behavior, or auth flow.
- Codex `agentIdentity` auth requires per-request signing and is not implemented
  in this standalone CLI.
- The Responses hosted image tool still requires a mainline `model` field.
  `tool_choice` defaults to Codex's `auto` value; forced image tool mode remains
  available through `--tool-choice image-generation` for providers that support
  forced hosted-tool choice.
- Live image generation is not part of `cargo test`, because it consumes account
  quota and depends on external service state.

## Development

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

See `docs/architecture.md` and `docs/security.md` for implementation details.
