import { iconNames } from "lucide-react/dynamic";
import type { IconName } from "lucide-react/dynamic";

const WORKSPACE_ICON_STORAGE_KEY = "kelpie.workspace-icons.v1";

const catalog = new Set<string>(iconNames);

export type WorkspaceIconMap = Partial<Record<string, IconName>>;

function isIconName(value: unknown): value is IconName {
  return typeof value === "string" && catalog.has(value);
}

function readStorage(): unknown {
  try {
    return typeof localStorage === "undefined" ? null : JSON.parse(localStorage.getItem(WORKSPACE_ICON_STORAGE_KEY) ?? "null");
  } catch {
    return null;
  }
}

function writeStorage(icons: WorkspaceIconMap): void {
  try {
    localStorage.setItem(WORKSPACE_ICON_STORAGE_KEY, JSON.stringify(icons));
  } catch {
    // Local persistence is best effort when storage is unavailable or full.
  }
}

export function readWorkspaceIcons(): WorkspaceIconMap {
  const stored = readStorage();
  if (stored === null || typeof stored !== "object" || Array.isArray(stored)) return {};

  const icons: WorkspaceIconMap = {};
  for (const [workspaceId, value] of Object.entries(stored)) {
    if (workspaceId && isIconName(value)) icons[workspaceId] = value;
  }
  return icons;
}

function randomDefaultIcon(): IconName {
  return iconNames[Math.floor(Math.random() * iconNames.length)];
}

export function ensureWorkspaceIcons(workspaceIds: readonly string[]): WorkspaceIconMap {
  const icons = readWorkspaceIcons();
  let changed = false;
  for (const workspaceId of workspaceIds) {
    if (!workspaceId || icons[workspaceId]) continue;
    icons[workspaceId] = randomDefaultIcon();
    changed = true;
  }
  if (changed) writeStorage(icons);
  return icons;
}

export function saveWorkspaceIcon(workspaceId: string, iconName: unknown): WorkspaceIconMap {
  const icons = readWorkspaceIcons();
  if (workspaceId && isIconName(iconName)) {
    icons[workspaceId] = iconName;
    writeStorage(icons);
  }
  return icons;
}

