import { useCallback, useEffect, useRef, useState } from "react";
import { emptyDownloadInventory } from "./download-types.ts";
import type {
  DownloadAdapter,
  DownloadInventorySnapshot,
  DownloadSnapshot,
} from "./download-types.ts";
import type { LibrarySnapshot } from "./library-types.ts";
import { DownloadError, downloadErrorMessage } from "./download-runtime.ts";

/** Receipt file checks run on relevant changes, never for each image-progress tick. */
export function useDownloadInventory(
  adapter: DownloadAdapter,
  enabled: boolean,
  library: LibrarySnapshot,
  downloads: DownloadSnapshot,
  view: string,
) {
  const [state, setState] = useState<{
    snapshot: DownloadInventorySnapshot;
    ready: boolean;
    busy: boolean;
    error: string | null;
  }>(() => ({
    snapshot: emptyDownloadInventory(),
    ready: false,
    busy: false,
    error: null,
  }));
  const epoch = useRef(0);
  const completed = JSON.stringify(
    downloads.tasks
      .filter((task) => task.phase === "downloaded")
      .map((task) => [
        task.id,
        task.revision,
        task.localFiles,
        task.libraryEntryId,
      ]),
  );
  const refresh = useCallback(async () => {
    if (!enabled) return;
    const request = ++epoch.current;
    setState((previous) => ({ ...previous, busy: true, error: null }));
    try {
      const snapshot = await adapter.inventory();
      if (request !== epoch.current) return;
      if (snapshot.rootId !== library.rootId)
        throw new DownloadError("DOWNLOAD_ROOT_CHANGED");
      setState({ snapshot, ready: true, busy: false, error: null });
    } catch (problem) {
      if (request === epoch.current)
        setState((previous) => ({
          ...previous,
          ready: false,
          busy: false,
          error: downloadErrorMessage(problem),
        }));
    }
  }, [adapter, enabled, library.rootId]);
  useEffect(() => {
    void refresh();
    return () => {
      epoch.current++;
    };
  }, [refresh, library.revision, completed, view]);
  useEffect(() => {
    if (!enabled) return;
    const focus = () => {
      if (document.visibilityState !== "hidden") void refresh();
    };
    window.addEventListener("focus", focus);
    return () => {
      window.removeEventListener("focus", focus);
    };
  }, [enabled, refresh]);
  return { ...state, refresh };
}
