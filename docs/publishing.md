# Publishing

This project publishes GitHub Release binaries and the npm wrapper from
`.github/workflows/release.yml`.

## npm Trusted Publishing

npm publishing must use Trusted Publishing with GitHub Actions OIDC. Do not add
an `NPM_TOKEN` repository secret and do not create a CI token with 2FA bypass.

The release workflow already grants:

```yaml
permissions:
  contents: read
  id-token: write
```

The publish step runs:

```bash
npm publish --access public
```

This requires npm's trusted publisher settings to authorize this repository and
workflow:

- npm package: `codex-imagegen-cli`
- GitHub repository: `ymjoo12/codex-imagegen-cli`
- Workflow file: `release.yml`
- Environment: leave empty unless the workflow is changed to use one

## First Publish

If the package does not exist on npm yet, publish version `0.3.0` once from a
local npm login with normal interactive 2FA:

```bash
npm publish --access public
```

After that first publish, configure the package's trusted publisher in npm, then
future tag releases can publish from GitHub Actions without an npm token.

## Release Checklist

1. Update `package.json` and `Cargo.toml` to the same version.
2. Run `cargo fmt --check`, `cargo test`, `cargo clippy -- -D warnings`,
   `cargo build --release --locked`, and `npm run test:npm`.
3. Commit and push.
4. Create and push a matching `vX.Y.Z` tag.
5. Confirm the GitHub Release assets were uploaded.
6. Confirm npm published the same version with provenance on the npm package
   page.
