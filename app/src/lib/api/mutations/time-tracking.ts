import { useMutation, useQueryClient } from "@tanstack/react-query";
import dayjs from "dayjs";
import { api } from "../api";
import { emitBuddyEvent } from "@/lib/buddy/events";
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
  invalidateFlexBalance,
  reloadQueries,
  NO_RANGES,
  type OptimisticRanges,
  removeAbsenceFromCachedRanges,
  removeEntryFromCachedRanges,
  replaceAbsencesInCachedRanges,
  replaceEntryInCachedRanges,
  setTimerCache,
  settleRangeMutation,
  upsertAbsenceInCachedRanges,
  upsertEntryInCachedRanges,
} from "../time-tracking-cache";

export const timeTrackingMutations = {
  useStartTimer,
  useStopTimer,
  useSaveTimer,
  useEditTimer,
  useEditProjectRegistration,
  useDeleteProjectRegistration,
  useCreateProjectRegistration,
  useCreateAbsences,
  useDeleteAbsence,
  useLockTimeEntries,
  useUnlockTimeEntries,
  useImportKleerUsers,
  useLinkKleerUsersByEmail,
  useUpsertKleerUserLink,
  useDeactivateKleerUserLink,
};

function useStartTimer(options?: DefaultMutationOptions<StartTimerPayload>) {
  const queryClient = useQueryClient();
  const { setTimer } = useTimeTrackingActions();

  return useMutation({
    mutationKey: ["time-tracking", "startTimer"],
    mutationFn: (body: StartTimerPayload) =>
      api.post("time-tracking/timer", {
        json: body,
      }),
    ...options,
    onMutate: (vars, mutationContext) => {
      queryClient.resetQueries({
        queryKey: timeTrackingQueries.getTimer().queryKey,
      });
      return options?.onMutate?.(vars, mutationContext);
    },
    onSuccess: (data, v, c, mutationContext) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timerBaseKey,
      });
      setTimer({
        visible: true,
        state: "running",
        timeSeconds: 0,
      });

      return options?.onSuccess?.(data, v, c, mutationContext);
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
    onSuccess: (data, v, c, mutationContext) => {
      const deletedSeconds =
        useTimeTrackingStore.getState().timer.timeSeconds ?? 0;
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timerBaseKey,
      });
      setTimer({
        visible: false,
        state: "stopped",
        timeSeconds: null,
      });
      emitBuddyEvent({ type: "deleted", seconds: deletedSeconds });

      return options?.onSuccess?.(data, v, c, mutationContext);
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
        .put("time-tracking/timer", {
          json: body,
        })
        .json<SaveTimerResponse>(),
    ...options,
    onMutate: async (vars, mutationContext) => {
      // Before the optimistic update below, so the buddy sees today's total without this entry.
      emitBuddyEvent({ type: "saveStarted" });
      await queryClient.cancelQueries({ queryKey: timerQuery.queryKey });
      const ranges = await cancelTimeTrackingRangeQueries(queryClient);

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
        upsertEntryInCachedRanges(queryClient, ranges, optimisticEntry);
        applyTimeInfoDelta(
          queryClient,
          ranges,
          optimisticEntry.date,
          optimisticEntry.hours,
        );
      }

      const restartTimer = vars.restartTimer
        ? ({
            startTime: new Date().toISOString(),
            projectId: vars.restartTimer.projectId ?? null,
            projectName: vars.restartTimer.projectName ?? null,
            activityId: vars.restartTimer.activityId ?? null,
            activityName: vars.restartTimer.activityName ?? null,
            note: vars.restartTimer.userNote,
            hours: 0,
            minutes: 0,
            seconds: 0,
          } satisfies TimerResponse)
        : null;

      setTimerCache(queryClient, restartTimer);
      setTimer(
        restartTimer
          ? { visible: true, state: "running", timeSeconds: 0 }
          : { visible: false, state: "stopped", timeSeconds: null },
      );

      const optionsContext = await options?.onMutate?.(vars, mutationContext);
      return {
        ranges,
        previousTimer,
        previousTimerState,
        optimisticEntry,
        optimisticId,
        optionsContext,
      };
    },
    onSuccess: (data, v, c, mutationContext) => {
      const ranges = c?.ranges ?? NO_RANGES;
      if (c?.optimisticEntry) {
        replaceEntryInCachedRanges(
          queryClient,
          ranges,
          c.optimisticId,
          data.entry,
        );
        // The server books the entry on its start's UTC date and with its own
        // end time, so the optimistic hours move to what it booked.
        applyTimeInfoDelta(
          queryClient,
          ranges,
          c.optimisticEntry.date,
          -c.optimisticEntry.hours,
        );
        applyTimeInfoDelta(
          queryClient,
          ranges,
          data.entry.date,
          data.entry.hours,
        );
      } else {
        upsertEntryInCachedRanges(queryClient, ranges, data.entry);
      }
      setTimerCache(queryClient, data.timer);
      setTimer(
        data.timer
          ? { visible: true, state: "running", timeSeconds: 0 }
          : { visible: false, state: "stopped", timeSeconds: null },
      );
      invalidateFlexBalance(queryClient);
      emitBuddyEvent({
        type: "saved",
        seconds: Math.round(data.entry.hours * 3600),
        countsTowardToday: data.entry.date === dayjs().format("YYYY-MM-DD"),
      });
      return options?.onSuccess?.(data, v, c?.optionsContext, mutationContext);
    },
    onError: (error, v, c, mutationContext) => {
      if (c?.optimisticEntry) {
        removeEntryFromCachedRanges(queryClient, c.ranges, c.optimisticId);
        applyTimeInfoDelta(
          queryClient,
          c.ranges,
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
      return options?.onError?.(error, v, c?.optionsContext, mutationContext);
    },
    onSettled: (data, error, v, c, mutationContext) => {
      settleRangeMutation(queryClient, c?.ranges);
      return options?.onSettled?.(
        data,
        error,
        v,
        c?.optionsContext,
        mutationContext,
      );
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
    projectId: body.projectId ?? timer.projectId,
    projectName: body.projectName ?? timer.projectName,
    activityId: body.activityId ?? timer.activityId,
    activityName: body.activityName ?? timer.activityName,
    startTime: body.startTime ?? timer.startTime,
  };
}

function useEditTimer(options?: DefaultMutationOptions<EditTimerPayload>) {
  const queryClient = useQueryClient();
  const timerQuery = timeTrackingQueries.getTimer();

  return useMutation({
    mutationKey: ["time-tracking", "editTimer"],
    mutationFn: (body: EditTimerPayload) =>
      api.put("time-tracking/update-timer", {
        json: body,
      }),
    ...options,
    onMutate: async (vars, mutationContext) => {
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

      const optionsContext = await options?.onMutate?.(vars, mutationContext);
      return { previousTimer, optionsContext };
    },
    onSuccess: (data, v, c, mutationContext) => {
      return options?.onSuccess?.(data, v, c?.optionsContext, mutationContext);
    },
    onError: (error, v, c, mutationContext) => {
      if (c?.previousTimer !== undefined) {
        queryClient.setQueryData(timerQuery.queryKey, c.previousTimer);
      }
      return options?.onError?.(error, v, c?.optionsContext, mutationContext);
    },
    onSettled: (data, error, v, c, mutationContext) => {
      queryClient.invalidateQueries({
        queryKey: timerQuery.queryKey,
      });
      return options?.onSettled?.(
        data,
        error,
        v,
        c?.optionsContext,
        mutationContext,
      );
    },
  });
}

function useEditProjectRegistration(
  options?: DefaultMutationOptions<EditProjectRegistrationPayload, TimeEntry>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "editProjectRegistration"],
    mutationFn: (body: EditProjectRegistrationPayload) =>
      api
        .put("time-tracking/time-entries", {
          json: body,
        })
        .json<TimeEntry>(),
    ...options,
    onMutate: async (vars, mutationContext) => {
      const ranges = await cancelTimeTrackingRangeQueries(queryClient);

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
        ranges,
        vars.projectRegistrationId,
        optimisticEntry,
      );
      if (previousEntry) {
        applyTimeInfoDelta(
          queryClient,
          ranges,
          previousEntry.date,
          -previousEntry.hours,
        );
      }
      applyTimeInfoDelta(
        queryClient,
        ranges,
        optimisticEntry.date,
        optimisticEntry.hours,
      );

      const optionsContext = await options?.onMutate?.(vars, mutationContext);
      return { ranges, previousEntry, optimisticEntry, optionsContext };
    },
    onSuccess: (data, v, c, mutationContext) => {
      replaceEntryInCachedRanges(
        queryClient,
        c?.ranges ?? NO_RANGES,
        v.projectRegistrationId,
        data,
      );
      invalidateFlexBalance(queryClient);
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntryDayStatusesBaseKey,
        refetchType: "none",
      });
      return options?.onSuccess?.(data, v, c?.optionsContext, mutationContext);
    },
    onError: (error, v, c, mutationContext) => {
      const ranges = c?.ranges ?? NO_RANGES;
      removeEntryFromCachedRanges(queryClient, ranges, v.projectRegistrationId);
      if (c?.optimisticEntry) {
        applyTimeInfoDelta(
          queryClient,
          ranges,
          c.optimisticEntry.date,
          -c.optimisticEntry.hours,
        );
      }
      if (c?.previousEntry) {
        upsertEntryInCachedRanges(queryClient, ranges, c.previousEntry);
        applyTimeInfoDelta(
          queryClient,
          ranges,
          c.previousEntry.date,
          c.previousEntry.hours,
        );
      }
      return options?.onError?.(error, v, c?.optionsContext, mutationContext);
    },
    onSettled: (data, error, v, c, mutationContext) => {
      settleRangeMutation(queryClient, c?.ranges);
      return options?.onSettled?.(
        data,
        error,
        v,
        c?.optionsContext,
        mutationContext,
      );
    },
  });
}

