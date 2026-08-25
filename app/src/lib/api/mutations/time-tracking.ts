import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { DefaultMutationOptions, MutationFnAsync } from "./mutations";
import {
  AbsenceEntry,
  ManagedAbsenceType,
  SaveTimerResponse,
  TimeEntry,
  TimerResponse,
  absenceTypeLabels,
  timeTrackingQueries,
} from "../queries/time-tracking";
import {
  useTimeTrackingActions,
  useTimeTrackingStore,
} from "@/hooks/useTimeTrackingStore";
import {
  applyAbsenceTimeInfoDelta,
  applyTimeInfoDelta,
  buildTimeEntryFromCreatePayload,
  buildTimeEntryFromSave,
  cancelAbsenceQueries,
  cancelTimeTrackingRangeQueries,
  findCachedAbsence,
  findCachedEntry,
  markTimeTrackingListsStale,
  removeAbsenceFromCachedRanges,
  removeEntryFromCachedRanges,
  replaceAbsencesInCachedRanges,
  replaceEntryInCachedRanges,
  setTimerCache,
  upsertAbsenceInCachedRanges,
  upsertEntryInCachedRanges,
} from "../time-tracking-cache";

/** Mutation hooks for provider-neutral time-tracking writes. */
export const timeTrackingMutations = {
  useStartTimer,
  useStopTimer,
  useSaveTimer,
  useEditTimer,
  useUpdateTimeEntry,
  useDeleteTimeEntry,
  useCreateTimeEntry,
  useCreateAbsences,
  useDeleteAbsence,
  useImportKleerUsers,
  useLinkKleerUsersByEmail,
  useUpsertKleerUserLink,
  useDeactivateKleerUserLink,
};

function useStartTimer(
  options?: DefaultMutationOptions<StartTimerPayload, TimerResponse>,
) {
  const queryClient = useQueryClient();
  const { setTimer } = useTimeTrackingActions();

  return useMutation({
    mutationKey: ["time-tracking", "startTimer"],
    mutationFn: (body: StartTimerPayload) =>
      api
        .post("time-tracking/timer", {
          json: {
            projectId: body.projectId,
            activityId: body.activityId,
            note: body.userNote,
          },
        })
        .json<TimerResponse>(),
    ...options,
    onMutate: (vars) => {
      queryClient.resetQueries({
        queryKey: timeTrackingQueries.getTimer().queryKey,
      });
      options?.onMutate?.(vars);
    },
    onSuccess: (data, v, c) => {
      setTimerCache(queryClient, data);
      setTimer({
        visible: true,
        state: "running",
        timeSeconds: 0,
      });

      options?.onSuccess?.(data, v, c);
    },
  });
}

function useStopTimer(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();
  const { setTimer } = useTimeTrackingActions();

  return useMutation({
    mutationKey: ["time-tracking", "stopTimer"],
    mutationFn: () => api.delete("time-tracking/timer"),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timerBaseKey,
      });
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });

      options?.onSuccess?.(data, v, c);
    },
  });
}

