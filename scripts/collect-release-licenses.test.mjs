import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import {
  buildLicenseReport,
  selectCargoPackages,
} from "./collect-release-licenses.mjs";

const MIT =
  "MIT License\nCopyright (c) Synthetic contributors\nPermission is hereby granted, free of charge.\n";

function fixture(t) {
  const root = fs.mkdtempSync(
    path.join(os.tmpdir(), "mangamonitor-license-fixture-"),
  );
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const write = (relative, data) => {
    const filename = path.join(root, relative);
    fs.mkdirSync(path.dirname(filename), { recursive: true });
    fs.writeFileSync(
      filename,
      typeof data === "string" ? data : JSON.stringify(data),
    );
    return filename;
  };
  write("LICENSE", MIT);
  const npmManifest = write("frontend/package.json", {
    name: "application",
    private: true,
    dependencies: { alpha: "1.0.0", beta: "1.0.0" },
    devDependencies: { "unused-tool": "1.0.0" },
  });
  write("frontend/node_modules/alpha/package.json", {
    name: "alpha",
    version: "1.0.0",
    license: "MIT",
    dependencies: { shared: "1.0.0" },
  });
  write("frontend/node_modules/beta/package.json", {
    name: "beta",
    version: "1.0.0",
    license: "MIT",
    dependencies: { shared: "1.0.0" },
  });
  write("frontend/node_modules/shared/package.json", {
    name: "shared",
    version: "1.0.0",
    license: "MIT",
  });
  for (const name of ["alpha", "beta", "shared"])
    write(`frontend/node_modules/${name}/LICENSE`, MIT);
  const cargoPackage = (name, local = false) => ({
    id: name,
    name,
    version: "1.0.0",
    source: local
      ? null
      : "registry+https://github.com/rust-lang/crates.io-index",
    license: "MIT",
    license_file: null,
    manifest_path: write(
      `${local ? "native" : `registry/${name}`}/Cargo.toml`,
      "[package]\n",
    ),
  });
  const packages = [
    cargoPackage("application", true),
    cargoPackage("runtime"),
    cargoPackage("build-helper"),
    cargoPackage("dev-helper"),
  ];
  for (const name of ["runtime", "build-helper"])
    write(`registry/${name}/LICENSE`, MIT);
  const dependency = (pkg, kind) => ({
    pkg,
    dep_kinds: [{ kind, target: null }],
  });
  const cargoMetadata = {
    packages,
    resolve: {
      root: "application",
      nodes: [
        {
          id: "application",
          deps: [
            dependency("runtime", null),
            dependency("build-helper", "build"),
            dependency("dev-helper", "dev"),
          ],
        },
        { id: "runtime", deps: [dependency("build-helper", "build")] },
        { id: "build-helper", deps: [] },
        { id: "dev-helper", deps: [] },
      ],
    },
  };
  const run = () =>
    buildLicenseReport({
      repositoryRoot: root,
      npmManifest,
      cargoMetadata,
      lockfiles: [],
      privateRoots: [root],
    });
  return { root, write, npmManifest, cargoMetadata, run };
}

test("deduplicates reachable packages, keeps native normal/build and excludes dev-only dependencies", (t) => {
  const f = fixture(t);
  const report = f.run();
  assert.deepEqual(report.inventory.errors, []);
  assert.deepEqual(
    selectCargoPackages(f.cargoMetadata)
      .map((entry) => entry.name)
      .sort(),
    ["application", "build-helper", "runtime"],
  );
  assert.equal(
    report.inventory.components.filter((entry) => entry.name === "shared")
      .length,
    1,
  );
  assert.equal(report.inventory.components.length, 6);
  assert.equal(
    report.inventory.components.some(
      (entry) => entry.name === "unused-tool" || entry.name === "dev-helper",
    ),
    false,
  );
  const output = JSON.stringify(report.inventory) + report.text;
  assert.equal(output.includes(f.root), false);
  assert.equal(output.includes(f.root.replaceAll("\\", "/")), false);
  assert.equal(output.includes("manifest_path"), false);
  assert.match(report.text, /MIT License/);
});

test("missing license text fails explicitly even when a SPDX declaration exists", (t) => {
  const f = fixture(t);
  fs.unlinkSync(path.join(f.root, "frontend/node_modules/alpha/LICENSE"));
  const report = f.run();
  assert.deepEqual(report.inventory.errors, [
    { component: "npm:alpha@1.0.0", code: "LICENSE_TEXT_MISSING" },
  ]);
  assert.match(report.text, /COLLECTION INCOMPLETE/);
});

