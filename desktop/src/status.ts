import type { Status } from "./types";

const STATUS_LABEL: Record<Status, string> = {
  working: "Working",
  blocked: "Needs input",
  idle: "Idle",
  done: "Done",
  unknown: "Unknown",
};

export function statusLabel(status: string): string {
  return STATUS_LABEL[status as Status] ?? STATUS_LABEL.unknown;
}
