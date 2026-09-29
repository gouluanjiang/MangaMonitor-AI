// Collect installed, locked public dependency notices without publishing metadata paths.
// Run in Windows CI after frozen pnpm installation and native dependency fetch/build.
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

export const TARGET = "x86_64-pc-windows-msvc";
const DEFAULT_OUTPUT =
  "apps/local-workbench/src-tauri/release-resources/licenses";
const NPM_ROOT = "apps/local-workbench";
const NATIVE_MANIFEST = "apps/local-workbench/src-tauri/Cargo.toml";
const SUPPLEMENT_ROOT = "third-party/dependency-licenses";
const sha256 = (value) => createHash("sha256").update(value).digest("hex");
const ordered = (values) =>
  [...values].sort((a, b) => a.localeCompare(b, "en"));
const packageName = /^(?:@[a-z0-9._-]+\/)?[a-z0-9][a-z0-9._-]*$/i;
const packageVersion = /^\d+\.\d+\.\d+(?:[-+][a-z0-9.-]+)?$/i;
const licenseName = /^(?:licen[sc]e|copying|unlicense)(?:[._-]|$)/i;
const noticeName = /^(?:notice|copyright|authors)(?:[._-]|$)/i;
const noticeDirectory = /^(?:licen[sc]es?|copying|notices?)(?:[._-]|$)/i;
const ignoredDirectories = new Set([
  "node_modules",
  ".git",
  "target",
  "tests",
  "test",
  "fixtures",
]);

function inside(root, candidate) {
  const relative = path.relative(root, candidate);
  return (
    relative === "" ||
    (!relative.startsWith(`..${path.sep}`) &&
      relative !== ".." &&
      !path.isAbsolute(relative))
  );
}

function hasMachinePath(value, roots) {
  const normalized = value.replaceAll("\\", "/").toLowerCase();
  return roots.filter(Boolean).some((root) => {
    const prefix = path.resolve(root).replaceAll("\\", "/").toLowerCase();
    return prefix.length > 3 && normalized.includes(prefix);
  });
}

function errorCode(error) {
  return /^[A-Z][A-Z0-9_]+$/.test(error?.code ?? "")
    ? error.code
    : "COLLECTION_FAILED";
}

function failure(code) {
  return Object.assign(new Error(code), { code });
}

function declaration(value) {
  const raw = typeof value === "string" ? value : value?.type;
  // Declarations are identifiers, not paths or arbitrary registry text.
  return typeof raw === "string" &&
    /^[a-z0-9 .()+/\-]+$/i.test(raw) &&
    !raw.startsWith("/")
    ? raw
    : null;
}

export function selectCargoPackages(metadata) {
  const packages = new Map(metadata.packages.map((entry) => [entry.id, entry]));
  const nodes = new Map(
    metadata.resolve.nodes.map((entry) => [entry.id, entry]),
  );
  const root = metadata.resolve.root;
  if (!packages.has(root)) throw failure("CARGO_ROOT_MISSING");
  const selected = new Map([[root, new Set(["application"])]]);
  const pending = [root];
  while (pending.length) {
    const id = pending.pop();
    const node = nodes.get(id);
    if (!node) throw failure("CARGO_RESOLVE_NODE_MISSING");
    for (const dep of node.deps) {
      // cargo metadata --filter-platform has already filtered target conditions.
      const kinds = dep.dep_kinds
        .filter((entry) => entry.kind === null || entry.kind === "build")
        .map((entry) => entry.kind ?? "normal");
      if (!kinds.length) continue;
      if (!packages.has(dep.pkg)) throw failure("CARGO_PACKAGE_MISSING");
      if (!selected.has(dep.pkg)) {
        selected.set(dep.pkg, new Set());
        pending.push(dep.pkg);
      }
      for (const kind of kinds) selected.get(dep.pkg).add(kind);
    }
  }
  return [...selected].map(([id, kinds]) => ({
    ...packages.get(id),
    dependencyKinds: ordered(kinds),
  }));
}

