# Limitations

## Hosted Tool Boundary

The CLI uses the Responses hosted `image_generation` tool. Official API
behavior requires a mainline `model` field such as `gpt-5.5`, even when the
image tool call is forced. Codex-compatible mode leaves `tool_choice` as
`auto`, so the backend may decide whether to call the hosted tool. Forced mode
is available with `--tool-choice image-generation`, but it still cannot prove
that the backend performs no model-side orchestration and may not be supported
by every profile.

## Codex Backend Stability

The ChatGPT/Codex backend URL is based on observed Codex CLI behavior:

```text
https://chatgpt.com/backend-api/codex
```

That backend is not documented as a public stable API. If Codex changes headers,
request compression, auth requirements, or endpoint paths, this CLI may need an
update.

## Provider Gateways

Custom OpenAI-compatible gateways may accept the Responses request but return no
`image_generation_call`. In that case the transport and auth path succeeded, but
the selected gateway did not return a hosted image tool result. Use `--profile`
to select the same Codex profile you use interactively, and use
`--auth-source managed --profile openai` when you specifically need the ChatGPT
Codex backend.

## Agent Identity

Codex can use `agentIdentity` auth in internal hosted contexts. That flow signs
each request with an agent private key and a registered task id. This standalone
CLI supports Codex file, keyring, auto, ChatGPT OAuth, and API key auth, but it
does not yet implement agent-identity request signing.

## Live Tests

Automated tests do not call the image service. They cover local request
construction, auth parsing, response extraction, and base64 file writing.

Manual live test:

```bash
cargo run -- \
  --prompt "Draw a tiny orange robot watering a cactus using the image_generation tool, no text." \
  --output ./generated/live-test.png \
  --json
```
