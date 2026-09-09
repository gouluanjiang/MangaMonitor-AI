export type BackgroundMode = "A" | "B";
export type CoverDensity = 5 | 7 | 9;
export type ResourceProfile = "economy" | "balanced" | "custom";

export interface AppearancePreferences {
  backgroundMode: BackgroundMode;
  density: CoverDensity;
  backgroundImage: string | null;
  backgroundName: string | null;
}

export interface ResourcePreferences {
  profile: ResourceProfile;
  simultaneousWorks: number;
  imageRequests: number;
}

export interface WorkbenchPreferences {
  version: 1;
  appearance: AppearancePreferences;
  resources: ResourcePreferences;
}

export interface PreferencesStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const PREFERENCES_STORAGE_KEY = "mangamonitor.workbench.preferences.v1";
export const MAX_BACKGROUND_BYTES = 2 * 1024 * 1024;
const maxEncodedLength = Math.ceil(MAX_BACKGROUND_BYTES / 3) * 4;
const maxStoredLength = maxEncodedLength + 2048;
const maxBackgroundDimension = 8192;
const maxBackgroundPixels = 24_000_000;
const imageTypes = new Set(["image/png", "image/jpeg", "image/webp"]);

export function resourcePreset(
  profile: "economy" | "balanced",
): ResourcePreferences {
  return profile === "economy"
    ? { profile, simultaneousWorks: 1, imageRequests: 2 }
    : { profile, simultaneousWorks: 2, imageRequests: 4 };
}

export function initialPreferences(): WorkbenchPreferences {
  return {
    version: 1,
    appearance: {
      backgroundMode: "B",
      density: 7,
      backgroundImage: null,
      backgroundName: null,
    },
    resources: resourcePreset("balanced"),
  };
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  return (
    Object.keys(value).length === keys.length &&
    keys.every((key) => Object.prototype.hasOwnProperty.call(value, key))
  );
}

function hasImageSignature(
  bytes: Uint8Array,
  type: string,
  maxBytes = MAX_BACKGROUND_BYTES,
): boolean {
  if (bytes.length < 12 || bytes.length > maxBytes) return false;
  if (type === "image/png") {
    return [137, 80, 78, 71, 13, 10, 26, 10].every(
      (byte, index) => bytes[index] === byte,
    );
  }
  if (type === "image/jpeg") {
    return bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255;
  }
  return (
    type === "image/webp" &&
    [82, 73, 70, 70].every((byte, index) => bytes[index] === byte) &&
    [87, 69, 66, 80].every((byte, index) => bytes[index + 8] === byte)
  );
}

// A data URL is never accepted on its MIME declaration alone. Browser decoding
// is a separate asynchronous requirement when selecting or restoring an image.
export function isBackgroundDataUrl(
  value: unknown,
  maxBytes = MAX_BACKGROUND_BYTES,
): value is string {
  if (
    typeof value !== "string" ||
    value.length > Math.ceil(maxBytes / 3) * 4 + 32
  )
    return false;
  const match =
    /^data:(image\/(?:png|jpeg|webp));base64,([A-Za-z0-9+/]+={0,2})$/.exec(
      value,
    );
  if (!match || match[2].length % 4 !== 0) return false;
  try {
    const decoded = atob(match[2]);
    if (decoded.length > maxBytes) return false;
    const bytes = Uint8Array.from(decoded, (character) =>
      character.charCodeAt(0),
    );
    return hasImageSignature(bytes, match[1], maxBytes);
  } catch {
    return false;
  }
}

// Preserve readable legacy browser names, including slash and C1 characters.
// New selections are normalized below; native writes enforce their own rules.
function validName(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value.length <= 180 &&
    value.trim() === value &&
    !/[\u0000-\u001f\u007f]/.test(value)
  );
}

function validAppearance(
  value: unknown,
  maxBytes = MAX_BACKGROUND_BYTES,
): value is AppearancePreferences {
  if (
    !isObject(value) ||
    !exactKeys(value, [
      "backgroundMode",
      "density",
      "backgroundImage",
      "backgroundName",
    ]) ||
    (value.backgroundMode !== "A" && value.backgroundMode !== "B") ||
    ![5, 7, 9].includes(value.density as number)
  )
    return false;
  return value.backgroundImage === null
    ? value.backgroundName === null
    : isBackgroundDataUrl(value.backgroundImage, maxBytes) &&
        validName(value.backgroundName);
}

function validResources(value: unknown): value is ResourcePreferences {
  if (
    !isObject(value) ||
    !exactKeys(value, ["profile", "simultaneousWorks", "imageRequests"]) ||
    !["economy", "balanced", "custom"].includes(value.profile as string) ||
    typeof value.simultaneousWorks !== "number" ||
    !Number.isInteger(value.simultaneousWorks) ||
    value.simultaneousWorks < 1 ||
    value.simultaneousWorks > 4 ||
    typeof value.imageRequests !== "number" ||
    !Number.isInteger(value.imageRequests) ||
    value.imageRequests < 1 ||
    value.imageRequests > 8
  )
    return false;
  if (value.profile === "custom") return true;
  const preset = resourcePreset(value.profile as "economy" | "balanced");
  return (
    value.simultaneousWorks === preset.simultaneousWorks &&
    value.imageRequests === preset.imageRequests
  );
}

