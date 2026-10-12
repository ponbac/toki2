import visualStudioLogo from "@/assets/visual-studio.svg";
import vscodeLogo from "@/assets/vscode.svg";
import { AiProviderIcon } from "@/components/ai-provider-icon";
import { cn } from "@/lib/utils";
import type { CopilotSurface } from "../-lib/copilot-setup";

/**
 * The mark of a place Copilot runs. Decorative: its name always sits beside
 * it. Copilot CLI has no mark of its own, so it carries Copilot's. The two
 * editors' logos are from Devicon (MIT), in their own colours.
 */
export function CopilotSurfaceIcon({
  surface,
  className,
}: {
  surface: CopilotSurface;
  className?: string;
}) {
  if (surface === "cli") {
    return <AiProviderIcon provider="copilot" className={className} />;
  }
  return (
    <img
      aria-hidden
      alt=""
      src={surface === "vscode" ? vscodeLogo : visualStudioLogo}
      className={cn("size-3.5 shrink-0", className)}
    />
  );
}
