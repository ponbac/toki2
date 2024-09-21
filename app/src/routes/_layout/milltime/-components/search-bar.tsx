"use client";

import { SearchIcon } from "lucide-react";
import { Input } from "@/components/ui/input";
import { useRef } from "react";

export function SearchBar(props: {
  search: string;
  setSearch: (search: string) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);

  return (
    <div className="relative">
      <SearchIcon
        className="absolute left-2 top-1/2 size-4 -translate-y-1/2 transform text-gray-400 hover:cursor-pointer"
        onClick={() => ref.current?.focus()}
      />
      <Input
        ref={ref}
        type="text"
        placeholder="Search entries..."
        className="w-64 pl-8"
        value={props.search}
        onChange={(e) => props.setSearch(e.target.value)}
      />
    </div>
  );
}
