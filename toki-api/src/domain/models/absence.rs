use serde::{Deserialize, Serialize};
use time::Date;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AbsenceType {
    Sick,
    Vacation,
    LeaveOfAbsence,
    LeaveOfAbsenceVacationEarned,
    ParentalLeave,
    Childcare,
    CloseRelativeCare,
    PaternityLeave,
    Furlough,
    OtherLeave,
    OtherLeaveVacationNotEarned,
}

impl AbsenceType {
    pub fn requires_child(self) -> bool {
        matches!(self, Self::ParentalLeave | Self::Childcare)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sick => "Sick (Sjuk)",
            Self::Vacation => "Vacation (Semester)",
            Self::LeaveOfAbsence => "Leave of absence (Tjänstledig)",
            Self::LeaveOfAbsenceVacationEarned => {
                "Leave of absence, vacation earned (Tjänstledig (Semestergrundande))"
            }
            Self::ParentalLeave => "Parental leave (Föräldraledighet)",
            Self::Childcare => "Childcare (VAB)",
            Self::CloseRelativeCare => "Close relative care (Vård av nära anhörig)",
            Self::PaternityLeave => "Paternity leave (10 dagar vid barns födelse)",
            Self::Furlough => "Furlough (Permission)",
            Self::OtherLeave => "Other leave (Övrig frånvaro)",
            Self::OtherLeaveVacationNotEarned => {
                "Other leave, vacation not earned (Övrig frånvaro (Semestergrundande))"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbsenceEntry {
    pub absence_id: String,
    pub date: Date,
    pub hours: f64,
    pub absence_type: AbsenceType,
    pub child: Option<String>,
    pub comment: Option<String>,
    pub managed: bool,
    pub deletable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsenceChild {
    pub name: String,
    pub birth_date: Option<Date>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbsenceDayDefault {
    pub date: Date,
    pub scheduled_hours: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateAbsencesRequest {
    pub absence_type: AbsenceType,
    pub child: Option<String>,
    pub comment: String,
    pub days: Vec<CreateAbsenceDay>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateAbsenceDay {
    pub date: Date,
    pub hours: f64,
}
