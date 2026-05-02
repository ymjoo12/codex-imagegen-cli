"use strict";

function resolvePlatformTarget(platform = process.platform, arch = process.arch) {
  if (platform === "darwin" && arch === "arm64") {
    return {
      target: "aarch64-apple-darwin",
      asset: "codex-imagegen-aarch64-apple-darwin.tar.gz",
      archiveType: "tar.gz",
      binaryName: "codex-imagegen",
    };
  }

  if (platform === "darwin" && arch === "x64") {
    return {
      target: "x86_64-apple-darwin",
      asset: "codex-imagegen-x86_64-apple-darwin.tar.gz",
      archiveType: "tar.gz",
      binaryName: "codex-imagegen",
    };
  }

  if (platform === "linux" && arch === "x64") {
    return {
      target: "x86_64-unknown-linux-gnu",
      asset: "codex-imagegen-x86_64-unknown-linux-gnu.tar.gz",
      archiveType: "tar.gz",
      binaryName: "codex-imagegen",
    };
  }

  if (platform === "win32" && arch === "x64") {
    return {
      target: "x86_64-pc-windows-msvc",
      asset: "codex-imagegen-x86_64-pc-windows-msvc.zip",
      archiveType: "zip",
      binaryName: "codex-imagegen.exe",
    };
  }

  throw new Error(
    `unsupported platform: ${platform}/${arch}. Supported targets are macOS arm64, macOS x64, Linux x64, and Windows x64.`,
  );
}

module.exports = {
  resolvePlatformTarget,
};
