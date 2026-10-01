import { useCallback, useEffect, useRef, useState } from "react";
import { historyCall } from "./history-runtime.ts";
import type { ViewingHistory } from "./history-runtime.ts";
import {
  subscribeLibraryVisits,
  subscribeSourceVisits,
} from "./work-visits.ts";
import { listenReaderEvent } from "./reader/window-runtime.ts";

export function useViewingHistory(native: boolean) {
  const [snapshot, setSnapshot] = useState<ViewingHistory | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const serial = useRef<Promise<unknown>>(Promise.resolve());
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const request = useCallback(
    (command: string, args?: Record<string, unknown>) => {
      if (!native) return Promise.resolve();
      const task = serial.current
        .catch(() => {})
        .then(async () => {
          if (!alive.current) return;
          setBusy(true);
          try {
            const value = await historyCall(command, args);
            if (alive.current) {
              setSnapshot(value);
              setError("");
            }
          } catch {
            if (alive.current)
              setError("浏览历史暂时无法更新，已有记录保留；可重新读取。");
          } finally {
            if (alive.current) setBusy(false);
          }
        });
      serial.current = task;
      return task;
    },
    [native],
  );
  useEffect(() => {
    if (!native) return;
    const stopSource = subscribeSourceVisits((visit) => {
      if (visit.work)
        void request("history_record", {
          identity: { kind: "source", ...visit.reference },
          title: visit.work.title,
        });
    });
    const stopLibrary = subscribeLibraryVisits((visit) => {
      void request("history_record", {
        identity: {
          kind: "library",
          rootId: visit.rootId,
          entryId: visit.entryId,
        },
        title: visit.title,
      });
    });
    let stopped = false;
    let unlisten: (() => void) | undefined;
    void listenReaderEvent<boolean>("mangamonitor-history-changed", (ok) => {
      if (stopped) return;
      if (ok) void request("history_read");
      else
        setError(
          "这次阅读未能写入浏览历史，阅读不受影响；可重新读取历史状态。",
        );
    })
      .then((stop) => {
        if (stopped) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      stopSource();
      stopLibrary();
      stopped = true;
      unlisten?.();
    };
  }, [native, request]);
  return {
    snapshot,
    error,
    busy,
    refresh: useCallback(() => request("history_read"), [request]),
    clear: () => request("history_clear"),
    setEnabled: (enabled: boolean) =>
      request("history_set_enabled", { enabled }),
  };
}