function useDeleteProjectRegistration(
  options?: DefaultMutationOptions<DeleteProjectRegistrationPayload>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "deleteProjectRegistration"],
    mutationFn: (body: DeleteProjectRegistrationPayload) =>
      api.delete("time-tracking/time-entries", {
        json: body,
      }),
    ...options,
    onMutate: async (vars, mutationContext) => {
      const ranges = await cancelTimeTrackingRangeQueries(queryClient);

      const removedEntry = findCachedEntry(
        queryClient,
        vars.projectRegistrationId,
      );
      removeEntryFromCachedRanges(
        queryClient,
        ranges,
        vars.projectRegistrationId,
      );
      if (removedEntry) {
        applyTimeInfoDelta(
          queryClient,
          ranges,
          removedEntry.date,
          -removedEntry.hours,
        );
      }

      const optionsContext = await options?.onMutate?.(vars, mutationContext);
      return { ranges, removedEntry, optionsContext };
    },
    onSuccess: (data, v, c, mutationContext) => {
      invalidateFlexBalance(queryClient);
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntryDayStatusesBaseKey,
        refetchType: "none",
      });
      return options?.onSuccess?.(data, v, c?.optionsContext, mutationContext);
    },
    onError: (error, v, c, mutationContext) => {
      if (c?.removedEntry) {
        upsertEntryInCachedRanges(queryClient, c.ranges, c.removedEntry);
        applyTimeInfoDelta(
          queryClient,
          c.ranges,
          c.removedEntry.date,
          c.removedEntry.hours,
        );
      }
      return options?.onError?.(error, v, c?.optionsContext, mutationContext);
    },
    onSettled: (data, error, v, c, mutationContext) => {
      settleRangeMutation(queryClient, c?.ranges);
      return options?.onSettled?.(
        data,
        error,
        v,
        c?.optionsContext,
        mutationContext,
      );
    },
  });
}

