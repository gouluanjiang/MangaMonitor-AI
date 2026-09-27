import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { verifyInstalledNsisBinary } from "./verify-nsis-bundle.mjs";

const marker = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_UNK");
const patchedMarker = Buffer.from("__TAURI_BUNDLE_TYPE_VAR_NSS");
const fixture = () =>
  Buffer.concat([
    Buffer.from([0x4d, 0x5a, 0, 0xff]),
    marker,
    Buffer.from([1, 2]),
  ]);
function patchFirst(bytes) {
  const patched = Buffer.from(bytes);
  patchedMarker.copy(patched, patched.indexOf(marker));
  return patched;
}

test("accepts only the exact NSIS patch and leaves the original bytes intact", () => {
  const built = fixture();
  const original = Buffer.from(built);
  const installed = patchFirst(built);
  const result = verifyInstalledNsisBinary(built, installed);
  assert.deepEqual(built, original);
  assert.equal(result.patchOffset, 4);
  assert.equal(
    result.installedSha256,
    createHash("sha256").update(installed).digest("hex"),
  );
  assert.notEqual(result.builtSha256, result.installedSha256);
});

test("rejects unrelated byte changes and truncated or appended installed data", () => {
  const built = fixture();
  for (const index of [0, built.length - 1]) {
    const changed = patchFirst(built);
    changed[index] ^= 1;
    assert.throws(
      () => verifyInstalledNsisBinary(built, changed),
      (error) => {
        assert.match(error.message, /differs beyond/);
        assert.equal(error.diagnostics.differingBytes, 1);
        assert.equal(error.diagnostics.expectedBytes, built.length);
        assert.equal(error.diagnostics.installedBytes, changed.length);
        return true;
      },
    );
  }
  const installed = patchFirst(built);
  assert.throws(
    () => verifyInstalledNsisBinary(built, installed.subarray(0, -1)),
    /differs beyond/,
  );
  assert.throws(
    () =>
      verifyInstalledNsisBinary(
        built,
        Buffer.concat([installed, Buffer.alloc(1)]),
      ),
    /differs beyond/,
  );
});

test("rejects an unpatched installer or a missing original marker", () => {
  const built = fixture();
  assert.throws(
    () => verifyInstalledNsisBinary(built, built),
    /differs beyond/,
  );
  assert.throws(
    () =>
      verifyInstalledNsisBinary(
        Buffer.from("no marker"),
        Buffer.from("no marker"),
      ),
    /no Tauri bundle marker/,
  );
});

test("matches the pinned bundler's first occurrence rule without normalizing other bytes", () => {
  const built = Buffer.concat([fixture(), marker]);
  const installed = patchFirst(built);
  verifyInstalledNsisBinary(built, installed);
  patchedMarker.copy(installed, built.lastIndexOf(marker));
  assert.throws(
    () => verifyInstalledNsisBinary(built, installed),
    /differs beyond/,
  );
});
