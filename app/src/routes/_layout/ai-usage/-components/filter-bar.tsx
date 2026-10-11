import * as React from "react";
import { format } from "date-fns";
import type { DateRange } from "react-day-picker";
import { CalendarIcon, Check, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { AI_PROVIDER_LABELS } from "@/lib/ai-providers";
import { AI_PROVIDERS, type AiProvider } from "@/lib/api/contracts/ai-usage";
import type {
  AiUsageReport,
  AiUsageReportParams,
} from "@/lib/api/queries/ai-usage-report";
import { cn } from "@/lib/utils";
import {
  EMPTY_DRAFT,
  MAX_RANGE_DAYS,
  pickDay,
  rangeDays,
  rangePresets,
  sameRange,
  type LocalDateRange,
  type RangeDraft,
} from "../-lib/date-range";
import { formatShortDate, modelLabel } from "../-lib/format";

const ALL = "all";

/** A change to the filters; `undefined` clears one. */
export type FilterPatch = Partial<{
  [Key in keyof AiUsageReportParams]: AiUsageReportParams[Key] | undefined;
}>;

/**
 * The one row of filters above the page: the local date range first, then
 * tool, model, project and machine. Every figure below follows them.
 */
export function FilterBar({
  params,
  report,
  machineLabels,
  onChange,
}: {
  params: AiUsageReportParams;
  report: AiUsageReport;
  machineLabels: ReadonlyMap<string, string>;
  onChange: (patch: FilterPatch) => void;
}) {
  const { options } = report;
  const filtered =
    params.provider !== undefined ||
    params.model !== undefined ||
    params.projectId !== undefined ||
    params.unassigned !== undefined ||
    params.machineId !== undefined;

  // A selected value stays listed even when the range has no usage of it.
  const providers = withSelected(options.providers, params.provider);
  const models = withSelected(options.models, params.model);
  const machineIds = withSelected(options.machineIds, params.machineId);
  const projects =
    params.projectId !== undefined &&
    !options.projects.some((project) => project.projectId === params.projectId)
      ? [
          ...options.projects,
          { projectId: params.projectId, projectName: "Selected project" },
        ]
      : options.projects;

  return (
    <div className="flex flex-wrap items-center gap-2">
      <DateRangeControl
        range={report}
        today={report.today}
        onChange={(range) => onChange(range)}
      />
      <FilterSelect
        label="Tool"
        value={params.provider ?? ALL}
        onChange={(value) =>
          onChange({ provider: AI_PROVIDERS.find((known) => known === value) })
        }
        items={[
          { value: ALL, label: "All tools" },
          ...providers.map((provider: AiProvider) => ({
            value: provider,
            label: AI_PROVIDER_LABELS[provider],
          })),
        ]}
      />
      <FilterSelect
        label="Model"
        value={params.model === undefined ? ALL : `model:${params.model}`}
        onChange={(value) =>
          onChange({
            model: value.startsWith("model:")
              ? value.slice("model:".length)
              : undefined,
          })
        }
        items={[
          { value: ALL, label: "All models" },
          ...models.map((model) => ({
            value: `model:${model}`,
            label: modelLabel(model),
          })),
        ]}
      />
      <FilterSelect
        label="Project"
        value={
          params.unassigned
            ? "unassigned"
            : params.projectId !== undefined
              ? `project:${params.projectId}`
              : ALL
        }
        onChange={(value) =>
          onChange({
            projectId: value.startsWith("project:")
              ? value.slice("project:".length)
              : undefined,
            unassigned: value === "unassigned" ? true : undefined,
          })
        }
        items={[
          { value: ALL, label: "All projects" },
          ...projects.map((project) => ({
            value: `project:${project.projectId}`,
            label: project.projectName,
          })),
          ...(options.unassigned || params.unassigned
            ? [{ value: "unassigned", label: "Unassigned" }]
            : []),
        ]}
      />
      <FilterSelect
        label="Machine"
        value={params.machineId ?? ALL}
        onChange={(value) =>
          onChange({ machineId: value === ALL ? undefined : value })
        }
        items={[
          { value: ALL, label: "All machines" },
          ...machineIds.map((machineId) => ({
            value: machineId,
            label: machineLabels.get(machineId) ?? "Unknown machine",
          })),
        ]}
      />
      {filtered && (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() =>
            onChange({
              provider: undefined,
              model: undefined,
              projectId: undefined,
              unassigned: undefined,
              machineId: undefined,
            })
          }
        >
          <X className="size-4" />
          Clear filters
        </Button>
      )}
    </div>
  );
}