function useSaveTimer(
  options?: DefaultMutationOptions<SaveTimerPayload, SaveTimerResponse>,
) {
  const queryClient = useQueryClient();
  const { setTimer } = useTimeTrackingActions();
  const timerQuery = timeTrackingQueries.getTimer();

  return useMutation({
    mutationKey: ["time-tracking", "saveTimer"],
    mutationFn: (body: SaveTimerPayload) =>
      api
        .post("time-tracking/timer/save", {
          headers: { "Idempotency-Key": crypto.randomUUID() },
          json: { note: body.userNote },
        })
        .json<SaveTimerResponse>(),
    ...options,
    onMutate: async (vars) => {
      await queryClient.cancelQueries({ queryKey: timerQuery.queryKey });
      await cancelTimeTrackingRangeQueries(queryClient);

      const previousTimer = queryClient.getQueryData(timerQuery.queryKey);
      const previousTimerState = useTimeTrackingStore.getState().timer;
      const optimisticId = `optimistic:timer-save:${crypto.randomUUID()}`;
      const optimisticEntry = previousTimer?.timer
        ? buildTimeEntryFromSave(
            previousTimer.timer,
            vars.userNote,
            optimisticId,
          )
        : null;

      if (optimisticEntry) {
        upsertEntryInCachedRanges(queryClient, optimisticEntry);
        applyTimeInfoDelta(
          queryClient,
          optimisticEntry.date,
          optimisticEntry.hours,
        );
      }

      setTimerCache(queryClient, null);
      setTimer({ visible: false, state: "stopped", timeSeconds: null });

      const optionsContext = await options?.onMutate?.(vars);
      return {
        previousTimer,
        previousTimerState,
        optimisticEntry,
        optimisticId,
        optionsContext,
      };
    },
    onSuccess: (data, v, c) => {
      if (c?.optimisticEntry) {
        replaceEntryInCachedRanges(queryClient, c.optimisticId, data.entry);
      } else {
        upsertEntryInCachedRanges(queryClient, data.entry);
      }
      setTimerCache(queryClient, null);
      setTimer({ visible: false, state: "stopped", timeSeconds: null });
      markTimeTrackingListsStale(queryClient);
      options?.onSuccess?.(data, v, c?.optionsContext);
    },
    onError: (error, v, c) => {
      if (c?.optimisticEntry) {
        removeEntryFromCachedRanges(queryClient, c.optimisticId);
        applyTimeInfoDelta(
          queryClient,
          c.optimisticEntry.date,
          -c.optimisticEntry.hours,
        );
      }
      if (c?.previousTimer !== undefined) {
        queryClient.setQueryData(timerQuery.queryKey, c.previousTimer);
      }
      if (c?.previousTimerState) {
        setTimer(c.previousTimerState);
      }
      options?.onError?.(error, v, c?.optionsContext);
    },
  });
}

function mergeOptimisticTimerEdit(
  timer: TimerResponse,
  body: EditTimerPayload,
): TimerResponse {
  return {
    ...timer,
    note: body.userNote ?? timer.note,
    projectId: body.projectId === undefined ? timer.projectId : body.projectId,
    projectName:
      body.projectId === null ? null : (body.projectName ?? timer.projectName),
    activityId:
      body.activityId === undefined ? timer.activityId : body.activityId,
    activityName:
      body.activityId === null
        ? null
        : (body.activityName ?? timer.activityName),
    startTime: body.startTime ?? timer.startTime,
  };
}

function useEditTimer(
  options?: DefaultMutationOptions<EditTimerPayload, TimerResponse>,
) {
  const queryClient = useQueryClient();
  const timerQuery = timeTrackingQueries.getTimer();

  return useMutation({
    mutationKey: ["time-tracking", "editTimer"],
    mutationFn: (body: EditTimerPayload) =>
      api
        .patch("time-tracking/timer", {
          json: {
            projectId: body.projectId,
            activityId: body.activityId,
            note: body.userNote,
            startTime: body.startTime,
          },
        })
        .json<TimerResponse>(),
    ...options,
    onMutate: async (vars) => {
      await queryClient.cancelQueries({
        queryKey: timerQuery.queryKey,
      });

      const previousTimer = queryClient.getQueryData(timerQuery.queryKey);

      queryClient.setQueryData(timerQuery.queryKey, (current) =>
        current?.timer
          ? {
              ...current,
              timer: mergeOptimisticTimerEdit(current.timer, vars),
            }
          : current,
      );

      const optionsContext = await options?.onMutate?.(vars);
      return { previousTimer, optionsContext };
    },
    onSuccess: (data, v, c) => {
      setTimerCache(queryClient, data);
      options?.onSuccess?.(data, v, c?.optionsContext);
    },
    onError: (error, v, c) => {
      if (c?.previousTimer !== undefined) {
        queryClient.setQueryData(timerQuery.queryKey, c.previousTimer);
      }
      options?.onError?.(error, v, c?.optionsContext);
    },
    onSettled: (data, error, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timerQuery.queryKey,
      });
      options?.onSettled?.(data, error, v, c?.optionsContext);
    },
  });
}