export function isWorkbenchPreferences(
  value: unknown,
  maxBytes = MAX_BACKGROUND_BYTES,
): value is WorkbenchPreferences {
  return (
    isObject(value) &&
    exactKeys(value, ["version", "appearance", "resources"]) &&
    value.version === 1 &&
    validAppearance(value.appearance, maxBytes) &&
    validResources(value.resources)
  );
}

export function restorePreferences(raw: string | null): WorkbenchPreferences {
  if (typeof raw !== "string" || raw.length > maxStoredLength)
    return initialPreferences();
  try {
    const value: unknown = JSON.parse(raw);
    return isWorkbenchPreferences(value) ? value : initialPreferences();
  } catch {
    return initialPreferences();
  }
}

export type BackgroundDecoder = (dataUrl: string) => Promise<boolean>;

export async function decodeBackgroundImage(
  dataUrl: string,
  maxBytes = MAX_BACKGROUND_BYTES,
): Promise<boolean> {
  if (!isBackgroundDataUrl(dataUrl, maxBytes)) return false;
  const image = new Image();
  try {
    image.src = dataUrl;
    await image.decode();
    return (
      image.naturalWidth > 0 &&
      image.naturalHeight > 0 &&
      image.naturalWidth <= maxBackgroundDimension &&
      image.naturalHeight <= maxBackgroundDimension &&
      image.naturalWidth * image.naturalHeight <= maxBackgroundPixels
    );
  } catch {
    return false;
  } finally {
    image.removeAttribute("src");
  }
}

export async function readPreferences(
  storage?: PreferencesStorage,
  decode: BackgroundDecoder = decodeBackgroundImage,
): Promise<{
  preferences: WorkbenchPreferences;
  storageFailed: boolean;
  backgroundFailed: boolean;
}> {
  let preferences: WorkbenchPreferences;
  try {
    preferences = restorePreferences(
      (storage ?? window.localStorage).getItem(PREFERENCES_STORAGE_KEY),
    );
  } catch {
    return {
      preferences: initialPreferences(),
      storageFailed: true,
      backgroundFailed: false,
    };
  }
  const dataUrl = preferences.appearance.backgroundImage;
  if (dataUrl !== null) {
    let decoded = false;
    try {
      decoded = await decode(dataUrl);
    } catch {
      // A rejected decoder must not prevent the workbench from opening.
    }
    if (!decoded)
      return { preferences, storageFailed: false, backgroundFailed: true };
  }
  return { preferences, storageFailed: false, backgroundFailed: false };
}

export function savePreferences(
  preferences: WorkbenchPreferences,
  storage?: PreferencesStorage,
): boolean {
  try {
    if (!isWorkbenchPreferences(preferences)) return false;
    (storage ?? window.localStorage).setItem(
      PREFERENCES_STORAGE_KEY,
      JSON.stringify(preferences),
    );
    return true;
  } catch {
    return false;
  }
}

export function restoreDefaultBackground(
  appearance: AppearancePreferences,
): AppearancePreferences {
  return { ...appearance, backgroundImage: null, backgroundName: null };
}

export interface BackgroundSelection {
  backgroundImage: string;
  backgroundName: string;
}

export async function readBackgroundFile(
  file: Pick<File, "name" | "type" | "size" | "arrayBuffer">,
  decode: BackgroundDecoder = decodeBackgroundImage,
): Promise<BackgroundSelection> {
  if (!imageTypes.has(file.type))
    throw new Error("请选择 PNG、JPEG 或 WebP 图片。SVG 和其他格式暂不支持。");
  if (file.size <= 0 || file.size > MAX_BACKGROUND_BYTES)
    throw new Error("背景图片需大于 0 字节且不超过 2 MiB。");
  let bytes: Uint8Array;
  try {
    bytes = new Uint8Array(await file.arrayBuffer());
  } catch {
    throw new Error("无法读取这张图片，原背景已保留。请重新选择文件。");
  }
  if (bytes.length !== file.size || !hasImageSignature(bytes, file.type))
    throw new Error("图片内容与格式不符，原背景已保留。");
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 32_768) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 32_768));
  }
  const dataUrl = `data:${file.type};base64,${btoa(binary)}`;
  let decoded = false;
  try {
    decoded = await decode(dataUrl);
  } catch {
    // Decode failure is shown as a local validation error, never a blank image.
  }
  if (!decoded)
    throw new Error(
      "这张图片无法解码，或尺寸超过 8192 像素／2400 万像素。原背景已保留。",
    );
  const cleanedName = file.name.replace(/[\p{Cc}/\\]/gu, "").trim();
  let name = "";
  // Count UTF-16 units without splitting a surrogate pair at the length limit.
  for (const character of cleanedName) {
    if (name.length + character.length > 180) break;
    name += character;
  }
  name = name.trim();
  return {
    backgroundImage: dataUrl,
    backgroundName: name || "自定义背景",
  };
}
