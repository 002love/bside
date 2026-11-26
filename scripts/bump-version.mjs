#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";

function runGit(args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

function parseSemver(raw) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(raw.trim());
  if (!match) {
    throw new Error(`Invalid semver version: "${raw}"`);
  }
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

function formatSemver(v) {
  return `${v.major}.${v.minor}.${v.patch}`;
}

function compareSemver(a, b) {
  if (a.major !== b.major) return a.major - b.major;
  if (a.minor !== b.minor) return a.minor - b.minor;
  return a.patch - b.patch;
}

function getLatestSemverTag() {
  const output = runGit(["tag", "--list", "v[0-9]*.[0-9]*.[0-9]*", "--sort=-v:refname"]);
  if (!output) return null;
  for (const line of output.split("\n")) {
    const trimmed = line.trim();
    if (/^v\d+\.\d+\.\d+$/.test(trimmed)) {
      return trimmed;
    }
  }
  return null;
}

function readCargoVersion() {
  const cargo = fs.readFileSync("Cargo.toml", "utf8");
  const match = /^version\s*=\s*"([^"]+)"/m.exec(cargo);
  if (!match) {
    throw new Error("Could not find package version in Cargo.toml");
  }
  return match[1];
}

function detectBumpFromHistory(lastTag) {
  const range = lastTag ? `${lastTag}..HEAD` : "HEAD";
  const delimiter = "---BSIDE-COMMIT-END---";
  const log = runGit(["log", range, `--format=%s%n%b%n${delimiter}`]);
  if (!log) return "patch";

  let bump = "patch";
  for (const entry of log.split(delimiter)) {
    const trimmed = entry.trim();
    if (!trimmed) continue;
    const [subject, ...bodyLines] = trimmed.split("\n");
    const body = bodyLines.join("\n");

    if (
      /^[a-z]+(\([^)]+\))?!:/.test(subject) ||
      /^BREAKING[ -]CHANGE:/m.test(body)
    ) {
      return "major";
    }
    if (/^feat(\([^)]+\))?:/.test(subject)) {
      bump = "minor";
    }
  }
  return bump;
}

function bumpSemver(base, kind) {
  if (kind === "major") {
    return { major: base.major + 1, minor: 0, patch: 0 };
  }
  if (kind === "minor") {
    return { major: base.major, minor: base.minor + 1, patch: 0 };
  }
  return { major: base.major, minor: base.minor, patch: base.patch + 1 };
}

function updatePackageToml(filePath, version) {
  const content = fs.readFileSync(filePath, "utf8");
  let replaced = false;
  const updated = content.replace(
    /(\[package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m,
    (_, prefix, _old, suffix) => {
      replaced = true;
      return `${prefix}${version}${suffix}`;
    },
  );
  if (!replaced) {
    throw new Error(`Could not update [package] version in ${filePath}`);
  }
  fs.writeFileSync(filePath, updated);
}

function updateCargoLock(filePath, packageNames, version) {
  if (!fs.existsSync(filePath)) return;
  let content = fs.readFileSync(filePath, "utf8");
  for (const pkgName of packageNames) {
    const pattern = new RegExp(
      `(\\[\\[package\\]\\]\\r?\\nname = "${pkgName}"\\r?\\nversion = ")([^"]+)(")`,
      "g",
    );
    content = content.replace(pattern, `$1${version}$3`);
  }
  fs.writeFileSync(filePath, content);
}

function updateJsonVersion(filePath, version) {
  const content = fs.readFileSync(filePath, "utf8");
  const parsed = JSON.parse(content);
  parsed.version = version;
  fs.writeFileSync(filePath, `${JSON.stringify(parsed, null, 2)}\n`);
}

function writeAllVersions(version) {
  updatePackageToml("Cargo.toml", version);
  updateCargoLock("Cargo.lock", ["bside"], version);
  updatePackageToml("src-tauri/Cargo.toml", version);
  updateCargoLock("src-tauri/Cargo.lock", ["bside", "bside-desktop"], version);
  updateJsonVersion("src-tauri/tauri.conf.json", version);
  updateJsonVersion("web/package.json", version);
}

function parseArgs(argv) {
  const opts = {
    bump: "auto",
    tag: "",
    write: false,
    json: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--write") {
      opts.write = true;
    } else if (arg === "--json") {
      opts.json = true;
    } else if (arg === "--bump") {
      opts.bump = argv[++i] || "auto";
    } else if (arg === "--tag") {
      opts.tag = argv[++i] || "";
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  if (!["auto", "patch", "minor", "major"].includes(opts.bump)) {
    throw new Error(`Invalid --bump value: ${opts.bump}`);
  }
  return opts;
}

const opts = parseArgs(process.argv.slice(2));
const lastTag = getLatestSemverTag();
const lastVer = lastTag ? parseSemver(lastTag) : { major: 0, minor: 0, patch: 0 };
const cargoVer = parseSemver(readCargoVersion());

let nextVer;
let reason;

if (opts.tag) {
  nextVer = parseSemver(opts.tag);
  reason = "explicit-tag";
} else if (opts.bump !== "auto") {
  const base = compareSemver(cargoVer, lastVer) > 0 ? cargoVer : lastVer;
  nextVer = bumpSemver(base, opts.bump);
  reason = `manual-${opts.bump}`;
} else if (compareSemver(cargoVer, lastVer) > 0) {
  nextVer = cargoVer;
  reason = "cargo-toml-ahead";
} else {
  const detected = detectBumpFromHistory(lastTag);
  nextVer = bumpSemver(lastVer, detected);
  reason = `auto-${detected}`;
}

const version = formatSemver(nextVer);
const tag = `v${version}`;

if (opts.write) {
  writeAllVersions(version);
}

const result = {
  lastTag: lastTag || "",
  version,
  tag,
  reason,
};

if (opts.json) {
  console.log(JSON.stringify(result));
} else {
  console.log(`version=${result.version}`);
  console.log(`tag=${result.tag}`);
  console.log(`last_tag=${result.lastTag}`);
  console.log(`reason=${result.reason}`);
}
