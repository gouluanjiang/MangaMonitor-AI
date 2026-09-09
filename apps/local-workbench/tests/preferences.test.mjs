import test from "node:test";
import assert from "node:assert/strict";
import {
  initialPreferences,
  isBackgroundDataUrl,
  isWorkbenchPreferences,
  MAX_BACKGROUND_BYTES,
  PREFERENCES_STORAGE_KEY,
  readBackgroundFile,
  readPreferences,
  resourcePreset,
  restoreDefaultBackground,
  restorePreferences,
  savePreferences,
} from "../src/preferences.ts";

const pngBase64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
const pngDataUrl = `data:image/png;base64,${pngBase64}`;
const pngBytes = Buffer.from(pngBase64, "base64");
const withBackground = () => ({
  ...initialPreferences(),
  appearance: {
    backgroundMode: "A",
    density: 5,
    backgroundImage: pngDataUrl,
    backgroundName: "背景.png",
  },
});

function storageWith(raw = null) {
  const values = new Map(raw === null ? [] : [[PREFERENCES_STORAGE_KEY, raw]]);
  return {
    values,
    getItem(key) {
      return values.get(key) ?? null;
    },
    setItem(key, value) {
      values.set(key, value);
    },
  };
}

function imageFile(overrides = {}) {
  return {
    name: "背景.png",
    type: "image/png",
    size: pngBytes.length,
    async arrayBuffer() {
      return Uint8Array.from(pngBytes).buffer;
    },
    ...overrides,
  };
}

test("defaults remember B, 7 covers and two distinct resource budgets", () => {
  const first = initialPreferences();
  assert.deepEqual(first, {
    version: 1,
    appearance: {
      backgroundMode: "B",
      density: 7,
      backgroundImage: null,
      backgroundName: null,
    },
    resources: {
      profile: "balanced",
      simultaneousWorks: 2,
      imageRequests: 4,
    },
  });
  first.appearance.density = 9;
  first.resources.imageRequests = 1;
  assert.equal(initialPreferences().appearance.density, 7);
  assert.equal(initialPreferences().resources.imageRequests, 4);
});

test("strict restore rejects foreign schema, corrupt state and forged resource profiles", () => {
  const defaults = initialPreferences();
  const invalid = [
    null,
    "not json",
    "[]",
    JSON.stringify({ ...defaults, version: 2 }),
    JSON.stringify({ ...defaults, tasks: [] }),
    JSON.stringify({
      ...defaults,
      appearance: { ...defaults.appearance, density: "7" },
    }),
    JSON.stringify({
      ...defaults,
      appearance: { ...defaults.appearance, density: 6 },
    }),
    JSON.stringify({
      ...defaults,
      appearance: { ...defaults.appearance, backgroundMode: "C" },
    }),
    JSON.stringify({
      ...defaults,
      appearance: { ...defaults.appearance, backgroundName: "missing.png" },
    }),
    JSON.stringify({
      ...defaults,
      resources: {
        profile: "balanced",
        simultaneousWorks: 4,
        imageRequests: 8,
      },
    }),
    JSON.stringify({
      ...defaults,
      resources: { profile: "custom", simultaneousWorks: 0, imageRequests: 4 },
    }),
    JSON.stringify({
      ...defaults,
      resources: { profile: "custom", simultaneousWorks: 2, imageRequests: 9 },
    }),
    JSON.stringify({
      ...defaults,
      resources: {
        profile: "custom",
        simultaneousWorks: 1.5,
        imageRequests: 4,
      },
    }),
  ];
  for (const raw of invalid)
    assert.deepEqual(restorePreferences(raw), defaults);
  assert.deepEqual(restorePreferences(" ".repeat(3_000_000)), defaults);
});

test("resource presets are bounded and custom values remain distinct", () => {
  assert.deepEqual(resourcePreset("economy"), {
    profile: "economy",
    simultaneousWorks: 1,
    imageRequests: 2,
  });
  const preferences = {
    ...initialPreferences(),
    resources: { profile: "custom", simultaneousWorks: 4, imageRequests: 1 },
  };
  assert.equal(isWorkbenchPreferences(preferences), true);
  assert.deepEqual(
    restorePreferences(JSON.stringify(preferences)),
    preferences,
  );
});

