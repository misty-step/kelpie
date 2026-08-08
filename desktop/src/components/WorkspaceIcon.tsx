import { CircleHelp } from "lucide-react";
import { DynamicIcon } from "lucide-react/dynamic";
import type { IconName } from "lucide-react/dynamic";
import { isIconName } from "../workspaceIcons";

export function WorkspaceIcon({
  name,
  size = 18,
  className,
}: {
  name: unknown;
  size?: number;
  className?: string;
}) {
  const iconName: IconName = isIconName(name) ? name : "box";
  return (
    <DynamicIcon
      name={iconName}
      size={size}
      className={className}
      strokeWidth={1.8}
      aria-hidden="true"
      fallback={() => <CircleHelp size={size} className={className} aria-hidden="true" />}
    />
  );
}