function findNpmManifest(fromManifest, name) {
  const resolver = createRequire(fromManifest);
  // Resolve the package directory, even when package.json is hidden by exports.
  for (const lookup of resolver.resolve.paths(name) ?? []) {
    const candidate = path.join(lookup, name, "package.json");
    if (fs.existsSync(candidate)) return fs.realpathSync(candidate);
  }
  return null;
}

function applies(values, current) {
  if (!Array.isArray(values) || values.length === 0) return true;
  if (values.includes(`!${current}`)) return false;
  const positive = values.filter((value) => !value.startsWith("!"));
  return (
    positive.length === 0 ||
    positive.includes(current) ||
    positive.includes("any")
  );
}

export function collectNpmPackages(
  rootManifest,
  reportError,
  skipped,
  platform = "win32",
  arch = "x64",
) {
  const selected = [];
  const visited = new Set();
  const root = JSON.parse(fs.readFileSync(rootManifest, "utf8"));
  const pending = [{ manifest: rootManifest, data: root, application: true }];
  while (pending.length) {
    const parent = pending.pop();
    const dependencies = {
      ...parent.data.dependencies,
      ...parent.data.optionalDependencies,
    };
    // Include reachable peers (e.g. React for react-dom), but never devDependencies.
    for (const name of Object.keys(parent.data.peerDependencies ?? {}))
      dependencies[name] ??= parent.data.peerDependencies[name];
    for (const name of ordered(Object.keys(dependencies))) {
      const optional =
        Object.hasOwn(parent.data.optionalDependencies ?? {}, name) ||
        parent.data.peerDependenciesMeta?.[name]?.optional === true;
      if (!packageName.test(name)) {
        reportError("npm", "INVALID_PACKAGE_NAME");
        continue;
      }
      const manifest = findNpmManifest(parent.manifest, name);
      if (!manifest) {
        // A reachable missing optional package has no installed platform metadata.
        // Fail rather than silently assuming it is not applicable to Windows.
        reportError(
          `npm:${name}`,
          optional
            ? "REACHABLE_OPTIONAL_NOT_INSTALLED"
            : "DEPENDENCY_NOT_INSTALLED",
        );
        continue;
      }
      const data = JSON.parse(fs.readFileSync(manifest, "utf8"));
      if (!applies(data.os, platform) || !applies(data.cpu, arch)) {
        if (!optional)
          reportError(`npm:${name}`, "REQUIRED_DEPENDENCY_PLATFORM_MISMATCH");
        else
          skipped.push({
            ecosystem: "npm",
            name,
            reason: "installed manifest excludes the Windows x64 target",
          });
        continue;
      }
      if (
        data.name !== name ||
        data.private === true ||
        !packageVersion.test(data.version)
      ) {
        reportError(`npm:${name}`, "INVALID_PUBLIC_PACKAGE_IDENTITY");
        continue;
      }
      if (parent.application && dependencies[name] !== data.version) {
        reportError(`npm:${name}`, "ROOT_DEPENDENCY_PIN_MISMATCH");
      }
      if (visited.has(manifest)) continue;
      visited.add(manifest);
      selected.push({
        ecosystem: "npm",
        name,
        version: data.version,
        source: `https://www.npmjs.com/package/${name}/v/${data.version}`,
        declaredLicense: declaration(data.license),
        root: path.dirname(manifest),
        dependencyKinds: ["production"],
      });
      pending.push({ manifest, data, application: false });
    }
  }
  return selected;
}

function licenseCandidates(root, declaredFile, fileOnly) {
  const files = new Map();
  if (declaredFile) {
    const candidate = path.resolve(root, declaredFile);
    if (!inside(root, candidate)) throw failure("LICENSE_FILE_OUTSIDE_PACKAGE");
    files.set(candidate, "license");
  }
  if (fileOnly) return [...files];
  let entries = 0;
  function scan(directory, depth, inNotices = false) {
    if (depth > 16) throw failure("LICENSE_SCAN_DEPTH_LIMIT");
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      if (++entries > 100000) throw failure("LICENSE_SCAN_ENTRY_LIMIT");
      const full = path.join(directory, entry.name);
      if (entry.isSymbolicLink()) {
        if (licenseName.test(entry.name) || noticeName.test(entry.name))
          throw failure("LICENSE_SYMLINK_UNSUPPORTED");
        continue;
      }
      if (
        entry.isFile() &&
        (inNotices ||
          licenseName.test(entry.name) ||
          noticeName.test(entry.name))
      ) {
        files.set(full, noticeName.test(entry.name) ? "notice" : "license");
      } else if (entry.isDirectory() && !ignoredDirectories.has(entry.name)) {
        // Vendored native code may keep notices below crypto/, vendor/, etc.
        // Read only license-like files, but do not silently miss nested notices.
        scan(full, depth + 1, inNotices || noticeDirectory.test(entry.name));
      }
    }
  }
  scan(root, 0);
  return [...files].sort(([a], [b]) => a.localeCompare(b, "en"));
}