function useUpdateTimeEntry(
  options?: DefaultMutationOptions<UpdateTimeEntryPayload, TimeEntry>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "updateTimeEntry"],
    mutationFn: (body: UpdateTimeEntryPayload) =>
      api
        .put(
          `time-tracking/time-entries/${encodeURIComponent(body.projectRegistrationId)}`,
          {
            json: {
              projectId: body.projectId,
              activityId: body.activityId,
              startTime: body.startTime,
              endTime: body.endTime,
              note: body.userNote,
            },
          },
        )
        .json<TimeEntry>(),
    ...options,
    onMutate: async (vars) => {
      await cancelTimeTrackingRangeQueries(queryClient);

      const previousEntry = findCachedEntry(
        queryClient,
        vars.projectRegistrationId,
      );
      const optimisticEntry = buildTimeEntryFromCreatePayload(
        vars,
        vars.projectRegistrationId,
      );

      replaceEntryInCachedRanges(
        queryClient,
        vars.projectRegistrationId,
        optimisticEntry,
      );
      if (previousEntry) {
        applyTimeInfoDelta(
          queryClient,
          previousEntry.date,
          -previousEntry.hours,
        );
      }
      applyTimeInfoDelta(
        queryClient,
        optimisticEntry.date,
        optimisticEntry.hours,
      );

      const optionsContext = await options?.onMutate?.(vars);
      return { previousEntry, optimisticEntry, optionsContext };
    },
    onSuccess: (data, v, c) => {
      replaceEntryInCachedRanges(queryClient, v.projectRegistrationId, data);
      markTimeTrackingListsStale(queryClient);
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntryDayStatusesBaseKey,
        refetchType: "none",
      });
      options?.onSuccess?.(data, v, c?.optionsContext);
    },
    onError: (error, v, c) => {
      removeEntryFromCachedRanges(queryClient, v.projectRegistrationId);
      if (c?.optimisticEntry) {
        applyTimeInfoDelta(
          queryClient,
          c.optimisticEntry.date,
          -c.optimisticEntry.hours,
        );
      }
      if (c?.previousEntry) {
        upsertEntryInCachedRanges(queryClient, c.previousEntry);
        applyTimeInfoDelta(
          queryClient,
          c.previousEntry.date,
          c.previousEntry.hours,
        );
      }
      options?.onError?.(error, v, c?.optionsContext);
    },
  });
}

function useDeleteTimeEntry(
  options?: DefaultMutationOptions<DeleteTimeEntryPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "deleteTimeEntry"],
    mutationFn: (body: DeleteTimeEntryPayload) =>
      api.delete(
        `time-tracking/time-entries/${encodeURIComponent(body.projectRegistrationId)}`,
      ),
    ...options,
    onMutate: async (vars) => {
      await cancelTimeTrackingRangeQueries(queryClient);

      const removedEntry = findCachedEntry(
        queryClient,
        vars.projectRegistrationId,
      );
      removeEntryFromCachedRanges(queryClient, vars.projectRegistrationId);
      if (removedEntry) {
        applyTimeInfoDelta(queryClient, removedEntry.date, -removedEntry.hours);
      }

      const optionsContext = await options?.onMutate?.(vars);
      return { removedEntry, optionsContext };
    },
    onSuccess: (data, v, c) => {
      markTimeTrackingListsStale(queryClient);
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntryDayStatusesBaseKey,
        refetchType: "none",
      });
      options?.onSuccess?.(data, v, c?.optionsContext);
    },
    onError: (error, v, c) => {
      if (c?.removedEntry) {
        upsertEntryInCachedRanges(queryClient, c.removedEntry);
        applyTimeInfoDelta(
          queryClient,
          c.removedEntry.date,
          c.removedEntry.hours,
        );
      }
      options?.onError?.(error, v, c?.optionsContext);
    },
  });
}

function useCreateTimeEntry(
  options?: DefaultMutationOptions<CreateTimeEntryPayload, TimeEntry>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "createTimeEntry"],
    mutationFn: (body: CreateTimeEntryPayload) =>
      api
        .post("time-tracking/time-entries", {
          headers: { "Idempotency-Key": crypto.randomUUID() },
          json: {
            projectId: body.projectId,
            activityId: body.activityId,
            startTime: body.startTime,
            endTime: body.endTime,
            note: body.userNote,
          },
        })
        .json<TimeEntry>(),
    ...options,
    onMutate: async (vars) => {
      await cancelTimeTrackingRangeQueries(queryClient);

      const optimisticId = `optimistic:create:${crypto.randomUUID()}`;
      const optimisticEntry = buildTimeEntryFromCreatePayload(
        vars,
        optimisticId,
      );
      upsertEntryInCachedRanges(queryClient, optimisticEntry);
      applyTimeInfoDelta(
        queryClient,
        optimisticEntry.date,
        optimisticEntry.hours,
      );

      const optionsContext = await options?.onMutate?.(vars);
      return { optimisticId, optimisticEntry, optionsContext };
    },
    onSuccess: (data, v, c) => {
      replaceEntryInCachedRanges(
        queryClient,
        c?.optimisticId ?? data.registrationId,
        data,
      );
      markTimeTrackingListsStale(queryClient);
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntryDayStatusesBaseKey,
        refetchType: "none",
      });
      options?.onSuccess?.(data, v, c?.optionsContext);
    },
    onError: (error, v, c) => {
      if (c?.optimisticEntry) {
        removeEntryFromCachedRanges(
          queryClient,
          c.optimisticEntry.registrationId,
        );
        applyTimeInfoDelta(
          queryClient,
          c.optimisticEntry.date,
          -c.optimisticEntry.hours,
        );
      }
      options?.onError?.(error, v, c?.optionsContext);
    },
  });
}

