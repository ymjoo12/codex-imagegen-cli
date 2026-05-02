"use strict";

const assert = require("node:assert/strict");
const { resolvePlatformTarget } = require("../lib/platform");

assert.equal(
  resolvePlatformTarget("darwin", "arm64").asset,
  "codex-imagegen-aarch64-apple-darwin.tar.gz",
);
assert.equal(
  resolvePlatformTarget("darwin", "x64").asset,
  "codex-imagegen-x86_64-apple-darwin.tar.gz",
);
assert.equal(
  resolvePlatformTarget("linux", "x64").asset,
  "codex-imagegen-x86_64-unknown-linux-gnu.tar.gz",
);
assert.equal(
  resolvePlatformTarget("win32", "x64").asset,
  "codex-imagegen-x86_64-pc-windows-msvc.zip",
);
assert.throws(
  () => resolvePlatformTarget("linux", "arm64"),
  /unsupported platform/,
);
