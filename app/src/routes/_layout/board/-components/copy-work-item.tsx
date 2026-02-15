import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { queries } from "@/lib/api/queries/queries";
import { ClipboardCopy, Check, Loader2 } from "lucide-react";
import { useState, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

export function CopyWorkItem({
  workItemId,
  organization,
  project,
}: {
  workItemId: string;
  organization: string;
  project: string;
}) {
  const [copied, setCopied] = useState(false);
  const [loading, setLoading] = useState(false);
  const queryClient = useQueryClient();

  const handleCopy = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      setLoading(true);
      try {
        const data = await queryClient.fetchQuery(
          queries.formatForLlm({ organization, project, workItemId }),
        );
        await navigator.clipboard.writeText(data.markdown);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
        if (data.hasImages) {
          toast.warning("Images were detected but not included in the copy.");
        }
      } catch {
        toast.error("Failed to format work item.");
      } finally {
        setLoading(false);
      }
    },
    [queryClient, organization, project, workItemId],
  );

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0"
          onClick={handleCopy}
          disabled={loading}
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : copied ? (
            <Check className="h-3.5 w-3.5 text-green-500" />
          ) : (
            <ClipboardCopy className="h-3.5 w-3.5" />
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {loading ? "Loading..." : copied ? "Copied!" : "Copy for LLM"}
      </TooltipContent>
    </Tooltip>
  );
}
