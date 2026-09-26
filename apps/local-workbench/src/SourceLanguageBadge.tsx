import { createContext, useContext } from "react";
import type { ReactNode } from "react";
import { classifySourceLanguage, hasLanguageTags } from "./source-language.ts";
import { sourceWorkKey } from "./source-types.ts";
import type { SourceScope, SourceWork } from "./source-types.ts";
import "./source-language-badge.css";

type SourceLanguageCache = Record<
  string,
  { scope: SourceScope; work: SourceWork }
>;
const SourceLanguageContext = createContext<SourceLanguageCache>({});

export function SourceLanguageProvider({
  cache,
  children,
}: {
  cache: SourceLanguageCache;
  children: ReactNode;
}) {
  return (
    <SourceLanguageContext.Provider value={cache}>
      {children}
    </SourceLanguageContext.Provider>
  );
}

interface SourceLanguageBadgeProps {
  tags: readonly string[];
  work?: SourceWork;
  scope?: SourceScope | null;
  localVersion?: boolean;
  inline?: boolean;
}

export function SourceLanguageBadge({
  tags,
  work,
  scope,
  localVersion = false,
  inline = false,
}: SourceLanguageBadgeProps) {
  const cache = useContext(SourceLanguageContext);
  let languageTags = localVersion ? tags : (work?.tags ?? tags);
  if (
    !localVersion &&
    work &&
    scope &&
    work.source === scope.source &&
    !hasLanguageTags(languageTags)
  ) {
    const entry = cache[sourceWorkKey(work)];
    if (
      entry?.scope.source === scope.source &&
      entry.scope.sessionId === scope.sessionId &&
      entry.work.source === work.source &&
      entry.work.workId === work.workId &&
      hasLanguageTags(entry.work.tags)
    ) {
      languageTags = entry.work.tags;
    }
  }
  const language = classifySourceLanguage(languageTags);
  const explanation = localVersion
    ? `本地版本语言：${language.label}。仅依据本地版本保存的标签。${language.explanation}`
    : `语言：${language.label}。${language.explanation}`;

  return (
    <span
      className={
        "source-language-badge" + (inline ? " source-language-inline" : "")
      }
      data-testid="source-language-badge"
      data-language-kind={language.kind}
      data-language-context={localVersion ? "local" : "source"}
      title={explanation}
      aria-label={explanation}
    >
      {language.label}
    </span>
  );
}
