"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { resolvePlatformTarget } = require("./platform");

const PACKAGE_ROOT = path.resolve(__dirname, "../..");
const PACKAGE_JSON = require(path.join(PACKAGE_ROOT, "package.json"));
const PACKAGE_NAME = PACKAGE_JSON.name.replace(/[^a-zA-Z0-9._-]/g, "-");
const FALLBACK_REPOSITORY = "ymjoo12/codex-imagegen-cli";

function cacheRoot() {
  if (process.env.CODEX_IMAGEGEN_CACHE_DIR) {
    return path.resolve(process.env.CODEX_IMAGEGEN_CACHE_DIR);
  }

  if (process.platform === "win32") {
    const base =
      process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local");
    return path.join(base, PACKAGE_NAME, "Cache");
  }

  if (process.platform === "darwin") {
    return path.join(os.homedir(), "Library", "Caches", PACKAGE_NAME);
  }

  return path.join(
    process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache"),
    PACKAGE_NAME,
  );
}

function repositoryPath() {
  const raw = PACKAGE_JSON.repository?.url;
  if (typeof raw !== "string") {
    return FALLBACK_REPOSITORY;
  }

  const match = raw.match(/github\.com[:/]([^/]+\/[^/.]+)(?:\.git)?/);
  return match?.[1] ?? FALLBACK_REPOSITORY;
}

function releaseTag() {
  return process.env.CODEX_IMAGEGEN_RELEASE_TAG || `v${PACKAGE_JSON.version}`;
}

function releaseBaseUrl() {
  if (process.env.CODEX_IMAGEGEN_RELEASE_BASE_URL) {
    return process.env.CODEX_IMAGEGEN_RELEASE_BASE_URL.replace(/\/+$/, "");
  }
  return `https://github.com/${repositoryPath()}/releases/download/${releaseTag()}`;
}

function binaryPathForTarget(target) {
  return path.join(
    cacheRoot(),
    PACKAGE_JSON.version,
    target.target,
    target.binaryName,
  );
}

async function pathExists(filePath) {
  try {
    await fs.promises.access(filePath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

async function fetchBuffer(url) {
  const response = await fetch(url, {
    headers: {
      "user-agent": `${PACKAGE_JSON.name}/${PACKAGE_JSON.version}`,
    },
  });

  if (!response.ok) {
    throw new Error(`failed to download ${url}: HTTP ${response.status}`);
  }

  return Buffer.from(await response.arrayBuffer());
}

async function downloadFile(url, destination) {
  const bytes = await fetchBuffer(url);
  await fs.promises.writeFile(destination, bytes);
}

function expectedChecksum(text) {
  const match = text.match(/\b[a-fA-F0-9]{64}\b/);
  if (!match) {
    throw new Error("release checksum file did not contain a sha256 digest");
  }
  return match[0].toLowerCase();
}

async function verifyChecksum(archivePath, checksumUrl) {
  const checksumText = (await fetchBuffer(checksumUrl)).toString("utf8");
  const expected = expectedChecksum(checksumText);
  const actual = crypto
    .createHash("sha256")
    .update(await fs.promises.readFile(archivePath))
    .digest("hex");

  if (actual !== expected) {
    throw new Error(
      `downloaded archive checksum mismatch: expected ${expected}, got ${actual}`,
    );
  }
}

function run(command, args) {
  const result = spawnSync(command, args, {
    stdio: "pipe",
    encoding: "utf8",
    windowsHide: true,
  });

  if (result.status !== 0) {
    const stderr = result.stderr.trim();
    throw new Error(
      stderr
        ? `${command} failed: ${stderr}`
        : `${command} failed with status ${result.status}`,
    );
  }
}

function extractArchive(archivePath, destination, archiveType) {
  fs.mkdirSync(destination, { recursive: true });

  if (archiveType === "tar.gz") {
    run("tar", ["-xzf", archivePath, "-C", destination]);
    return;
  }

  if (archiveType === "zip") {
    run("powershell.exe", [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-Command",
      "Expand-Archive",
      "-LiteralPath",
      archivePath,
      "-DestinationPath",
      destination,
      "-Force",
    ]);
    return;
  }

  throw new Error(`unsupported archive type: ${archiveType}`);
}

async function installBinary(target, binaryPath) {
  const tmpRoot = await fs.promises.mkdtemp(
    path.join(os.tmpdir(), `${PACKAGE_NAME}-`),
  );
  const archivePath = path.join(tmpRoot, target.asset);
  const extractPath = path.join(tmpRoot, "extract");
  const assetUrl = `${releaseBaseUrl()}/${target.asset}`;

  try {
    await downloadFile(assetUrl, archivePath);
    await verifyChecksum(archivePath, `${assetUrl}.sha256`);
    extractArchive(archivePath, extractPath, target.archiveType);

    const extractedBinary = path.join(extractPath, target.binaryName);
    await fs.promises.copyFile(extractedBinary, binaryPath);
    if (process.platform !== "win32") {
      await fs.promises.chmod(binaryPath, 0o755);
    }
  } finally {
    await fs.promises.rm(tmpRoot, { recursive: true, force: true });
  }
}

async function ensureBinary() {
  if (process.env.CODEX_IMAGEGEN_BIN) {
    return path.resolve(process.env.CODEX_IMAGEGEN_BIN);
  }

  const target = resolvePlatformTarget();
  const binaryPath = binaryPathForTarget(target);
  if (await pathExists(binaryPath)) {
    return binaryPath;
  }

  await fs.promises.mkdir(path.dirname(binaryPath), { recursive: true });
  await installBinary(target, binaryPath);
  return binaryPath;
}

module.exports = {
  binaryPathForTarget,
  cacheRoot,
  ensureBinary,
  releaseBaseUrl,
  releaseTag,
};