function readLicenseFiles(component, privateRoots) {
  const files = [];
  for (const [filename, kind] of licenseCandidates(
    component.root,
    component.licenseFile,
    component.licenseFileOnly,
  )) {
    if (!inside(component.root, fs.realpathSync(filename)))
      throw failure("LICENSE_FILE_OUTSIDE_PACKAGE");
    const bytes = fs.readFileSync(filename);
    if (!bytes.length || bytes.length > 2 * 1024 * 1024 || bytes.includes(0))
      throw failure("LICENSE_TEXT_INVALID");
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    if (hasMachinePath(text, [...privateRoots, component.root]))
      throw failure("LICENSE_CONTAINS_MACHINE_PATH");
    files.push({
      name: path.relative(component.root, filename).replaceAll("\\", "/"),
      kind,
      sha256: sha256(bytes),
      text,
    });
  }
  return files;
}

function supplementalIndex(repositoryRoot) {
  const filename = path.join(repositoryRoot, SUPPLEMENT_ROOT, "manifest.json");
  if (!fs.existsSync(filename)) return new Map();
  const manifest = JSON.parse(fs.readFileSync(filename, "utf8"));
  if (manifest.schemaVersion !== 1 || !Array.isArray(manifest.components))
    throw failure("SUPPLEMENT_MANIFEST_INVALID");
  const entries = new Map();
  for (const entry of manifest.components) {
    const id = `cargo:${entry.name}@${entry.version}`;
    if (
      !packageName.test(entry.name) ||
      !packageVersion.test(entry.version) ||
      !declaration(entry.declaredLicense) ||
      !/^[a-f0-9]{64}$/.test(entry.crateSha256) ||
      !(
        entry.upstreamCommit === null ||
        /^[a-f0-9]{40}$/.test(entry.upstreamCommit)
      ) ||
      !Array.isArray(entry.files) ||
      !entry.files.length ||
      entries.has(id)
    )
      throw failure("SUPPLEMENT_MANIFEST_INVALID");
    entries.set(id, entry);
  }
  return entries;
}

function publicSupplementSource(value) {
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    )
      return false;
    if (url.hostname === "raw.githubusercontent.com")
      return /^\/[^/]+\/[^/]+\/[a-f0-9]{40}\//.test(url.pathname);
    if (url.hostname === "www.mozilla.org")
      return url.pathname === "/media/MPL/2.0/index.txt";
    if (url.hostname === "www.apache.org")
      return url.pathname === "/licenses/LICENSE-2.0.txt";
    return ["docs.rs", "static.crates.io"].includes(url.hostname);
  } catch {
    return false;
  }
}

