import {
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
} from "@/components/ui/hover-card";
import { API_URL } from "@/lib/api/api";
import { ExternalLink } from "lucide-react";
import Markdown, { type Components } from "react-markdown";
import { useMemo, useState } from "react";

const BLOCKED_HTML_TAGS =
  "script,style,iframe,object,embed,meta,link,svg,math,form,input,button,textarea,select";

function hasHtmlMarkup(value: string): boolean {
  return /<\/?[a-z][\s\S]*>/i.test(value);
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function buildWorkItemImageProxyUrl({
  organization,
  project,
  imageUrl,
}: {
  organization: string;
  project: string;
  imageUrl: string;
}): string {
  const url = new URL("work-items/image", `${API_URL}/`);
  url.searchParams.set("organization", organization);
  url.searchParams.set("project", project);
  url.searchParams.set("imageUrl", imageUrl);
  return url.toString();
}

function rewriteImageSrc({
  source,
  organization,
  project,
}: {
  source: string;
  organization: string;
  project: string;
}): string | null {
  let parsed: URL;
  try {
    parsed = new URL(source);
  } catch {
    return null;
  }

  if (parsed.protocol !== "https:") {
    return null;
  }

  if (
    parsed.hostname.toLowerCase() === "dev.azure.com" &&
    parsed.pathname.includes("/_apis/wit/attachments/")
  ) {
    return buildWorkItemImageProxyUrl({
      organization,
      project,
      imageUrl: parsed.toString(),
    });
  }

  return parsed.toString();
}

function sanitizeHtml({
  value,
  organization,
  project,
}: {
  value: string;
  organization: string;
  project: string;
}): string {
  if (typeof window === "undefined") {
    return value;
  }

  const doc = new DOMParser().parseFromString(value, "text/html");
  doc.querySelectorAll(BLOCKED_HTML_TAGS).forEach((element) => element.remove());

  for (const element of doc.body.querySelectorAll("*")) {
    const tagName = element.tagName.toLowerCase();

    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      const attributeValue = attribute.value;

      if (name.startsWith("on") || name === "style") {
        element.removeAttribute(attribute.name);
        continue;
      }

      if (name === "href" || name === "src") {
        const isUnsafeProtocol = /^\s*(javascript|data):/i.test(attributeValue);
        if (isUnsafeProtocol) {
          element.removeAttribute(attribute.name);
          continue;
        }
      }

      if (tagName === "img" && name === "src") {
        const rewrittenSrc = rewriteImageSrc({
          source: attributeValue,
          organization,
          project,
        });

        if (rewrittenSrc) {
          element.setAttribute("src", rewrittenSrc);
        } else {
          element.removeAttribute("src");
        }
      }
    }

    if (tagName === "a") {
      element.setAttribute("target", "_blank");
      element.setAttribute("rel", "noopener noreferrer");
    }

    if (tagName === "img") {
      element.setAttribute("loading", "lazy");
      element.setAttribute("decoding", "async");
    }
  }

  return doc.body.innerHTML;
}

export function WorkItemDescriptionHoverCard({
  id,
  url,
  description,
  organization,
  project,
}: {
  id: string;
  url: string;
  description: string | null;
  organization: string;
  project: string;
}) {
  const normalizedDescription = description?.trim() ?? "";
  const hasDescription = normalizedDescription.length > 0;
  const [open, setOpen] = useState(false);
  const prefersHtml = hasHtmlMarkup(normalizedDescription);

  const renderedHtml = useMemo(() => {
    if (!hasDescription) {
      return "";
    }

    if (prefersHtml) {
      return sanitizeHtml({
        value: normalizedDescription,
        organization,
        project,
      });
    }

    return escapeHtml(normalizedDescription).replace(/\n/g, "<br />");
  }, [hasDescription, normalizedDescription, organization, prefersHtml, project]);

  const markdownComponents: Components = {
    a: ({ node, ...props }) => {
      void node;
      const { onClick, ...anchorProps } = props;
      return (
        <a
          {...anchorProps}
          target={anchorProps.target ?? "_blank"}
          rel={anchorProps.rel ?? "noopener noreferrer"}
          onClick={(event) => {
            event.stopPropagation();
            onClick?.(event);
          }}
        />
      );
    },
  };

  return (
    <HoverCard
      open={open}
      onOpenChange={setOpen}
      openDelay={120}
      closeDelay={160}
    >
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
              {prefersHtml ? (
                <article
                  className="prose prose-sm max-w-none break-words text-foreground dark:prose-invert prose-p:my-1 prose-ol:my-1 prose-ul:my-1 prose-li:my-0.5 prose-pre:whitespace-pre-wrap prose-a:text-primary prose-a:underline prose-blockquote:my-2 prose-blockquote:border-l prose-blockquote:border-border prose-blockquote:pl-3 prose-blockquote:text-foreground prose-img:my-2 prose-img:max-w-full prose-img:rounded-md prose-img:border prose-img:border-border"
                  onClick={(event) => event.stopPropagation()}
                  dangerouslySetInnerHTML={{ __html: renderedHtml }}
                />
              ) : (
                <article
                  className="prose prose-sm max-w-none break-words text-foreground dark:prose-invert prose-p:my-1 prose-ol:my-1 prose-ul:my-1 prose-li:my-0.5 prose-pre:whitespace-pre-wrap prose-a:text-primary prose-a:underline"
                  onClick={(event) => event.stopPropagation()}
                >
                  <Markdown components={markdownComponents}>
                    {normalizedDescription}
                  </Markdown>
                </article>
              )}
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
