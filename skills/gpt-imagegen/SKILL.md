---
name: gpt-imagegen
description: |
  Generate or edit images through OpenAI's hosted image_generation tool by
  shelling out to the codex-imagegen CLI, which authenticates with the user's
  local Codex profile (`$CODEX_HOME` / `~/.codex`). USE THIS SKILL whenever
  the user asks Claude to make, render, draw, design, illustrate, mock up,
  or modify an image — including phrases like "GPT 이미지 생성", "OpenAI
  이미지 만들어줘", "이미지 그려줘", "이 이미지 수정해줘", "이미지 편집",
  "draw an image", "edit this picture", "modify this image", "redesign
  this avatar", or any request that needs a PNG/JPEG/WebP rendered from
  text or transformed from a reference image. Supports both pure
  text-to-image generation and image editing with one or more local
  reference images via `--input-image`. Prefer this skill over generic
  shell calls so the resulting image lands in a known directory and the
  output path is reported back to the user. Do not use for video, audio,
  or 3D model generation.
---

# gpt-imagegen

Wrap the `codex-imagegen` Rust CLI so Claude can render images on
behalf of the user without touching credentials directly. The CLI talks
to the OpenAI Responses API via the user's existing Codex profile, so
auth is whatever is already configured under `$CODEX_HOME`
(or `~/.codex`).

CLI source / docs: `https://github.com/ymjoo12/codex-imagegen-cli`.

## Always invoke through the bundled wrapper

The wrapper (`scripts/imagegen.sh` next to this SKILL.md) resolves the
binary across machines: `$CODEX_IMAGEGEN_BIN` → `codex-imagegen` on
PATH → `npx codex-imagegen-cli` → `bunx codex-imagegen-cli`. Use it
instead of calling the binary directly so the skill works regardless of
how the user installed it.

The skill is loaded from one of two locations depending on how the user
installed it:

- Plugin install: `${CLAUDE_PLUGIN_ROOT}/skills/gpt-imagegen/scripts/imagegen.sh`
- Manual copy:    `~/.claude/skills/gpt-imagegen/scripts/imagegen.sh`

The skill loader exposes the active base directory of this SKILL.md to
you. Use that base directory to build the absolute wrapper path; do not
hardcode either of the two literals above.

## Output discipline

Always pass `--output <path>` and `--json`. Never let the CLI invent a
default `./generated/image-<timestamp>.<ext>` filename — that hides the
result from the user and pollutes whatever `cwd` happens to be active.

- For one-off requests with no destination given, save under
  `./generated/<short-slug>.<ext>` inside the current working directory
  and create the directory if it does not exist.
- The slug should be 1–4 lowercase hyphenated words derived from the
  prompt (e.g. `moon-cat`, `desk-setup`, `news-app-icon`).
- Default `--format` to `png`. Switch to `jpeg` only if the user asks
  for a photo / smaller file, or to `webp` for web assets.
- After the CLI returns, parse the JSON, then tell the user the
  absolute output path on its own line so it is easy to click in the
  terminal.

Example call (substitute the absolute wrapper path your skill loader
gave you):

```bash
mkdir -p ./generated
<wrapper> \
  --prompt "Draw a small ceramic robot watering a tiny cactus, studio lighting, no text. Use the image_generation tool." \
  --output ./generated/robot-cactus.png \
  --format png \
  --json
```

## Phrase the prompt so the model actually calls the tool

The default `tool_choice` is Codex's `auto`, so the model can decide
**not** to call the hosted image tool and instead reply with text. The
README is explicit about this: "phrase the prompt so the selected model
calls the `image_generation` tool". To make generation reliable, append
an explicit instruction such as **"Use the image_generation tool. No
text in the image."** to the user's request before passing it to
`--prompt`.

If the user's prompt already contains very direct image instructions
(e.g. they themselves wrote "use the image_generation tool"), don't
duplicate it — just forward as is.

When generation still falls through (the response carries no
`image_generation_call`), retry once with `--tool-choice image-generation`
to force the hosted tool. Some Codex profiles / gateways accept this and
some don't; if the second attempt also fails, surface the error rather
than looping.

## Image editing

To edit or transform existing images, attach them with `--input-image`
(alias `--image`). Pass the flag once per file; up to several local
PNG/JPG/JPEG/WebP files are supported, and the CLI inlines them as data
URLs so no upload step is required.

