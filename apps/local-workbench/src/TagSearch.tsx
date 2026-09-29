import { createContext, useContext } from "react";
import type { Source } from "./source-types.ts";

const TagSearchContext = createContext<
  ((source: Source, tag: string, category?: boolean) => void) | null
>(null);
export const TagSearchProvider = TagSearchContext.Provider;
export const useTagSearch = () => useContext(TagSearchContext);