function registryArchive(component) {
  const packageRoot = path.resolve(component.root);
  const registryIdRoot = path.dirname(packageRoot);
  const srcRoot = path.dirname(registryIdRoot);
  const registryRoot = path.dirname(srcRoot);
  const registryId = path.basename(registryIdRoot);
  const packageDirectory = `${component.name}-${component.version}`;
  if (
    path.basename(packageRoot) !== packageDirectory ||
    path.basename(srcRoot) !== "src" ||
    path.basename(registryRoot) !== "registry" ||
    !/^[a-z0-9][a-z0-9._-]*$/i.test(registryId)
  )
    throw failure("SUPPLEMENT_REGISTRY_LAYOUT_INVALID");
  const realRegistry = fs.realpathSync(registryRoot);
  const realPackage = fs.realpathSync(packageRoot);
  const expectedPackage = path.join(
    realRegistry,
    "src",
    registryId,
    packageDirectory,
  );
  if (
    !inside(realRegistry, realPackage) ||
    path.relative(expectedPackage, realPackage) !== ""
  )
    throw failure("SUPPLEMENT_REGISTRY_PATH_OUTSIDE_ROOT");
  const expectedArchive = path.join(
    realRegistry,
    "cache",
    registryId,
    `${packageDirectory}.crate`,
  );
  if (!fs.existsSync(expectedArchive))
    throw failure("SUPPLEMENT_ARCHIVE_MISSING");
  const realArchive = fs.realpathSync(expectedArchive);
  if (
    !inside(realRegistry, realArchive) ||
    path.relative(expectedArchive, realArchive) !== ""
  )
    throw failure("SUPPLEMENT_REGISTRY_PATH_OUTSIDE_ROOT");
  if (!fs.statSync(realArchive).isFile())
    throw failure("SUPPLEMENT_ARCHIVE_NOT_FILE");
  return realArchive;
}

function readSupplement(component, entry, repositoryRoot, privateRoots) {
  if (entry.declaredLicense !== component.declaredLicense)
    throw failure("SUPPLEMENT_LICENSE_MISMATCH");
  // Bind an exception to the exact registry archive, not just a same-named package.
  // Regular registry sources use src/ and cache/ siblings. The per-directory
  // .cargo-checksum.json belongs to vendored directory sources, not this layout.
  if (sha256(fs.readFileSync(registryArchive(component))) !== entry.crateSha256)
    throw failure("SUPPLEMENT_CRATE_CHECKSUM_MISMATCH");
  if (entry.upstreamCommit !== null) {
    const vcs = JSON.parse(
      fs.readFileSync(
        path.join(component.root, ".cargo_vcs_info.json"),
        "utf8",
      ),
    );
    if (vcs.git?.sha1 !== entry.upstreamCommit)
      throw failure("SUPPLEMENT_COMMIT_MISMATCH");
  }
  const root = path.join(repositoryRoot, SUPPLEMENT_ROOT);
  return entry.files.map((file) => {
    if (
      typeof file.path !== "string" ||
      path.isAbsolute(file.path) ||
      !["license", "notice"].includes(file.kind) ||
      ![
        "upstream-file",
        "published-header",
        "published-file",
        "standard-license",
      ].includes(file.provenance) ||
      !/^[a-f0-9]{64}$/.test(file.sha256) ||
      !publicSupplementSource(file.source)
    )
      throw failure("SUPPLEMENT_FILE_INVALID");
    const filename = path.resolve(root, file.path);
    if (!inside(root, filename) || !inside(root, fs.realpathSync(filename)))
      throw failure("SUPPLEMENT_PATH_OUTSIDE_ROOT");
    const bytes = fs.readFileSync(filename);
    if (sha256(bytes) !== file.sha256)
      throw failure("SUPPLEMENT_TEXT_HASH_MISMATCH");
    if (!bytes.length || bytes.length > 2 * 1024 * 1024 || bytes.includes(0))
      throw failure("LICENSE_TEXT_INVALID");
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    if (hasMachinePath(text, privateRoots))
      throw failure("LICENSE_CONTAINS_MACHINE_PATH");
    return {
      name: `supplemental/${file.path}`,
      kind: file.kind,
      sha256: file.sha256,
      source: file.source,
      provenance: file.provenance,
      crateSha256: entry.crateSha256,
      sourceArchive: `https://static.crates.io/crates/${component.name}/${component.name}-${component.version}.crate`,
      upstreamCommit: entry.upstreamCommit,
      selectedLicense: entry.selectedLicense ?? null,
      text,
    };
  });
}

