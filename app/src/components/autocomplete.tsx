import { cn } from "@/lib/utils";
import { Command as CommandPrimitive } from "cmdk";
import { Check } from "lucide-react";
import { useMemo, useState, useRef, useEffect } from "react";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandItem,
  CommandList,
} from "./ui/command";
import { Input } from "./ui/input";
import { Popover, PopoverAnchor, PopoverContent } from "./ui/popover";
import { Skeleton } from "./ui/skeleton";
import { ScrollArea } from "./ui/scroll-area";

type Props<T extends string> = {
  selectedValue: T;
  onSelectedValueChange: (value: T) => void;
  searchValue: string;
  onSearchValueChange: (value: string) => void;
  items: { value: T; label: string }[];
  isLoading?: boolean;
  emptyMessage?: string;
  placeholder?: string;
};

export function AutoComplete<T extends string>({
  selectedValue,
  onSelectedValueChange,
  searchValue,
  onSearchValueChange,
  items,
  isLoading,
  emptyMessage = "No items.",
  placeholder = "Search...",
}: Props<T>) {
  const [open, setOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const [highlightedIndex, setHighlightedIndex] = useState(-1);

  const labels = useMemo(
    () =>
      items.reduce(
        (acc, item) => {
          acc[item.value] = item.label;
          return acc;
        },
        {} as Record<string, string>,
      ),
    [items],
  );

  const filteredItems = useMemo(() => {
    const selectedItem = items.find((item) => item.value === selectedValue);
    if (selectedItem && searchValue === selectedItem.label) {
      return items;
    }

    return items.filter((item) =>
      item.label.toLowerCase().includes(searchValue.toLowerCase()),
    );
  }, [items, searchValue, selectedValue]);

  const reset = () => {
    onSelectedValueChange("" as T);
    onSearchValueChange("");
  };

  const onInputBlur = (e: React.FocusEvent<HTMLInputElement>) => {
    if (
      !e.relatedTarget?.hasAttribute("cmdk-item") &&
      labels[selectedValue] !== searchValue
    ) {
      reset();
    }
    setOpen(false);
  };

  const onSelectItem = (inputValue: string) => {
    if (inputValue === selectedValue) {
      reset();
    } else {
      onSelectedValueChange(inputValue as T);
      onSearchValueChange(labels[inputValue] ?? "");
    }
    setOpen(false);
    inputRef.current?.focus();
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!open) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setOpen(true);
      }
      return; // Allow default Tab behavior when closed
    }

    if (e.key === "ArrowDown" || (e.key === "Tab" && !e.shiftKey)) {
      e.preventDefault();
      setHighlightedIndex((prev) =>
        prev < filteredItems.length - 1 ? prev + 1 : 0,
      );
    } else if (e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey)) {
      e.preventDefault();
      setHighlightedIndex((prev) =>
        prev > 0 ? prev - 1 : filteredItems.length - 1,
      );
    } else if (e.key === "Enter" && highlightedIndex !== -1) {
      e.preventDefault();
      onSelectItem(filteredItems[highlightedIndex].value);
    } else if (e.key === "Escape") {
      setOpen(false);
    }
  };

  useEffect(() => {
    setHighlightedIndex(-1);
  }, [searchValue]);

  return (
    <div className="flex items-center">
      <Popover open={open} onOpenChange={setOpen}>
        <Command shouldFilter={false}>
          <PopoverAnchor asChild>
            <CommandPrimitive.Input
              ref={inputRef}
              asChild
              value={searchValue}
              onValueChange={onSearchValueChange}
              onKeyDown={handleKeyDown}
              onMouseDown={() => setOpen((open) => !!searchValue || !open)}
              onFocus={() => setOpen(true)}
              onBlur={onInputBlur}
            >
              <Input placeholder={placeholder} />
            </CommandPrimitive.Input>
          </PopoverAnchor>
          {!open && <CommandList aria-hidden="true" className="hidden" />}
          <PopoverContent
            asChild
            onOpenAutoFocus={(e) => e.preventDefault()}
            onInteractOutside={(e) => {
              if (
                e.target instanceof Element &&
                e.target.hasAttribute("cmdk-input")
              ) {
                e.preventDefault();
              }
            }}
            className="w-[--radix-popover-trigger-width] p-0"
          >
            <ScrollArea className="max-h-[200px] overflow-y-auto">
              <CommandList>
                {isLoading && (
                  <CommandPrimitive.Loading>
                    <div className="p-1">
                      <Skeleton className="h-6 w-full" />
                    </div>
                  </CommandPrimitive.Loading>
                )}
                {filteredItems.length > 0 && !isLoading ? (
                  <CommandGroup>
                    {filteredItems.map((option, index) => (
                      <CommandItem
                        key={option.value}
                        value={option.value}
                        onMouseDown={(e) => e.preventDefault()}
                        onSelect={onSelectItem}
                        className={cn(
                          highlightedIndex === index && "bg-accent",
                        )}
                      >
                        <Check
                          className={cn(
                            "mr-2 h-4 w-4",
                            selectedValue === option.value
                              ? "opacity-100"
                              : "opacity-0",
                          )}
                        />
                        {option.label}
                      </CommandItem>
                    ))}
                  </CommandGroup>
                ) : null}
                {!isLoading && filteredItems.length === 0 ? (
                  <CommandEmpty>{emptyMessage}</CommandEmpty>
                ) : null}
              </CommandList>
            </ScrollArea>
          </PopoverContent>
        </Command>
      </Popover>
    </div>
  );
}
