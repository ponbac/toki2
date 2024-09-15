"use client";

import { useState } from "react";
import { AddEntryButton } from "./add-entry-button";
import { DataVisualization } from "./data-visualization";
import { DateRangeSelector } from "./date-range-selector";
import { ExportButton } from "./export-button";
import { SearchBar } from "./search-bar";
import { Summary } from "./summary";
import { TimeEntriesList } from "./time-entries-list";

export function TimeTrackerComponent() {
  const [dateRange, setDateRange] = useState({
    start: new Date(),
    end: new Date(),
  });

  return (
    <div className={`min-h-screen`}>
      <div className="">
        <div className="container mx-auto px-4 py-8">
          <header className="mb-8 flex items-center justify-between">
            <h1 className="text-3xl font-bold">Time Tracker</h1>
          </header>
          <div className="grid grid-cols-1 gap-8 lg:grid-cols-3">
            <div className="lg:col-span-2">
              <DateRangeSelector onRangeChange={setDateRange} />
              <div className="mt-4 flex items-center justify-between">
                <SearchBar />
                <ExportButton />
              </div>
              <TimeEntriesList dateRange={dateRange} />
              <AddEntryButton />
            </div>
            <div>
              <Summary dateRange={dateRange} />
              <DataVisualization dateRange={dateRange} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
