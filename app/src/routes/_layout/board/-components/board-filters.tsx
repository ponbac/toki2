import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { useAtom } from "jotai";
import { useEffect, type SetStateAction } from "react";
import { Users, ChevronDown, Check, Columns3 } from "lucide-react";
import {
  categoryFilterAtom,
  type MemberFilter,
} from "../-lib/board-preferences";
import { BOARD_CATEGORY_OPTIONS } from "../-lib/category-meta";
import { cn } from "@/lib/utils";

const MEMBER_MODES: { value: MemberFilter["mode"]; label: string }[] = [
  { value: "mine", label: "My Items" },
  { value: "all", label: "All" },
  { value: "custom", label: "Custom" },
];

export function BoardFilters({
  memberFilter,
  setMemberFilter,
  members,
  columns,
  hiddenColumnIds,
  onToggleColumn,
  onShowAllColumns,
}: {
  memberFilter: MemberFilter;
  setMemberFilter: (update: SetStateAction<MemberFilter>) => void;
  members: { email: string; displayName: string }[];
  columns: { id: string; name: string; count: number }[];
  hiddenColumnIds: string[];
  onToggleColumn: (columnId: string) => void;
  onShowAllColumns: () => void;
}) {
  const [categoryFilter, setCategoryFilter] = useAtom(categoryFilterAtom);

  useEffect(() => {
    setCategoryFilter((prev) => (prev.includes("other") ? prev : [...prev, "other"]));
  }, [setCategoryFilter]);

  const toggleCategory = (category: string) => {
    setCategoryFilter((prev) =>
      prev.includes(category)
        ? prev.filter((c) => c !== category)
        : [...prev, category],
    );
  };

  const toggleMember = (email: string) => {
    const normalizedEmail = email.toLowerCase();
    setMemberFilter((prev) => ({
      ...prev,
      selectedEmails: prev.selectedEmails.some(
        (candidate) => candidate.toLowerCase() === normalizedEmail,
      )
        ? prev.selectedEmails.filter(
            (candidate) => candidate.toLowerCase() !== normalizedEmail,
          )
        : [...prev.selectedEmails, email],
    }));
  };

  const hiddenColumns = new Set(hiddenColumnIds);
  const hiddenColumnsCount = columns.filter((column) =>
    hiddenColumns.has(column.id),
  ).length;
  const visibleColumnsCount = columns.length - hiddenColumnsCount;

  return (
    <div className="flex flex-wrap items-center gap-3">
      {/* Member filter */}
      <div className="flex items-center gap-1">
        <Users className="mr-1 h-4 w-4 text-muted-foreground" />
        <div className="flex rounded-lg border border-border/50 p-0.5">
          {MEMBER_MODES.map((mode) => (
            <button
              key={mode.value}
              onClick={() =>
                setMemberFilter((prev) => ({ ...prev, mode: mode.value }))
              }
              className={cn(
                "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
                memberFilter.mode === mode.value
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {mode.label}
            </button>
          ))}
        </div>

        {memberFilter.mode === "custom" && (
          <Popover>
            <PopoverTrigger asChild>
              <Button variant="outline" size="sm" className="ml-1 h-7 gap-1 text-xs">
                {memberFilter.selectedEmails.length === 0
                  ? "Select members"
                  : `${memberFilter.selectedEmails.length} selected`}
                <ChevronDown className="h-3 w-3" />
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-64 p-2" align="start">
              <div className="max-h-60 overflow-y-auto">
                {members.map((member) => {
                  const selected = memberFilter.selectedEmails.some(
                    (candidate) =>
                      candidate.toLowerCase() === member.email.toLowerCase(),
                  );
                  return (
                    <button
                      key={member.email}
                      onClick={() => toggleMember(member.email)}
                      className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted"
                    >
                      <div
                        className={cn(
                          "flex h-4 w-4 shrink-0 items-center justify-center rounded-sm border",
                          selected
                            ? "border-primary bg-primary"
                            : "border-muted-foreground/30",
                        )}
                      >
                        {selected && (
                          <Check className="h-3 w-3 text-primary-foreground" />
                        )}
                      </div>
                      <span className="truncate">{member.displayName}</span>
                    </button>
                  );
                })}
                {members.length === 0 && (
                  <p className="px-2 py-3 text-center text-xs text-muted-foreground">
                    No assignees found
                  </p>
                )}
              </div>
            </PopoverContent>
          </Popover>
        )}
      </div>

      {/* Divider */}
      <div className="h-5 w-px bg-border/50" />

      {/* Column visibility filter */}
      <Popover>
        <PopoverTrigger asChild>
          <Button variant="outline" size="sm" className="h-7 gap-1.5 text-xs">
            <Columns3 className="h-3.5 w-3.5" />
            Columns
            <span className="text-muted-foreground">
              {visibleColumnsCount}/{columns.length}
            </span>
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-72 p-2" align="start">
          <div className="mb-2 flex items-center justify-between px-1">
            <span className="text-xs font-medium text-muted-foreground">
              Visible columns
            </span>
            <button
              onClick={onShowAllColumns}
              className="text-xs text-muted-foreground hover:text-foreground"
            >
              Show all
            </button>
          </div>
          <div className="max-h-64 overflow-y-auto">
            {columns.map((column) => {
              const visible = !hiddenColumns.has(column.id);
              return (
                <button
                  key={column.id}
                  onClick={() => onToggleColumn(column.id)}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted"
                >
                  <div
                    className={cn(
                      "flex h-4 w-4 shrink-0 items-center justify-center rounded-sm border",
                      visible
                        ? "border-primary bg-primary"
                        : "border-muted-foreground/30",
                    )}
                  >
                    {visible && (
                      <Check className="h-3 w-3 text-primary-foreground" />
                    )}
                  </div>
                  <span className="truncate">{column.name}</span>
                  <span className="ml-auto text-xs text-muted-foreground">
                    {column.count}
                  </span>
                </button>
              );
            })}
          </div>
        </PopoverContent>
      </Popover>

      {/* Divider */}
      <div className="h-5 w-px bg-border/50" />

      {/* Category filter chips */}
      <div className="flex flex-wrap items-center gap-1.5">
        {BOARD_CATEGORY_OPTIONS.map((cat) => {
          const active = categoryFilter.includes(cat.value);
          return (
            <button
              key={cat.value}
              onClick={() => toggleCategory(cat.value)}
              className={cn(
                "rounded-md border px-2 py-0.5 text-xs font-medium transition-colors",
                active
                  ? [cat.bg, cat.text, cat.border]
                  : "border-border/30 text-muted-foreground/50",
              )}
            >
              {cat.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
