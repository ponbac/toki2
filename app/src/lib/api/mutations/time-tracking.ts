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
    onMutate: (vars) => {
      queryClient.resetQueries({
        queryKey: timeTrackingQueries.getTimer().queryKey,
      });
      options?.onMutate?.(vars);
    },
    onSuccess: (data, v, c) => {
      queryClient.invalidateQueries({
        queryKey: timeTrackingQueries.timerBaseKey,
      });
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
        .put("time-tracking/timer", {
          json: body,
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
      setTimerCache(queryClient, data.timer);
      setTimer(
        data.timer
          ? { visible: true, state: "running", timeSeconds: 0 }
          : { visible: false, state: "stopped", timeSeconds: null },
      );
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
    applyAbsenceTimeInfoDelta(
      queryClient,
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
