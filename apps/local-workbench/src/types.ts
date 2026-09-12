export type WorkStatus = "owned" | "ready" | "review";
export interface Work {
  id: string;
  title: string;
  subtitle: string;
  author: string;
  source: "JM" | "Pica";
  cover: string;
  accent: string;
  tags: string[];
  description: string;
  chapters: number;
  pages: number;
  sizeMB: number;
  status: WorkStatus;
  finished: boolean;
  updated: string;
}

export type TaskStage =
  | "queued"
  | "downloading"
  | "verifying"
  | "packing"
  | "importing"
  | "sync_pending"
  | "completed"
  | "error";
export interface DemoTask {
  id: string;
  workId: string;
  stage: TaskStage;
  progress: number;
  paused: boolean;
  error: string | null;
}

export interface DemoState {
  version: 1;
  tasks: DemoTask[];
  online: boolean;
  paused: boolean;
  closed: boolean;
}