export function buildLicenseReport({
  repositoryRoot,
  cargoMetadata,
  privateRoots = [],
  npmManifest = path.join(repositoryRoot, NPM_ROOT, "package.json"),
  lockfiles = [
    "Cargo.lock",
    "apps/local-workbench/src-tauri/Cargo.lock",
    "apps/local-workbench/pnpm-lock.yaml",
  ],
}) {
  const errors = [];
  const skipped = [];
  const reportError = (component, code) => errors.push({ component, code });
  const roots = [repositoryRoot, ...privateRoots];
  const supplements = supplementalIndex(repositoryRoot);
  const candidates = collectNpmPackages(npmManifest, reportError, skipped);
  for (const pkg of selectCargoPackages(cargoMetadata)) {
    if (!packageName.test(pkg.name) || !packageVersion.test(pkg.version)) {
      reportError("cargo", "INVALID_PUBLIC_PACKAGE_IDENTITY");
      continue;
    }
    const local = pkg.source === null;
    if (local && !inside(repositoryRoot, pkg.manifest_path)) {
      reportError(
        `cargo:${pkg.name}@${pkg.version}`,
        "EXTERNAL_PATH_DEPENDENCY",
      );
      continue;
    }
    if (
      !local &&
      !/^registry\+https:\/\/(?:github\.com\/rust-lang\/crates\.io-index|index\.crates\.io\/?)$/.test(
        pkg.source,
      )
    ) {
      reportError(
        `cargo:${pkg.name}@${pkg.version}`,
        "UNREVIEWED_DEPENDENCY_SOURCE",
      );
      continue;
    }
    candidates.push({
      ecosystem: "cargo",
      name: pkg.name,
      version: pkg.version,
      source: local
        ? "MangaMonitor project"
        : `https://crates.io/crates/${pkg.name}/${pkg.version}`,
      declaredLicense:
        declaration(pkg.license) ??
        (pkg.license_file ? "LicenseRef-package-file" : null),
      root: local ? repositoryRoot : path.dirname(pkg.manifest_path),
      licenseFile: local ? "LICENSE" : pkg.license_file,
      licenseFileOnly: local,
      dependencyKinds: pkg.dependencyKinds,
    });
  }
  const components = new Map();
  for (const candidate of candidates) {
    const id = `${candidate.ecosystem}:${candidate.name}@${candidate.version}`;
    if (!candidate.declaredLicense)
      reportError(id, "LICENSE_DECLARATION_MISSING");
    let files = [];
    let readFailed = false;
    try {
      files = readLicenseFiles(candidate, roots);
    } catch (error) {
      readFailed = true;
      reportError(id, errorCode(error));
    }
    if (
      !readFailed &&
      !files.some((file) => file.kind === "license") &&
      supplements.has(id)
    ) {
      try {
        files.push(
          ...readSupplement(candidate, supplements.get(id), repositoryRoot, [
            ...roots,
            candidate.root,
          ]),
        );
      } catch (error) {
        reportError(id, errorCode(error));
      }
    }
    if (!files.some((file) => file.kind === "license"))
      reportError(id, "LICENSE_TEXT_MISSING");
    const component = {
      ecosystem: candidate.ecosystem,
      name: candidate.name,
      version: candidate.version,
      source: candidate.source,
      declaredLicense: candidate.declaredLicense,
      dependencyKinds: candidate.dependencyKinds,
      files,
    };
    const old = components.get(id);
    if (old) {
      const content = ({ dependencyKinds, ...entry }) => JSON.stringify(entry);
      if (content(old) !== content(component))
        reportError(id, "DUPLICATE_COMPONENT_CONTENT_MISMATCH");
      else
        old.dependencyKinds = ordered(
          new Set([...old.dependencyKinds, ...component.dependencyKinds]),
        );
    } else components.set(id, component);
  }
  const sorted = [...components]
    .sort(([a], [b]) => a.localeCompare(b, "en"))
    .map(([, value]) => value);
  const lockHashes = {};
  for (const relative of lockfiles) {
    if (
      path.isAbsolute(relative) ||
      !inside(repositoryRoot, path.resolve(repositoryRoot, relative))
    )
      throw failure("LOCK_PATH_INVALID");
    lockHashes[relative.replaceAll("\\", "/")] = sha256(
      fs.readFileSync(path.join(repositoryRoot, relative)),
    );
  }
  const inventory = {
    schemaVersion: 1,
    target: TARGET,
    scope:
      "Frozen installed npm production dependencies; locked Windows native application, normal and build dependencies. No dev-only edges. Build dependencies are included conservatively; this is not a claim that all are embedded in the executable.",
    lockfiles: lockHashes,
    components: sorted.map(({ files, ...component }) => ({
      ...component,
      licenseFiles: files.map(({ text, ...file }) => file),
    })),
    excludedOptionalDependencies: skipped,
    errors,
  };
  let text = `MangaMonitor dependency license texts\nTarget: ${TARGET}\n${inventory.scope}\n\nManual upstream reuse is covered by the separate THIRD_PARTY_NOTICES.md.\nExplicit archive-bound supplements are identified with their public sources below.\n\n`;
  for (const component of sorted) {
    text += `${"=".repeat(72)}\n${component.ecosystem}: ${component.name} ${component.version}\nSource: ${component.source}\nDeclared license: ${component.declaredLicense ?? "MISSING"}\nDependency kinds: ${component.dependencyKinds.join(", ")}\n`;
    for (const file of component.files) {
      text += `\n--- ${file.name} (${file.kind}; SHA-256 ${file.sha256}) ---\n`;
      if (file.source)
        text += `Text source: ${file.source}\nProvenance: ${file.provenance}\nPublished source archive: ${file.sourceArchive}\nRegistry archive SHA-256: ${file.crateSha256}\nUpstream release commit: ${file.upstreamCommit ?? "unknown; exact published registry archive is pinned"}\n${file.selectedLicense ? `Selected declared license option: ${file.selectedLicense}\n` : ""}`;
      text += `${file.text}\n`;
    }
    text += "\n";
  }
  if (errors.length)
    text += `COLLECTION INCOMPLETE: ${errors.length} error(s). See inventory.json. Do not package this output.\n`;
  if (hasMachinePath(JSON.stringify(inventory) + text, roots))
    throw failure("OUTPUT_CONTAINS_MACHINE_PATH");
  return { inventory, text };
}

