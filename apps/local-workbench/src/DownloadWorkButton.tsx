import { createContext, useContext } from "react";
import type { SourceWork } from "./source-types.ts";

export type DownloadFeedback = { label: string; disabled: boolean };
export const DownloadFeedbackContext = createContext<
  (work: Pick<SourceWork, "source" | "workId">) => DownloadFeedback
>(() => ({ label: "下载到漫画库", disabled: false }));

export function DownloadWorkButton({
  work,
  owned = false,
  ready = true,
  onClick,
  className = "text-button",
  testId,
}: {
  work: SourceWork;
  owned?: boolean;
  ready?: boolean;
  onClick(): void;
  className?: string;
  testId?: string;
}) {
  const feedback = useContext(DownloadFeedbackContext)(work);
  return (
    <button
      className={className}
      data-testid={testId}
      disabled={owned || !ready || feedback.disabled}
      onClick={onClick}
    >
      {owned ? "已入库" : feedback.label}
    </button>
  );
}