function useCreateProjectRegistration(
  options?: DefaultMutationOptions<CreateProjectRegistrationPayload, TimeEntry>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "createProjectRegistration"],
    mutationFn: (body: CreateProjectRegistrationPayload) =>
      api
        .post("time-tracking/time-entries", {
          json: body,
        })
        .json<TimeEntry>(),
    ...options,
    onMutate: async (vars, mutationContext) => {
      const ranges = await cancelTimeTrackingRangeQueries(queryClient);

      const optimisticId = `optimistic:create:${crypto.randomUUID()}`;
      const optimisticEntry = buildTimeEntryFromCreatePayload(
        vars,
        optimisticId,
      );
      upsertEntryInCachedRanges(queryClient, ranges, optimisticEntry);
      applyTimeInfoDelta(
        queryClient,
        ranges,
        optimisticEntry.date,
        optimisticEntry.hours,
      );

      const optionsContext = await options?.onMutate?.(vars, mutationContext);
      return { ranges, optimisticId, optimisticEntry, optionsContext };
    },
    onSuccess: (data, v, c, mutationContext) => {
      replaceEntryInCachedRanges(
        queryClient,
        c?.ranges ?? NO_RANGES,
        c?.optimisticId ?? data.registrationId,
        data,
      );
      invalidateFlexBalance(queryClient);
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timeEntryDayStatusesBaseKey,
        refetchType: "none",
      });
      return options?.onSuccess?.(data, v, c?.optionsContext, mutationContext);
    },
    onError: (error, v, c, mutationContext) => {
      if (c?.optimisticEntry) {
        removeEntryFromCachedRanges(
          queryClient,
          c.ranges,
          c.optimisticEntry.registrationId,
        );
        applyTimeInfoDelta(
          queryClient,
          c.ranges,
          c.optimisticEntry.date,
          -c.optimisticEntry.hours,
        );
      }
      return options?.onError?.(error, v, c?.optionsContext, mutationContext);
    },
    onSettled: (data, error, v, c, mutationContext) => {
      settleRangeMutation(queryClient, c?.ranges);
      return options?.onSettled?.(
        data,
        error,
        v,
        c?.optionsContext,
        mutationContext,
      );
    },
  });
}

