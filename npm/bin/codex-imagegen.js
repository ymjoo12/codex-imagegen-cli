#!/usr/bin/env node

"use strict";

const { spawn } = require("node:child_process");
const { constants } = require("node:os");
const { ensureBinary } = require("../lib/install");

async function main() {
  const binary = await ensureBinary();
  const child = spawn(binary, process.argv.slice(2), {
    stdio: "inherit",
    windowsHide: false,
  });

  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.on(signal, () => {
      if (!child.killed) {
        child.kill(signal);
      }
    });
  }

  child.on("error", (error) => {
    console.error(`codex-imagegen-cli: failed to run ${binary}: ${error.message}`);
    process.exit(1);
  });

  child.on("exit", (code, signal) => {
    if (signal) {
      process.exit(128 + (constants.signals[signal] ?? 1));
      return;
    }
    process.exit(code ?? 1);
  });
}

main().catch((error) => {
  console.error(`codex-imagegen-cli: ${error.message}`);
  process.exit(1);
});