test("background restore permits only bounded raster data with a matching signature", () => {
  assert.equal(isBackgroundDataUrl(pngDataUrl), true);
  const invalid = [
    "https://example.com/background.png",
    "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=",
    "data:text/html;base64,PHNjcmlwdD48L3NjcmlwdD4=",
    "data:image/png;base64,PHN2Zz48L3N2Zz4=",
    `data:image/jpeg;base64,${pngBase64}`,
    "data:image/png;base64,not base64",
    `data:image/png;base64,${Buffer.alloc(MAX_BACKGROUND_BYTES + 1).toString("base64")}`,
  ];
  for (const value of invalid) assert.equal(isBackgroundDataUrl(value), false);
  for (const backgroundImage of invalid) {
    const preferences = withBackground();
    preferences.appearance.backgroundImage = backgroundImage;
    assert.deepEqual(
      restorePreferences(JSON.stringify(preferences)),
      initialPreferences(),
    );
  }
});

test("preference writes use a key separate from the demonstration queue", async () => {
  const storage = storageWith();
  storage.setItem("mangamonitor.workbench.demo.v1", "queue sentinel");
  const preferences = withBackground();
  assert.equal(savePreferences(preferences, storage), true);
  assert.equal(
    storage.getItem("mangamonitor.workbench.demo.v1"),
    "queue sentinel",
  );
  let decodeCalls = 0;
  const result = await readPreferences(storage, async (dataUrl) => {
    decodeCalls += 1;
    assert.equal(dataUrl, pngDataUrl);
    return true;
  });
  assert.equal(decodeCalls, 1);
  assert.equal(result.storageFailed, false);
  assert.equal(result.backgroundFailed, false);
  assert.deepEqual(result.preferences, preferences);
});

test("failed stored-image decoding preserves its recoverable configuration", async () => {
  const preferences = withBackground();
  const storage = storageWith(JSON.stringify(preferences));
  for (const decode of [
    async () => false,
    async () => {
      throw new Error("decoder rejected");
    },
  ]) {
    const result = await readPreferences(storage, decode);
    assert.deepEqual(result.preferences, preferences);
    assert.equal(result.storageFailed, false);
    assert.equal(result.backgroundFailed, true);
    assert.equal(
      storage.getItem(PREFERENCES_STORAGE_KEY),
      JSON.stringify(preferences),
    );
  }
});

test("saving resource settings after a decode failure does not erase the background", async () => {
  const preferences = withBackground();
  const storage = storageWith(JSON.stringify(preferences));
  const failed = await readPreferences(storage, async () => false);
  assert.equal(failed.backgroundFailed, true);
  const next = {
    ...failed.preferences,
    resources: resourcePreset("economy"),
  };
  assert.equal(savePreferences(next, storage), true);
  const recovered = await readPreferences(
    storage,
    async (dataUrl) => dataUrl === pngDataUrl,
  );
  assert.equal(recovered.backgroundFailed, false);
  assert.equal(recovered.storageFailed, false);
  assert.deepEqual(recovered.preferences.appearance, preferences.appearance);
  assert.deepEqual(recovered.preferences.resources, resourcePreset("economy"));
});
test("storage access and quota failure never mutate the supplied draft", async () => {
  const broken = {
    getItem() {
      throw new Error("storage denied");
    },
    setItem() {
      throw new Error("quota exceeded");
    },
  };
  const draft = withBackground();
  const before = JSON.stringify(draft);
  assert.equal(savePreferences(draft, broken), false);
  assert.equal(JSON.stringify(draft), before);
  assert.deepEqual(await readPreferences(broken), {
    preferences: initialPreferences(),
    storageFailed: true,
    backgroundFailed: false,
  });
  const storage = storageWith("previous value");
  assert.equal(savePreferences({ ...draft, version: 2 }, storage), false);
  assert.equal(storage.getItem(PREFERENCES_STORAGE_KEY), "previous value");
});

test("restoring default background retains density, mode and original draft", () => {
  const original = withBackground().appearance;
  const restored = restoreDefaultBackground(original);
  assert.deepEqual(restored, {
    backgroundMode: "A",
    density: 5,
    backgroundImage: null,
    backgroundName: null,
  });
  assert.equal(original.backgroundImage, pngDataUrl);
});