/** The absence lists and hours reload when the mutation settles. */
function invalidateAbsenceQueries(
  queryClient: ReturnType<typeof useQueryClient>,
) {
  reloadQueries(queryClient, timeTrackingQueries.absenceDayDefaultsBaseKey);
  invalidateFlexBalance(queryClient);
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
  ranges: OptimisticRanges,
  entries: Array<AbsenceEntry>,
  direction: 1 | -1,
) {
  for (const entry of entries) {
    applyAbsenceTimeInfoDelta(
      queryClient,
      ranges,
      entry.date,
      entry.hours * direction,
    );
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
    onMutate: async (vars, mutationContext) => {
      const ranges = await cancelAbsenceQueries(queryClient);

      const optimisticEntries = buildOptimisticAbsences(vars);
      for (const entry of optimisticEntries) {
        upsertAbsenceInCachedRanges(queryClient, ranges, entry);
      }
      applyAbsenceEntriesTimeInfoDelta(
        queryClient,
        ranges,
        optimisticEntries,
        1,
      );

      const optionsContext = await options?.onMutate?.(vars, mutationContext);
      return { ranges, optimisticEntries, optionsContext };
    },
    onSuccess: (data, v, c, mutationContext) => {
      const ranges = c?.ranges ?? NO_RANGES;
      if (c?.optimisticEntries) {
        replaceAbsencesInCachedRanges(
          queryClient,
          ranges,
          c.optimisticEntries.map((entry) => entry.absenceId),
          data,
        );
      } else {
        for (const entry of data) {
          upsertAbsenceInCachedRanges(queryClient, ranges, entry);
        }
      }
      invalidateAbsenceQueries(queryClient);
      return options?.onSuccess?.(data, v, c?.optionsContext, mutationContext);
    },
    onError: (error, v, c, mutationContext) => {
      if (c?.optimisticEntries) {
        for (const entry of c.optimisticEntries) {
          removeAbsenceFromCachedRanges(queryClient, c.ranges, entry.absenceId);
        }
        applyAbsenceEntriesTimeInfoDelta(
          queryClient,
          c.ranges,
          c.optimisticEntries,
          -1,
        );
      }
      return options?.onError?.(error, v, c?.optionsContext, mutationContext);
    },
    onSettled: (data, error, v, c, mutationContext) => {
      settleRangeMutation(queryClient, c?.ranges);
      return options?.onSettled?.(
        data,
        error,
        v,
        c?.optionsContext,
        mutationContext,
      );
    },
  });
}

type TimeEntryPeriod = { from: string; to: string };
type UnlockTimeEntriesResponse = { stillLocked: Array<string> };

function refreshLockedTime(queryClient: ReturnType<typeof useQueryClient>) {
  queryClient.invalidateQueries({
    queryKey: timeTrackingQueries.timeEntriesBaseKey,
  });
  queryClient.invalidateQueries({
    queryKey: timeTrackingQueries.timeEntryDayStatusesBaseKey,
  });
}

function useLockTimeEntries(
  options?: DefaultMutationOptions<TimeEntryPeriod, void>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (body: TimeEntryPeriod) => {
      await api.post("time-tracking/lock", { json: body });
    },
    ...options,
    onSettled: (...args) => {
      refreshLockedTime(queryClient);
      return options?.onSettled?.(...args);
    },
  });
}

