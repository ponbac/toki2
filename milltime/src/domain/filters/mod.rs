mod date_filter;
mod timer_registration_filter;

pub use date_filter::DateFilter;
pub use timer_registration_filter::TimerRegistrationFilter;

pub trait MilltimeFilter {
    fn as_milltime_filter(&self) -> String;
}
