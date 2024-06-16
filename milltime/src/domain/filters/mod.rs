mod activity_filter;
mod date_filter;
mod project_search_filter;
mod timer_registration_filter;

pub use activity_filter::ActivityFilter;
pub use date_filter::DateFilter;
pub use project_search_filter::ProjectSearchFilter;
pub use timer_registration_filter::TimerRegistrationFilter;

pub trait MilltimeFilter {
    fn as_milltime_filter(&self) -> String;
}
