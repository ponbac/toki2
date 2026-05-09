use serde::{Deserialize, Serialize};
use time::Date;

fn optional_string_from_any<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(serde_json::Value::Number(value)) => Ok(Some(value.to_string())),
        Some(value) => Err(serde::de::Error::custom(format!(
            "expected string, number, or null, got {value}"
        ))),
    }
}

mod date_format {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::Date;

    pub fn serialize<S>(date: &Date, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let format = time::format_description::parse("[year]-[month]-[day]")
            .expect("valid Kleer date format");
        serializer.serialize_str(&date.format(&format).map_err(serde::ser::Error::custom)?)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Date, D::Error>
    where
        D: Deserializer<'de>,
    {
        let format = time::format_description::parse("[year]-[month]-[day]")
            .expect("valid Kleer date format");
        let value = String::deserialize(deserializer)?;
        Date::parse(&value, &format).map_err(serde::de::Error::custom)
    }
}

mod option_date_format {
    use serde::{Deserialize, Deserializer, Serializer};
    use time::Date;

    pub fn serialize<S>(value: &Option<Date>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(date) => super::date_format::serialize(date, serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        value
            .map(|value| {
                let format = time::format_description::parse("[year]-[month]-[day]")
                    .expect("valid Kleer date format");
                Date::parse(&value, &format).map_err(serde::de::Error::custom)
            })
            .transpose()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KleerIdRef {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KleerUserMe {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerUserReadable {
    pub id: i64,
    #[serde(default, deserialize_with = "optional_string_from_any")]
    pub foreign_id: Option<String>,
    #[serde(default, deserialize_with = "optional_string_from_any")]
    pub internal_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    pub active: bool,
    #[serde(default)]
    pub dimension_entry: Option<serde_json::Value>,
    #[serde(default)]
    pub dimensions: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KleerUserList {
    #[serde(default)]
    pub users: Vec<KleerUserReadable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerActivityReadable {
    pub id: KleerIdRef,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mandatory_child_when_reporting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerActivityList {
    #[serde(default)]
    pub activity_readables: Vec<KleerActivityReadable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KleerProjectActivityAssignment {
    pub activity: KleerIdRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KleerProjectUserAssignment {
    pub user: KleerIdRef,
    #[serde(default)]
    pub activities: Vec<KleerProjectActivityAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerClientProjectReadable {
    pub id: KleerIdRef,
    #[serde(default)]
    pub number: String,
    pub name: String,
    pub active: bool,
    #[serde(default)]
    pub all_activities: bool,
    #[serde(default)]
    pub activities: Vec<KleerProjectActivityAssignment>,
    #[serde(default)]
    pub all_users: bool,
    #[serde(default)]
    pub users: Vec<KleerProjectUserAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerClientProjectList {
    #[serde(default)]
    pub client_project_readables: Vec<KleerClientProjectReadable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KleerStatusType {
    Open,
    Approved,
    Certified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerEventStatus {
    #[serde(rename = "type")]
    pub status_type: KleerStatusType,
    pub registration_user: Option<KleerIdRef>,
    #[serde(with = "option_date_format", default)]
    pub registration_date: Option<Date>,
    #[serde(with = "option_date_format", default)]
    pub event_date: Option<Date>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerEventReadable {
    pub id: KleerIdRef,
    #[serde(default)]
    pub foreign_id: String,
    pub user: KleerIdRef,
    pub activity: KleerIdRef,
    pub client_project: Option<KleerIdRef>,
    #[serde(default)]
    pub child: Option<String>,
    #[serde(with = "date_format")]
    pub date: Date,
    pub hours: f64,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub internal_comment: Option<String>,
    #[serde(default)]
    pub approved: Option<bool>,
    #[serde(default)]
    pub status: Option<KleerEventStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerEventList {
    #[serde(default)]
    pub event_readables: Vec<KleerEventReadable>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KleerPayrollEventType {
    #[serde(alias = "Sick")]
    Sick,
    #[serde(alias = "Vacation")]
    Vacation,
    #[serde(alias = "LeaveOfAbsence")]
    LeaveOfAbsence,
    #[serde(alias = "LeaveOfAbsenceVacationEarned")]
    LeaveOfAbsenceVacationEarned,
    #[serde(alias = "WorkHour")]
    WorkHour,
    #[serde(alias = "ParentalLeave")]
    ParentalLeave,
    #[serde(alias = "Childcare")]
    Childcare,
    #[serde(alias = "CloseRelativeCare")]
    CloseRelativeCare,
    #[serde(alias = "PaternityLeave")]
    PaternityLeave,
    #[serde(alias = "Furlough")]
    Furlough,
    #[serde(alias = "OtherLeave")]
    OtherLeave,
    #[serde(alias = "OtherLeaveVacationNotEarned")]
    OtherLeaveVacationNotEarned,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerPayrollEvent {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(with = "date_format")]
    pub date: Date,
    pub hours: f64,
    #[serde(rename = "type")]
    pub event_type: KleerPayrollEventType,
    #[serde(default)]
    pub child: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerPayrollEventList {
    #[serde(default)]
    pub payroll_events: Vec<KleerPayrollEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KleerEventRestrictionReadable {
    pub status: KleerEventStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerEventRestrictionList {
    #[serde(default)]
    pub event_restriction_readables: Vec<KleerEventRestrictionReadable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerEventWritable {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub foreign_id: String,
    pub user: KleerIdRef,
    pub activity: KleerIdRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_project: Option<KleerIdRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
    #[serde(with = "date_format")]
    pub date: Date,
    pub hours: f64,
    pub comment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KleerSavedId {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerScheduleMetadata {
    #[serde(with = "date_format")]
    pub date: Date,
    pub level_of_employment: f64,
    pub gross_hours: f64,
    pub net_hours: f64,
    pub actual_hours: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct KleerScheduleMetadataList {
    #[serde(default)]
    pub payroll_user_schedule_metadatas: Vec<KleerScheduleMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_event_writable_with_kleer_field_names() {
        let payload = KleerEventWritable {
            foreign_id: String::new(),
            user: KleerIdRef { id: 1 },
            activity: KleerIdRef { id: 2 },
            client_project: Some(KleerIdRef { id: 3 }),
            child: None,
            date: Date::from_calendar_date(2026, time::Month::April, 17).unwrap(),
            hours: 8.0,
            comment: "Worked on migration".to_string(),
            internal_comment: None,
        };

        let json = serde_json::to_string(&payload).unwrap();

        assert!(json.contains("\"client-project\":{\"id\":3}"));
        assert!(json.contains("\"date\":\"2026-04-17\""));
        assert!(!json.contains("foreign-id"));
    }

    #[test]
    fn deserializes_event_list_example_shape() {
        let raw = r#"{
            "event-readables": [
                {
                    "id": { "id": 4494670 },
                    "foreign-id": "",
                    "user": { "id": 31118 },
                    "activity": { "id": 22427 },
                    "client-project": { "id": 322222 },
                    "date": "2020-07-20",
                    "hours": 8,
                    "comment": "External comment",
                    "status": {
                        "type": "OPEN",
                        "registration-user": { "id": 5236 },
                        "registration-date": "2020-07-31"
                    }
                }
            ]
        }"#;

        let parsed: KleerEventList = serde_json::from_str(raw).unwrap();

        assert_eq!(parsed.event_readables.len(), 1);
        assert_eq!(parsed.event_readables[0].id.id, 4_494_670);
        assert_eq!(
            parsed.event_readables[0]
                .status
                .as_ref()
                .unwrap()
                .status_type,
            KleerStatusType::Open
        );
    }

    #[test]
    fn deserializes_payroll_event_list_example_shape() {
        let raw = r#"{
            "payroll-events": [
                {
                    "id": 4493036,
                    "date": "2020-07-13",
                    "hours": 8,
                    "type": "VACATION",
                    "comment": ""
                },
                {
                    "id": 4493056,
                    "date": "2020-07-29",
                    "hours": 8,
                    "type": "SICK",
                    "comment": ""
                },
                {
                    "id": 4493057,
                    "date": "2020-07-30",
                    "hours": 2,
                    "type": "WorkHour",
                    "comment": ""
                }
            ]
        }"#;

        let parsed: KleerPayrollEventList = serde_json::from_str(raw).unwrap();

        assert_eq!(parsed.payroll_events.len(), 3);
        assert_eq!(
            parsed.payroll_events[0].event_type,
            KleerPayrollEventType::Vacation
        );
        assert_eq!(
            parsed.payroll_events[1].event_type,
            KleerPayrollEventType::Sick
        );
        assert_eq!(
            parsed.payroll_events[2].event_type,
            KleerPayrollEventType::WorkHour
        );
    }

    #[test]
    fn deserializes_user_list_example_shape() {
        let raw = r#"{
            "users": [
                {
                    "id": 31118,
                    "foreign-id": "aad-user-id",
                    "internal-id": 123,
                    "name": "Ada Lovelace",
                    "email": "ada@example.com",
                    "active": true,
                    "dimension-entry": { "id": 1 },
                    "dimensions": {}
                }
            ]
        }"#;

        let parsed: KleerUserList = serde_json::from_str(raw).unwrap();

        assert_eq!(parsed.users.len(), 1);
        assert_eq!(parsed.users[0].id, 31_118);
        assert_eq!(parsed.users[0].foreign_id.as_deref(), Some("aad-user-id"));
        assert_eq!(parsed.users[0].internal_id.as_deref(), Some("123"));
        assert_eq!(parsed.users[0].email.as_deref(), Some("ada@example.com"));
    }
}
