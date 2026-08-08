import { CircleHelp } from "lucide-react";
import { DynamicIcon } from "lucide-react/dynamic";
import type { IconName } from "lucide-react/dynamic";

export function WorkspaceIcon({
  name,
  size = 18,
}: {
  name: IconName | undefined;
  size?: number;
}) {
  const iconName: IconName = name ?? "box";
  return (
    <DynamicIcon
      name={iconName}
      size={size}
      strokeWidth={1.8}
      aria-hidden="true"
      fallback={() => <CircleHelp size={size} aria-hidden="true" />}
    />
  );
}