test("includes nested vendored notices without copying unrelated source files", (t) => {
  const f = fixture(t);
  f.write("registry/runtime/vendor/crypto/LICENSE", MIT);
  f.write(
    "registry/runtime/vendor/crypto/NOTICE",
    "Synthetic public attribution\n",
  );
  f.write("registry/runtime/vendor/crypto/source.c", "not a license\n");
  const report = f.run();
  assert.deepEqual(report.inventory.errors, []);
  const runtime = report.inventory.components.find(
    (entry) => entry.name === "runtime",
  );
  assert.deepEqual(
    runtime.licenseFiles.map((entry) => entry.name),
    ["LICENSE", "vendor/crypto/LICENSE", "vendor/crypto/NOTICE"],
  );
  assert.equal(report.text.includes("not a license"), false);
});

test("missing declarations and reachable missing optional dependencies do not silently pass", (t) => {
  const f = fixture(t);
  f.write("frontend/node_modules/alpha/package.json", {
    name: "alpha",
    version: "1.0.0",
    optionalDependencies: { "missing-optional": "1.0.0" },
  });
  const report = f.run();
  assert.ok(
    report.inventory.errors.some(
      (entry) => entry.code === "LICENSE_DECLARATION_MISSING",
    ),
  );
  assert.ok(
    report.inventory.errors.some(
      (entry) =>
        entry.component === "npm:missing-optional" &&
        entry.code === "REACHABLE_OPTIONAL_NOT_INSTALLED",
    ),
  );
});

test("an installed optional package with an explicit incompatible platform is recorded and excluded", (t) => {
  const f = fixture(t);
  f.write("frontend/node_modules/alpha/package.json", {
    name: "alpha",
    version: "1.0.0",
    license: "MIT",
    optionalDependencies: { "linux-only": "1.0.0" },
  });
  f.write("frontend/node_modules/linux-only/package.json", {
    name: "linux-only",
    version: "1.0.0",
    license: "MIT",
    os: ["linux"],
  });
  const report = f.run();
  assert.deepEqual(report.inventory.errors, []);
  assert.deepEqual(report.inventory.excludedOptionalDependencies, [
    {
      ecosystem: "npm",
      name: "linux-only",
      reason: "installed manifest excludes the Windows x64 target",
    },
  ]);
});

test("machine paths in license contents are rejected without leaking them into diagnostics", (t) => {
  const f = fixture(t);
  f.write(
    "frontend/node_modules/alpha/LICENSE",
    `${MIT}\nGenerated in ${f.root}\n`,
  );
  const report = f.run();
  assert.ok(
    report.inventory.errors.some(
      (entry) => entry.code === "LICENSE_CONTAINS_MACHINE_PATH",
    ),
  );
  assert.equal(
    (JSON.stringify(report.inventory) + report.text).includes(f.root),
    false,
  );
});

test("declared license files cannot escape a dependency package", (t) => {
  const f = fixture(t);
  f.cargoMetadata.packages.find(
    (entry) => entry.name === "runtime",
  ).license_file = "../../LICENSE";
  const report = f.run();
  assert.ok(
    report.inventory.errors.some(
      (entry) => entry.code === "LICENSE_FILE_OUTSIDE_PACKAGE",
    ),
  );
  assert.equal(
    (JSON.stringify(report.inventory) + report.text).includes(f.root),
    false,
  );
});

function supplement(f) {
  const hash = createHash("sha256").update(MIT).digest("hex");
  const row = {
    name: "runtime",
    version: "1.0.0",
    declaredLicense: "MIT",
    crateSha256: "b".repeat(64),
    upstreamCommit: "a".repeat(40),
    files: [
      {
        path: "runtime/LICENSE",
        kind: "license",
        provenance: "upstream-file",
        sha256: hash,
        source: `https://raw.githubusercontent.com/example/runtime/${"a".repeat(40)}/LICENSE`,
      },
    ],
  };
  fs.unlinkSync(path.join(f.root, "registry/runtime/LICENSE"));
  f.write("registry/runtime/.cargo-checksum.json", {
    package: row.crateSha256,
  });
  f.write("registry/runtime/.cargo_vcs_info.json", {
    git: { sha1: row.upstreamCommit },
  });
  f.write("third-party/dependency-licenses/runtime/LICENSE", MIT);
  const save = () =>
    f.write("third-party/dependency-licenses/manifest.json", {
      schemaVersion: 1,
      components: [row],
    });
  save();
  return { row, save };
}

