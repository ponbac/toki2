use time::UtcOffset;

pub fn to_local_time(dt: time::OffsetDateTime) -> time::OffsetDateTime {
    if let Ok(local_offset) = UtcOffset::current_local_offset() {
        dt.to_offset(local_offset)
    } else {
        dt
    }
}
