import { CircleHelp } from "lucide-react";
import { DynamicIcon } from "lucide-react/dynamic";
import type { IconName } from "lucide-react/dynamic";

export function WorkspaceIcon({
  name,
  size = 18,
  className,
}: {
  name: IconName | undefined;
  size?: number;
  className?: string;
}) {
  const iconName: IconName = name ?? "box";
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