test("an explicit exact-archive supplement restores omitted text and publishes its provenance", (t) => {
  const f = fixture(t);
  supplement(f);
  const report = f.run();
  assert.deepEqual(report.inventory.errors, []);
  const runtime = report.inventory.components.find(
    (entry) => entry.name === "runtime",
  );
  assert.equal(runtime.licenseFiles[0].provenance, "upstream-file");
  assert.equal(runtime.licenseFiles[0].crateSha256, "b".repeat(64));
  assert.match(
    report.text,
    /Text source: https:\/\/raw\.githubusercontent\.com/,
  );
});

test("supplements never excuse version, SPDX, archive, commit or text drift", async (t) => {
  const cases = [
    [
      "version",
      "LICENSE_TEXT_MISSING",
      (f) => {
        f.cargoMetadata.packages.find(
          (entry) => entry.name === "runtime",
        ).version = "1.0.1";
      },
    ],
    [
      "license",
      "SUPPLEMENT_LICENSE_MISMATCH",
      (f) => {
        f.cargoMetadata.packages.find(
          (entry) => entry.name === "runtime",
        ).license = "Apache-2.0";
      },
    ],
    [
      "archive",
      "SUPPLEMENT_CRATE_CHECKSUM_MISMATCH",
      (f) =>
        f.write("registry/runtime/.cargo-checksum.json", {
          package: "c".repeat(64),
        }),
    ],
    [
      "commit",
      "SUPPLEMENT_COMMIT_MISMATCH",
      (f) =>
        f.write("registry/runtime/.cargo_vcs_info.json", {
          git: { sha1: "c".repeat(40) },
        }),
    ],
    [
      "text",
      "SUPPLEMENT_TEXT_HASH_MISMATCH",
      (f) =>
        f.write(
          "third-party/dependency-licenses/runtime/LICENSE",
          MIT + "altered\n",
        ),
    ],
  ];
  for (const [name, code, change] of cases) {
    await t.test(name, (child) => {
      const f = fixture(child);
      supplement(f);
      change(f);
      const report = f.run();
      assert.ok(report.inventory.errors.some((entry) => entry.code === code));
      assert.match(report.text, /COLLECTION INCOMPLETE/);
    });
  }
});

test("an unrelated missing package license still fails when a known supplement succeeds", (t) => {
  const f = fixture(t);
  supplement(f);
  fs.unlinkSync(path.join(f.root, "registry/build-helper/LICENSE"));
  const report = f.run();
  assert.deepEqual(report.inventory.errors, [
    { component: "cargo:build-helper@1.0.0", code: "LICENSE_TEXT_MISSING" },
  ]);
});

test("a reviewed legacy archive can use an explicitly identified standard license without inventing a commit", (t) => {
  const f = fixture(t);
  const pinned = supplement(f);
  const declaration = "MIT OR Apache-2.0";
  f.cargoMetadata.packages.find((entry) => entry.name === "runtime").license =
    declaration;
  pinned.row.declaredLicense = declaration;
  pinned.row.upstreamCommit = null;
  pinned.row.selectedLicense = "Apache-2.0";
  const standardText = "Apache License\nVersion 2.0\nSynthetic test text.\n";
  f.write("third-party/dependency-licenses/runtime/LICENSE", standardText);
  pinned.row.files[0].sha256 = createHash("sha256")
    .update(standardText)
    .digest("hex");
  pinned.row.files[0].provenance = "standard-license";
  pinned.row.files[0].source =
    "https://www.apache.org/licenses/LICENSE-2.0.txt";
  const publishedManifest =
    '[package]\nname = "runtime"\nlicense = "MIT OR Apache-2.0"\n';
  f.write(
    "third-party/dependency-licenses/runtime/PUBLISHED-Cargo.toml",
    publishedManifest,
  );
  pinned.row.files.push({
    path: "runtime/PUBLISHED-Cargo.toml",
    kind: "notice",
    provenance: "published-file",
    source: "https://static.crates.io/crates/runtime/runtime-1.0.0.crate",
    sha256: createHash("sha256").update(publishedManifest).digest("hex"),
  });
  pinned.save();
  fs.unlinkSync(path.join(f.root, "registry/runtime/.cargo_vcs_info.json"));
  const report = f.run();
  assert.deepEqual(report.inventory.errors, []);
  assert.match(
    report.text,
    /Upstream release commit: unknown; exact published registry archive is pinned/,
  );
  assert.equal(
    report.inventory.components.find((entry) => entry.name === "runtime")
      .licenseFiles[0].upstreamCommit,
    null,
  );
  assert.equal(
    report.inventory.components.find((entry) => entry.name === "runtime")
      .licenseFiles[1].provenance,
    "published-file",
  );
});
