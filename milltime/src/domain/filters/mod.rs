mod activity_filter;
mod date_filter;
mod project_registration_filter;
mod project_search_filter;
mod timer_registration_filter;
mod update_timer_filter;

pub use activity_filter::ActivityFilter;
pub use date_filter::DateFilter;
pub use project_registration_filter::ProjectRegistrationFilter;
pub use project_search_filter::ProjectSearchFilter;
pub use timer_registration_filter::TimerRegistrationFilter;
pub use update_timer_filter::UpdateTimerFilter;

pub trait MilltimeFilter {
    fn as_milltime_filter(&self) -> String;
}
