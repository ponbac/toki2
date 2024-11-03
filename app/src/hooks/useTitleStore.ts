import { create } from "zustand";

type SegmentId = "timer" | "notifications";

type TitleSegment = {
  id: SegmentId;
  title: string;
};

interface TitleState {
  baseTitle: string;
  segments: TitleSegment[];
  addSegment: (segment: TitleSegment) => void;
  removeSegment: (id: SegmentId) => void;
  updateSegment: (id: SegmentId, title: string) => void;
}

const SEGMENT_PRIORITIES: Record<SegmentId, number> = {
  notifications: 100,
  timer: 50,
};

export const useTitleStore = create<TitleState>((set) => ({
  baseTitle: "Toki2",
  segments: [],
  addSegment: (segment) => {
    set((state) => {
      const existingIndex = state.segments.findIndex(
        (s) => s.id === segment.id,
      );

      let segments;
      if (existingIndex >= 0) {
        segments = state.segments.map((s) =>
          s.id === segment.id ? segment : s,
        );
      } else {
        segments = [...state.segments, segment];
      }

      segments = segments.sort(
        (a, b) => SEGMENT_PRIORITIES[b.id] - SEGMENT_PRIORITIES[a.id],
      );

      document.title = formatTitle(state.baseTitle, segments);
      return { segments };
    });
  },
  removeSegment: (id) => {
    set((state) => {
      const segments = state.segments.filter((s) => s.id !== id);
      document.title = formatTitle(state.baseTitle, segments);
      return { segments };
    });
  },
  updateSegment: (id, title) => {
    set((state) => {
      const segments = state.segments.map((s) =>
        s.id === id ? { ...s, title } : s,
      );
      document.title = formatTitle(state.baseTitle, segments);
      return { segments };
    });
  },
}));

function formatTitle(baseTitle: string, segments: TitleSegment[]): string {
  if (segments.length === 0) return baseTitle;

  const segmentTitle = segments.reduce((title, segment, index) => {
    if (index === 0) return segment.title;

    const separator = segments[index - 1].id === "notifications" ? " " : " - ";
    return `${title}${separator}${segment.title}`;
  }, "");

  // If there's only one segment and it's notifications, don't add a dash
  const finalSeparator =
    segments.length === 1 && segments[0].id === "notifications" ? " " : " - ";

  return `${segmentTitle}${finalSeparator}${baseTitle}`;
}