function withSelected<Value>(
  values: readonly Value[],
  selected: Value | undefined,
): readonly Value[] {
  return selected === undefined || values.includes(selected)
    ? values
    : [...values, selected];
}

function FilterSelect({
  label,
  value,
  items,
  onChange,
}: {
  label: string;
  value: string;
  items: readonly Readonly<{ value: string; label: string }>[];
  onChange: (value: string) => void;
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger
        aria-label={label}
        className={cn(
          "h-9 w-auto min-w-32 max-w-56 gap-2",
          value !== ALL && "border-primary/60",
        )}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {items.map((item) => (
          <SelectItem key={item.value} value={item.value}>
            {item.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/** A local calendar date as a date-picker day, at local midnight. */
function toPickerDate(date: string): Date {
  const [year = 1970, month = 1, day = 1] = date.split("-").map(Number);
  return new Date(year, month - 1, day);
}

function DateRangeControl({
  range,
  today,
  onChange,
}: {
  range: LocalDateRange;
  today: string;
  onChange: (range: LocalDateRange) => void;
}) {
  const [open, setOpen] = React.useState(false);
  // A new pick always starts clean; the current range is only a marker.
  const [draft, setDraft] = React.useState<RangeDraft>(EMPTY_DRAFT);
  const presets = rangePresets(today);
  const picked = draft.kind === "complete" ? draft.range : null;
  const tooLong = picked !== null && rangeDays(picked) > MAX_RANGE_DAYS;
  const apply = (next: LocalDateRange) => {
    onChange(next);
    setOpen(false);
  };
  const selected: DateRange | undefined =
    draft.kind === "empty"
      ? undefined
      : draft.kind === "start"
        ? { from: toPickerDate(draft.from), to: undefined }
        : {
            from: toPickerDate(draft.range.from),
            to: toPickerDate(draft.range.to),
          };

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        setDraft(EMPTY_DRAFT);
      }}
    >
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          className="h-9 justify-start gap-2 font-normal tabular-nums"
          aria-label="Date range"
        >
          <CalendarIcon className="size-4" />
          {formatShortDate(range.from)} – {formatShortDate(range.to)}{" "}
          {range.to.slice(0, 4)}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0" align="start">
        <div className="flex flex-col sm:flex-row">
          <ul className="border-b border-border/60 p-1 sm:w-40 sm:border-b-0 sm:border-r">
            {presets.map((preset) => {
              const current = sameRange(preset.range, range);
              return (
                <li key={preset.label}>
                  <button
                    type="button"
                    onClick={() => apply(preset.range)}
                    className="flex w-full items-center justify-between rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted"
                  >
                    {preset.label}
                    {current && (
                      <Check
                        className="size-4 stroke-[3]"
                        aria-label="Current range"
                      />
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
          <div className="p-1">
            <Calendar
              mode="range"
              weekStartsOn={1}
              numberOfMonths={1}
              defaultMonth={toPickerDate(range.to)}
              selected={selected}
              onSelect={(_range, day) =>
                setDraft((current) =>
                  pickDay(current, format(day, "yyyy-MM-dd")),
                )
              }
              modifiers={{
                current: {
                  from: toPickerDate(range.from),
                  to: toPickerDate(range.to),
                },
              }}
              modifiersClassNames={{
                current:
                  "underline decoration-primary decoration-2 underline-offset-4",
              }}
            />
            <div className="flex items-center justify-between gap-2 border-t border-border/60 px-2 py-2">
              <p
                className={cn(
                  "max-w-48 text-xs",
                  tooLong ? "text-destructive" : "text-muted-foreground",
                )}
                aria-live="polite"
              >
                {draft.kind === "empty"
                  ? "Pick the first day. The current range is underlined."
                  : draft.kind === "start"
                    ? `From ${formatShortDate(draft.from)}; now pick the last day.`
                    : tooLong
                      ? `At most ${MAX_RANGE_DAYS} days`
                      : `${formatShortDate(draft.range.from)} – ${formatShortDate(draft.range.to)}, both days included`}
              </p>
              <Button
                type="button"
                size="sm"
                disabled={picked === null || tooLong}
                onClick={() => picked && apply(picked)}
              >
                Apply
              </Button>
            </div>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
