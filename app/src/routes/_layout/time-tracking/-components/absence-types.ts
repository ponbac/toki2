import type React from "react";
import {
  Baby,
  BriefcaseBusiness,
  Coffee,
  HeartHandshake,
  Palmtree,
  Thermometer,
  Umbrella,
  UserRoundPlus,
} from "lucide-react";
import type { AbsenceType } from "@/lib/api/queries/time-tracking";

export const absenceTypeIcons: Record<AbsenceType, React.ElementType> = {
  paternityLeave: UserRoundPlus,
  parentalLeave: Baby,
  furlough: Coffee,
  vacation: Palmtree,
  sick: Thermometer,
  leaveOfAbsence: BriefcaseBusiness,
  leaveOfAbsenceVacationEarned: BriefcaseBusiness,
  childcare: Umbrella,
  closeRelativeCare: HeartHandshake,
  otherLeave: Coffee,
  otherLeaveVacationNotEarned: Coffee,
};

export const absenceTypeAccent: Record<AbsenceType, string> = {
  sick: "border-l-red-400/70 bg-red-500/5",
  vacation: "border-l-emerald-400/70 bg-emerald-500/5",
  parentalLeave: "border-l-pink-400/70 bg-pink-500/5",
  childcare: "border-l-sky-400/70 bg-sky-500/5",
  paternityLeave: "border-l-violet-400/70 bg-violet-500/5",
  furlough: "border-l-amber-400/70 bg-amber-500/5",
  leaveOfAbsence: "border-l-slate-400/70 bg-slate-500/5",
  leaveOfAbsenceVacationEarned: "border-l-slate-400/70 bg-slate-500/5",
  closeRelativeCare: "border-l-rose-400/70 bg-rose-500/5",
  otherLeave: "border-l-zinc-400/70 bg-zinc-500/5",
  otherLeaveVacationNotEarned: "border-l-zinc-400/70 bg-zinc-500/5",
};

export const absenceTypeColors: Record<AbsenceType, string> = {
  sick: "hsl(0 84% 60%)",
  vacation: "hsl(142 71% 45%)",
  parentalLeave: "hsl(330 81% 60%)",
  childcare: "hsl(199 89% 48%)",
  paternityLeave: "hsl(262 83% 58%)",
  furlough: "hsl(38 95% 55%)",
  leaveOfAbsence: "hsl(215 16% 47%)",
  leaveOfAbsenceVacationEarned: "hsl(215 16% 47%)",
  closeRelativeCare: "hsl(350 89% 60%)",
  otherLeave: "hsl(240 5% 46%)",
  otherLeaveVacationNotEarned: "hsl(240 5% 46%)",
};