function invalidateAbsenceQueries(
  queryClient: ReturnType<typeof useQueryClient>,
) {
  queryClient.invalidateQueries({
    queryKey: timeTrackingQueries.absenceEntriesBaseKey,
  });
  queryClient.invalidateQueries({
    queryKey: timeTrackingQueries.absenceDayDefaultsBaseKey,
  });
  queryClient.invalidateQueries({
    queryKey: timeTrackingQueries.timeInfoBaseKey,
  });
}

function buildOptimisticAbsences(
  payload: CreateAbsencesPayload,
): Array<AbsenceEntry> {
  const batchId = crypto.randomUUID();
  const comment = payload.comment.trim() ? payload.comment : null;

  return payload.days.map((day) => ({
    absenceId: `optimistic:absence:create:${batchId}:${day.date}`,
    date: day.date,
    hours: day.hours,
    absenceType: payload.absenceType,
    absenceTypeLabel: absenceTypeLabels[payload.absenceType],
    child: payload.child,
    comment,
    managed: true,
    deletable: false,
  }));
}

function applyAbsenceEntriesTimeInfoDelta(
  queryClient: ReturnType<typeof useQueryClient>,
  entries: Array<AbsenceEntry>,
  direction: 1 | -1,
) {
  for (const entry of entries) {
    applyAbsenceTimeInfoDelta(queryClient, entry.date, entry.hours * direction);
  }
}

function useCreateAbsences(
  options?: DefaultMutationOptions<CreateAbsencesPayload, Array<AbsenceEntry>>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "createAbsences"],
    mutationFn: (body: CreateAbsencesPayload) =>
      api
        .post("time-tracking/absences", {
          json: body,
        })
        .json<Array<AbsenceEntry>>(),
    ...options,
    onMutate: async (vars) => {
      await cancelAbsenceQueries(queryClient);

      const optimisticEntries = buildOptimisticAbsences(vars);
      for (const entry of optimisticEntries) {
        upsertAbsenceInCachedRanges(queryClient, entry);
      }
      applyAbsenceEntriesTimeInfoDelta(queryClient, optimisticEntries, 1);

      const optionsContext = await options?.onMutate?.(vars);
      return { optimisticEntries, optionsContext };
    },
    onSuccess: (data, v, c) => {
      if (c?.optimisticEntries) {
        replaceAbsencesInCachedRanges(
          queryClient,
          c.optimisticEntries.map((entry) => entry.absenceId),
          data,
        );
      } else {
        for (const entry of data) {
          upsertAbsenceInCachedRanges(queryClient, entry);
        }
      }
      invalidateAbsenceQueries(queryClient);
      options?.onSuccess?.(data, v, c?.optionsContext);
    },
    onError: (error, v, c) => {
      if (c?.optimisticEntries) {
        for (const entry of c.optimisticEntries) {
          removeAbsenceFromCachedRanges(queryClient, entry.absenceId);
        }
        applyAbsenceEntriesTimeInfoDelta(queryClient, c.optimisticEntries, -1);
      }
      options?.onError?.(error, v, c?.optionsContext);
    },
  });
}