Add `--action edit` so the request explicitly tells the hosted tool this
is an edit, not a generation. (`--action reference` is the right choice
when the source image is style guidance only and the result should be a
new composition.)

```bash
<wrapper> \
  --prompt "Replace the background with a sunlit kitchen counter. Keep the robot identical. Use the image_generation tool." \
  --input-image /abs/path/to/source.png \
  --action edit \
  --output ./generated/robot-kitchen.png \
  --json
```

Multiple references:

```bash
<wrapper> \
  --prompt "Combine the pose from the first image with the color palette of the second. Use the image_generation tool." \
  --image ./refs/pose.png \
  --image ./refs/palette.png \
  --action reference \
  --output ./generated/combined.png \
  --json
```

The user usually provides reference paths inline — accept absolute or
relative paths. If a path doesn't exist, stop and ask before spending
quota.

## Profile, model, and other knobs

The skill defers to the user's Codex configuration by default — do not
inject `--profile` or `--model` unless the user names them, because
overriding the user's `config.toml` defaults is surprising and can route
the request to a gateway they didn't intend.

Forward these CLI flags only when the user asks for them:

| Flag | Purpose |
| --- | --- |
| `--profile <name>` / `-p` | Pick a `[profiles.<name>]` from `config.toml` (e.g. `openai`). |
| `--model <name>` / `-m` | Override the mainline model (`gpt-5.5` etc.). |
| `--image-model <name>` | Pick a specific image model (`gpt-image-2`). |
| `--size <WxH>` | E.g. `1024x1024`, `1792x1024`. |
| `--quality <low\|medium\|high>` | Hosted-tool quality knob. |
| `--background <opaque\|transparent>` | Background mode. |
| `--compression <0-100>` | Output compression for jpeg/webp. |
| `--tool-param KEY=JSON_OR_TEXT` | Pass-through for experimental tool params. |
| `--auth-source managed --profile openai` | Force the official ChatGPT/Codex backend when a custom gateway lacks `image_generation`. |

If you need to inspect what would be sent without spending quota, use
`--dry-run`. It prints the request body and never calls the network.
This is useful when debugging a profile/auth mismatch.

## Reading the JSON response

With `--json`, the CLI prints a single object:

```json
{
  "response_id": "resp_...",
  "image_id": "ig_...",
  "revised_prompt": "...",
  "output_path": "/abs/path/.../robot-cactus.png",
  "format": "png"
}
```

- Quote `output_path` back to the user verbatim (absolute path is fine).
- If `revised_prompt` differs meaningfully from what was asked, surface
  it as one short line — the model sometimes rewrites the prompt and
  the user benefits from knowing.
- `response_id` and `image_id` are useful for follow-up edits / debug;
  keep them in your scratch context but don't dump them on the user
  unless asked.

Optional: after writing, you may also `Read` the resulting PNG so the
user gets an inline preview in the chat. Do this for single-image
results when the file is under a few MB — it makes the conversation
visually self-contained.

## Failure modes worth handling

- **`401 Unauthorized` after one retry.** The CLI already does one
  guarded refresh against the OAuth endpoint. If it still fails, the
  user's stored Codex credentials are stale — tell them to run their
  Codex login flow (e.g. `codex login`) and try again. Don't try to
  refresh tokens yourself.
- **Empty `image_generation_call` in the response.** The selected
  profile / gateway succeeded but did not return an image. Retry once
  with `--tool-choice image-generation`. If that still fails, suggest
  `--auth-source managed --profile openai` so the official ChatGPT
  Codex backend is used.
- **`compression must be between 0 and 100`** or other CLI-side
  validation errors. Just relay the message; do not mask it.
- **Wrapper exits 127 with "codex-imagegen binary not found".** The
  user has not installed the CLI yet. Point them at the install options
  in `https://github.com/ymjoo12/codex-imagegen-cli` (npm / release
  binary / source build) or to set `CODEX_IMAGEGEN_BIN` to a known
  binary path.

## What this skill does NOT do

- It does not generate video, audio, 3D assets, or vector SVG.
- It does not remove backgrounds, upscale, or run any post-processing
  beyond what the hosted tool itself does.
- It does not create credentials. Auth must already be configured by
  the user under `$CODEX_HOME`.
- It does not batch-generate dozens of variations in parallel — the CLI
  is one image per invocation. If the user asks for N variants, loop
  with distinct `--output` filenames; do not try to fan out concurrent
  invocations against the same Codex auth (you'll race the OAuth
  refresh path).
