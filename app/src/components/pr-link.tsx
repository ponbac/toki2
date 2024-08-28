import { cn, LinkData, pullRequestUrl } from "@/lib/utils";

type PRLinkProps<T extends LinkData> = {
  data: T;
  className?: string;
  children?: React.ReactNode;
};

// https://dev.azure.com/ex-change-part/Quote%20Manager/_git/hexagon/pullrequest/1542
export function PRLink<T extends LinkData>({
  data,
  className,
  children,
}: PRLinkProps<T>) {
  return (
    <a
      href={pullRequestUrl(data)}
      target="_blank"
      rel="noreferrer"
      className={cn("hover:underline", className)}
      onClick={(e) => e.stopPropagation()}
    >
      {children ? children : `!${data.id}`}
    </a>
  );
}
