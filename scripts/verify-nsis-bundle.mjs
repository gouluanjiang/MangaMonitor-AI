import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const originalMarker = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_UNK");
const nsisMarker = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_NSS");
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

export function verifyInstalledNsisBinary(built, installed) {
  // The pinned bundler patches the first marker, packages it, then restores the
  // original build output. Compare every byte after that one exact transform.
  // https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle.rs#L32-L88
  // https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-bundler/src/bundle.rs#L119-L188
  const offset = built.indexOf(originalMarker);
  assert.notEqual(offset, -1, "Built executable has no Tauri bundle marker.");
  const expected = Buffer.from(built);
  nsisMarker.copy(expected, offset);
  if (!expected.equals(installed)) {
    let differingBytes = Math.abs(expected.length - installed.length);
    for (
      let index = 0;
      index < Math.min(expected.length, installed.length);
      index++
    ) {
      if (expected[index] !== installed[index]) differingBytes++;
    }
    throw Object.assign(
      new Error(
        "Installed executable differs beyond the pinned Tauri NSIS marker patch.",
      ),
      {
        diagnostics: {
          expectedBytes: expected.length,
          installedBytes: installed.length,
          expectedSha256: sha256(expected),
          installedSha256: sha256(installed),
          differingBytes,
        },
      },
    );
  }
  return {
    builtSha256: sha256(built),
    installedSha256: sha256(installed),
    permittedPatch:
      "first __TAURI_BUNDLE_TYPE_VAR_UNK to __TAURI_BUNDLE_TYPE_VAR_NSS",
    patchOffset: offset,
  };
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    assert.equal(
      process.argv.length,
      4,
      "Expected built and installed EXE paths.",
    );
    const [built, installed] = await Promise.all([
      readFile(process.argv[2]),
      readFile(process.argv[3]),
    ]);
    console.log(JSON.stringify(verifyInstalledNsisBinary(built, installed)));
  } catch (error) {
    console.error(
      JSON.stringify({
        status: "INSTALLER_BINARY_VERIFICATION_FAILED",
        reason: error.diagnostics
          ? "NSIS_BYTES_MISMATCH"
          : error.code === "ERR_ASSERTION"
            ? "INVALID_ARGUMENTS_OR_MISSING_MARKER"
            : "BINARY_READ_FAILED",
        diagnostics: error.diagnostics ?? null,
      }),
    );
    process.exitCode = 1;
  }
}
