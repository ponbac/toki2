import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";
import { ExternalLink } from "lucide-react";

export function WorkItemDescriptionHoverCard({
  id,
  url,
  descriptionRenderedHtml,
}: {
  id: string;
  url: string;
  descriptionRenderedHtml: string | null;
}) {
  const hasDescription = Boolean(descriptionRenderedHtml);

  return (
    <HoverCard openDelay={120} closeDelay={160}>
      <HoverCardTrigger asChild>
        <a
          href={url}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(event) => event.stopPropagation()}
          aria-label={`Show description for issue ${id}`}
          className="inline-flex items-center gap-0.5 text-xs text-muted-foreground hover:text-foreground"
        >
          #{id}
          <ExternalLink className="h-3 w-3 opacity-0 transition-opacity group-hover:opacity-100" />
        </a>
      </HoverCardTrigger>
      <HoverCardContent
        align="start"
        side="bottom"
        sideOffset={8}
        className="w-[50rem] max-w-[calc(100vw-2rem)] overflow-hidden rounded-xl border border-border/60 bg-popover p-0 shadow-xl"
      >
        <div className="border-b border-border/60 px-3 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
            Issue #{id}
          </p>
        </div>
        <div className="px-3 py-2.5">
          {hasDescription ? (
            <div className="max-h-[15rem] overflow-y-auto pr-1">
              <article
                className="prose prose-sm max-w-none break-words text-foreground dark:prose-invert prose-p:my-1 prose-ol:my-1 prose-ul:my-1 prose-li:my-0.5 prose-pre:whitespace-pre-wrap prose-a:text-primary prose-a:underline prose-blockquote:my-2 prose-blockquote:border-l prose-blockquote:border-border prose-blockquote:pl-3 prose-blockquote:text-foreground prose-img:my-2 prose-img:max-w-full prose-img:rounded-md prose-img:border prose-img:border-border"
                onClick={(event) => event.stopPropagation()}
                dangerouslySetInnerHTML={{ __html: descriptionRenderedHtml ?? "" }}
              />
            </div>
          ) : (
            <p className="text-xs italic text-muted-foreground">
              No description available.
            </p>
          )}
        </div>
      </HoverCardContent>
    </HoverCard>
  );
}