test("image selection checks type, byte size and contents before invoking the decoder", async () => {
  let reads = 0;
  let decodes = 0;
  const decode = async () => {
    decodes += 1;
    return true;
  };
  const arrayBuffer = async () => {
    reads += 1;
    return new ArrayBuffer(16);
  };
  await assert.rejects(
    readBackgroundFile(
      imageFile({ type: "image/svg+xml", arrayBuffer }),
      decode,
    ),
    /PNG/,
  );
  await assert.rejects(
    readBackgroundFile(imageFile({ type: "text/html", arrayBuffer }), decode),
    /PNG/,
  );
  await assert.rejects(
    readBackgroundFile(
      imageFile({ size: MAX_BACKGROUND_BYTES + 1, arrayBuffer }),
      decode,
    ),
    /2 MiB/,
  );
  await assert.rejects(
    readBackgroundFile(imageFile({ size: 0, arrayBuffer }), decode),
    /2 MiB/,
  );
  assert.equal(reads, 0);
  assert.equal(decodes, 0);
  await assert.rejects(
    readBackgroundFile(imageFile({ size: 16, arrayBuffer }), decode),
    /内容与格式/,
  );
  assert.equal(reads, 1);
  assert.equal(decodes, 0);
});

test("read and decode failures return no replacement image", async () => {
  await assert.rejects(
    readBackgroundFile(
      imageFile({
        async arrayBuffer() {
          throw new Error("file disappeared");
        },
      }),
    ),
    /原背景已保留/,
  );
  await assert.rejects(
    readBackgroundFile(imageFile(), async () => false),
    /无法解码/,
  );
  await assert.rejects(
    readBackgroundFile(imageFile(), async () => {
      throw new Error("bad image");
    }),
    /无法解码/,
  );
});

test("valid selection waits for decoding and normalizes its display name", async () => {
  let finishDecode;
  const decoding = new Promise((resolve) => {
    finishDecode = resolve;
  });
  let settled = false;
  const result = readBackgroundFile(
    imageFile({ name: "  背景\u0000.png  " }),
    async () => decoding,
  ).then((value) => {
    settled = true;
    return value;
  });
  await Promise.resolve();
  assert.equal(settled, false);
  finishDecode(true);
  assert.deepEqual(await result, {
    backgroundImage: pngDataUrl,
    backgroundName: "背景.png",
  });
});

test("new background names strip Unicode controls and separators with JavaScript trim", async () => {
  const selected = await readBackgroundFile(
    imageFile({ name: " \uFEFF a/\u0085b\\c.png \uFEFF " }),
    async () => true,
  );
  assert.equal(selected.backgroundName, "abc.png");
  const innerBom = await readBackgroundFile(
    imageFile({ name: " \uFEFFa\uFEFFb.png\uFEFF " }),
    async () => true,
  );
  assert.equal(innerBom.backgroundName, "a\uFEFFb.png");
  const blank = await readBackgroundFile(
    imageFile({ name: " \uFEFF/\\\u0085 " }),
    async () => true,
  );
  assert.equal(blank.backgroundName, "自定义背景");
});

test("new background names respect UTF-16 limits without splitting a character", async () => {
  for (const prefix of [178, 179]) {
    const selected = await readBackgroundFile(
      imageFile({ name: "x".repeat(prefix) + "😀.png" }),
      async () => true,
    );
    const expected = "x".repeat(prefix) + (prefix === 178 ? "😀" : "");
    assert.equal(selected.backgroundName, expected);
    assert.ok(selected.backgroundName.length <= 180);
    const preferences = withBackground();
    preferences.appearance.backgroundName = selected.backgroundName;
    assert.equal(isWorkbenchPreferences(preferences), true);
  }
});

test("legacy slash and C1 background names remain readable and are never silently rewritten", async () => {
  const preferences = withBackground();
  preferences.appearance.backgroundName = "folder/legacy\\old\u0085name.png";
  const original = JSON.stringify(preferences);
  const storage = storageWith(original);
  assert.equal(isWorkbenchPreferences(preferences), true);
  const loaded = await readPreferences(storage, async () => true);
  assert.deepEqual(loaded.preferences, preferences);
  assert.equal(loaded.backgroundFailed, false);
  assert.equal(storage.getItem(PREFERENCES_STORAGE_KEY), original);
  const next = { ...loaded.preferences, resources: resourcePreset("economy") };
  assert.equal(savePreferences(next, storage), true);
  const reloaded = await readPreferences(storage, async () => true);
  assert.deepEqual(reloaded.preferences.appearance, preferences.appearance);
});
