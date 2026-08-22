import { cn } from "@/lib/utils";

export function PRLink({
  data,
  className,
  children,
}: {
  data: { id: string | number; url: string };
  className?: string;
  children?: React.ReactNode;
}) {
  return (
    <a
      href={data.url}
      target="_blank"
      rel="noreferrer"
      className={cn("hover:underline", className)}
      onClick={(e) => e.stopPropagation()}
    >
      {children ? children : `!${data.id}`}
    </a>
  );
}
