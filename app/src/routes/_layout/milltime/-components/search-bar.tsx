import { SearchIcon } from "lucide-react";
import { Input } from "@/components/ui/input";
import { useRef } from "react";

export function SearchBar(props: {
  search: string;
  setSearch: (search: string) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);

  return (
    <div className="relative flex items-center">
      <SearchIcon
        className="absolute left-2.5 size-3.5 text-muted-foreground/60 hover:cursor-pointer"
        onClick={() => ref.current?.focus()}
      />
      <Input
        ref={ref}
        type="text"
        placeholder="Search entries..."
        className="h-8 w-48 border-0 bg-transparent pl-8 text-xs shadow-none placeholder:text-muted-foreground/50 focus-visible:ring-0 focus-visible:ring-offset-0"
        value={props.search}
        onChange={(e) => props.setSearch(e.target.value)}
      />
    </div>
  );
}