function main() {
  if (process.env.GITHUB_ACTIONS !== "true" || process.platform !== "win32") {
    throw failure("WINDOWS_CI_REQUIRED");
  }
  const repositoryRoot = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
  );
  const args = process.argv.slice(2);
  if (args.length !== 0 && !(args.length === 2 && args[0] === "--output"))
    throw failure("INVALID_ARGUMENTS");
  const output = args.length
    ? path.resolve(args[1])
    : path.join(repositoryRoot, DEFAULT_OUTPUT);
  // Offline metadata reads manifests only; it does not compile or execute crates.
  const metadata = spawnSync(
    "cargo",
    [
      "metadata",
      "--format-version",
      "1",
      "--locked",
      "--offline",
      "--filter-platform",
      TARGET,
      "--manifest-path",
      path.join(repositoryRoot, NATIVE_MANIFEST),
    ],
    {
      cwd: repositoryRoot,
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      windowsHide: true,
    },
  );
  if (metadata.error || metadata.status !== 0)
    throw failure("CARGO_METADATA_FAILED");
  const report = buildLicenseReport({
    repositoryRoot,
    cargoMetadata: JSON.parse(metadata.stdout),
    privateRoots: [
      os.homedir(),
      os.tmpdir(),
      process.env.CARGO_HOME,
      process.env.PNPM_HOME,
    ],
  });
  fs.mkdirSync(output, { recursive: true });
  fs.writeFileSync(
    path.join(output, "inventory.json"),
    JSON.stringify(report.inventory, null, 2) + "\n",
  );
  fs.writeFileSync(path.join(output, "THIRD_PARTY_LICENSES.txt"), report.text);
  for (const error of report.inventory.errors)
    process.stderr.write(`${error.component}: ${error.code}\n`);
  if (report.inventory.errors.length) process.exitCode = 1;
  else
    process.stdout.write(
      `License collection complete: ${report.inventory.components.length} components; inventory.json and THIRD_PARTY_LICENSES.txt written.\n`,
    );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  try {
    main();
  } catch (error) {
    // Never emit raw fs/cargo errors, stdout, stderr or stack traces with paths.
    process.stderr.write(`License collection failed: ${errorCode(error)}\n`);
    process.exitCode = 1;
  }
}
