import { createContext, useContext, useLayoutEffect } from "react";

export type CoverRetryAction = (() => void) | null;
export const CoverRetryContext = createContext<{
  register(action: CoverRetryAction): () => void;
  selectionMode: boolean;
} | null>(null);

/** The cover owns its loading state; the shared card owns pointer/keyboard gestures. */
export function useCoverRetry(action: CoverRetryAction) {
  const context = useContext(CoverRetryContext);
  const register = context?.register;
  useLayoutEffect(() => register?.(action), [register, action]);
  return {
    managed: context !== null,
    selectionMode: context?.selectionMode ?? false,
  };
}