function useDeleteAbsence(
  options?: DefaultMutationOptions<DeleteAbsencePayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "deleteAbsence"],
    mutationFn: (body: DeleteAbsencePayload) =>
      api.delete("time-tracking/absences", {
        json: body,
      }),
    ...options,
    onMutate: async (vars) => {
      await cancelAbsenceQueries(queryClient);

      const removedAbsence = findCachedAbsence(queryClient, vars.absenceId);
      if (removedAbsence) {
        removeAbsenceFromCachedRanges(queryClient, vars.absenceId);
        applyAbsenceEntriesTimeInfoDelta(queryClient, [removedAbsence], -1);
      }

      const optionsContext = await options?.onMutate?.(vars);
      return { removedAbsence, optionsContext };
    },
    onSuccess: (data, v, c) => {
      invalidateAbsenceQueries(queryClient);
      options?.onSuccess?.(data, v, c?.optionsContext);
    },
    onError: (error, v, c) => {
      if (c?.removedAbsence) {
        upsertAbsenceInCachedRanges(queryClient, c.removedAbsence);
        applyAbsenceEntriesTimeInfoDelta(queryClient, [c.removedAbsence], 1);
      }
      options?.onError?.(error, v, c?.optionsContext);
    },
  });
}

function useImportKleerUsers(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "admin", "importKleerUsers"],
    mutationFn: () => api.post("time-tracking/admin/kleer-users/import"),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useLinkKleerUsersByEmail(
  options?: DefaultMutationOptions<void, LinkKleerUsersByEmailResponse>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "admin", "linkKleerUsersByEmail"],
    mutationFn: () =>
      api
        .post("time-tracking/admin/kleer-users/link-by-email")
        .json<LinkKleerUsersByEmailResponse>(),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      queryClient.invalidateQueries({ queryKey: ["time-tracking"] });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useUpsertKleerUserLink(
  options?: DefaultMutationOptions<UpsertKleerUserLinkPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "admin", "upsertKleerUserLink"],
    mutationFn: (body: UpsertKleerUserLinkPayload) =>
      api.put("time-tracking/admin/user-links", { json: body }),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      queryClient.invalidateQueries({ queryKey: ["time-tracking"] });
      options?.onSuccess?.(data, v, c);
    },
  });
}

function useDeactivateKleerUserLink(
  options?: DefaultMutationOptions<DeactivateKleerUserLinkPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "admin", "deactivateKleerUserLink"],
    mutationFn: (body: DeactivateKleerUserLinkPayload) =>
      api.delete(`time-tracking/admin/user-links/${body.userId}`),
    ...options,
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      queryClient.invalidateQueries({ queryKey: ["time-tracking"] });
      options?.onSuccess?.(data, v, c);
    },
  });
}

/** Starts a timer using opaque provider-neutral selection IDs. */
export type StartTimerPayload = {
  userNote?: string;
  projectId?: string;
  activityId?: string;
};

/** Async start-timer mutation callable used by shared time-report actions. */
export type StartTimerMutationAsync = MutationFnAsync<typeof useStartTimer>;

/** Saves the active timer, optionally replacing its note. */
export type SaveTimerPayload = {
  userNote?: string;
};

/** Partially updates the active timer; null clears a selection. */
export type EditTimerPayload = {
  userNote?: string;
  projectId?: string | null;
  projectName?: string;
  activityId?: string | null;
  activityName?: string;
  startTime?: string;
};

/** Async update-timer mutation callable used by shared time-report actions. */
export type EditTimerMutationAsync = MutationFnAsync<typeof useEditTimer>;

/** Updates an entry; display names are local optimistic-cache hints only. */
export type UpdateTimeEntryPayload = {
  projectRegistrationId: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  startTime: string;
  endTime: string;
  userNote: string;
};

/** Deletes the time entry identified by its opaque registration ID. */
export type DeleteTimeEntryPayload = {
  projectRegistrationId: string;
};

/** Creates an entry; display names are local optimistic-cache hints only. */
export type CreateTimeEntryPayload = {
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  startTime: string; // ISO
  endTime: string; // ISO
  userNote: string;
};

export type CreateAbsencesPayload = {
  absenceType: ManagedAbsenceType;
  child: string | null;
  comment: string;
  days: Array<{
    date: string;
    hours: number;
  }>;
};

export type DeleteAbsencePayload = {
  absenceId: string;
  date: string;
};

export type UpsertKleerUserLinkPayload = {
  userId: number;
  providerUserId: string;
};

export type LinkKleerUsersByEmailResponse = {
  createdLinkCount: number;
};

export type DeactivateKleerUserLinkPayload = {
  userId: number;
};
