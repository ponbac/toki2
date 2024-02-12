import { cn } from "@/lib/utils";

type LinkData = {
  organization: string;
  project: string;
  repoName: string;
  id: number;
};

type PRLinkProps<T extends LinkData> = {
  data: T;
  className?: string;
};

// https://dev.azure.com/ex-change-part/Quote%20Manager/_git/hexagon/pullrequest/1542
export function PRLink<T extends LinkData>({
  data,
  className,
}: PRLinkProps<T>) {
  return (
    <a
      href={`https://dev.azure.com/${data.organization}/${data.project}/_git/${data.repoName}/pullrequest/${data.id}`}
      target="_blank"
      rel="noreferrer"
      className={cn("hover:underline", className)}
      onClick={(e) => e.stopPropagation()}
    >
      !{data.id}
    </a>
  );
}
