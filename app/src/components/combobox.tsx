import * as React from "react";
import { Check, ChevronsUpDown } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";

type ComboboxItem = {
  value: string;
  label: string;
};

type ComboboxProps = {
  items: ComboboxItem[];
  placeholder: string;
  searchPlaceholder?: string;
  onSelect: (value: string) => void;
  emptyMessage?: string;
  disabled?: boolean;
  value: string;
  onChange: (value: string) => void;
};

export const Combobox = React.forwardRef<HTMLButtonElement, ComboboxProps>(
  (props, ref) => {
    const [open, setOpen] = React.useState(false);
    const [search, setSearch] = React.useState("");

    const filteredItems = React.useMemo(() => {
      if (!search) return props.items;
      return props.items.filter((item) =>
        item.label.toLowerCase().includes(search.toLowerCase()),
      );
    }, [props.items, search]);

    return (
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            variant="outline"
            role="combobox"
            aria-expanded={open}
            className="w-full justify-between"
            disabled={props.disabled}
            ref={ref}
          >
            {props.value
              ? props.items.find((item) => item.value === props.value)?.label
              : props.placeholder}
            <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
          </Button>
        </PopoverTrigger>
        <PopoverContent className="w-[--radix-popover-trigger-width] p-0">
          <Command shouldFilter={false}>
            <CommandInput
              placeholder={
                props.searchPlaceholder ||
                `Search ${props.placeholder.toLowerCase()}...`
              }
              value={search}
              onValueChange={setSearch}
            />
            <CommandList className="h-[300px]">
              <CommandEmpty>
                {props.emptyMessage || "No items found"}
              </CommandEmpty>
              <CommandGroup>
                {filteredItems.map((item) => (
                  <CommandItem
                    key={item.value}
                    value={item.value}
                    onSelect={(currentValue) => {
                      props.onChange(
                        currentValue === props.value ? "" : currentValue,
                      );
                      setOpen(false);
                      setSearch("");
                      props.onSelect(currentValue);
                    }}
                  >
                    <Check
                      className={cn(
                        "mr-2 h-4 w-4",
                        props.value === item.value
                          ? "opacity-100"
                          : "opacity-0",
                      )}
                    />
                    {item.label}
                  </CommandItem>
                ))}
              </CommandGroup>
            </CommandList>
          </Command>
        </PopoverContent>
      </Popover>
    );
  },
);