function useUnlockTimeEntries(
  options?: DefaultMutationOptions<TimeEntryPeriod, UnlockTimeEntriesResponse>,
) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (body: TimeEntryPeriod) =>
      api
        .post("time-tracking/unlock", { json: body })
        .json<UnlockTimeEntriesResponse>(),
    ...options,
    onSettled: (...args) => {
      refreshLockedTime(queryClient);
      return options?.onSettled?.(...args);
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
    onMutate: async (vars, mutationContext) => {
      const ranges = await cancelAbsenceQueries(queryClient);

      const removedAbsence = findCachedAbsence(queryClient, vars.absenceId);
      if (removedAbsence) {
        removeAbsenceFromCachedRanges(queryClient, ranges, vars.absenceId);
        applyAbsenceEntriesTimeInfoDelta(
          queryClient,
          ranges,
          [removedAbsence],
          -1,
        );
      }

      const optionsContext = await options?.onMutate?.(vars, mutationContext);
      return { ranges, removedAbsence, optionsContext };
    },
    onSuccess: (data, v, c, mutationContext) => {
      invalidateAbsenceQueries(queryClient);
      return options?.onSuccess?.(data, v, c?.optionsContext, mutationContext);
    },
    onError: (error, v, c, mutationContext) => {
      if (c?.removedAbsence) {
        upsertAbsenceInCachedRanges(queryClient, c.ranges, c.removedAbsence);
        applyAbsenceEntriesTimeInfoDelta(
          queryClient,
          c.ranges,
          [c.removedAbsence],
          1,
        );
      }
      return options?.onError?.(error, v, c?.optionsContext, mutationContext);
    },
    onSettled: (data, error, v, c, mutationContext) => {
      settleRangeMutation(queryClient, c?.ranges);
      return options?.onSettled?.(
        data,
        error,
        v,
        c?.optionsContext,
        mutationContext,
      );
    },
  });
}

function useImportKleerUsers(options?: DefaultMutationOptions<void>) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: ["time-tracking", "admin", "importKleerUsers"],
    mutationFn: () => api.post("time-tracking/admin/kleer-users/import"),
    ...options,
    onSuccess: (data, v, c, mutationContext) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      return options?.onSuccess?.(data, v, c, mutationContext);
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
    onSuccess: (data, v, c, mutationContext) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      queryClient.invalidateQueries({ queryKey: ["time-tracking"] });
      return options?.onSuccess?.(data, v, c, mutationContext);
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
    onSuccess: (data, v, c, mutationContext) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      queryClient.invalidateQueries({ queryKey: ["time-tracking"] });
      return options?.onSuccess?.(data, v, c, mutationContext);
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
    onSuccess: (data, v, c, mutationContext) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.adminMappings().queryKey,
      });
      queryClient.invalidateQueries({ queryKey: ["time-tracking"] });
      return options?.onSuccess?.(data, v, c, mutationContext);
    },
  });
}

export type StartTimerPayload = {
  userNote?: string;
  projectId?: string;
  projectName?: string;
  activityId?: string;
  activityName?: string;
};

export type StartTimerMutationAsync = MutationFnAsync<typeof useStartTimer>;

export type SaveTimerPayload = {
  userNote?: string;
  restartTimer?: {
    userNote: string;
    projectId?: string;
    projectName?: string;
    activityId?: string;
    activityName?: string;
  };
};

export type EditTimerPayload = {
  userNote?: string;
  projectId?: string;
  projectName?: string;
  activityId?: string;
  activityName?: string;
  startTime?: string;
};

export type EditTimerMutationAsync = MutationFnAsync<typeof useEditTimer>;

export type EditProjectRegistrationPayload = {
  projectRegistrationId: string;
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  startTime: string;
  endTime: string;
  regDay: string;
  weekNumber: number;
  userNote: string;
  originalRegDay?: string;
  originalProjectId?: string;
  originalActivityId?: string;
};

export type DeleteProjectRegistrationPayload = {
  projectRegistrationId: string;
};

export type UpdateTimeEntryPayload = {
  id: string;
  note: string;
  hours: number;
};

export type CreateProjectRegistrationPayload = {
  projectId: string;
  projectName: string;
  activityId: string;
  activityName: string;
  startTime: string; // ISO
  endTime: string; // ISO
  regDay: string; // YYYY-MM-DD
  weekNumber: number;
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
