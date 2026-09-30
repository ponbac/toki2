use super::{AiUsageTotals, MAX_SAFE_COUNT};
use crate::domain::AiUsageError;

impl AiUsageTotals {
    /// Keeps totals only when every count, their combined token count, and the
    /// known cost fit the report's numeric contract. This checks a read result;
    /// valid individual uploads need not have a representable aggregate.
    pub fn checked_for_report(self) -> Result<Self, AiUsageError> {
        let tokens = [
            self.tokens.input,
            self.tokens.cache_read,
            self.tokens.cache_write,
            self.tokens.output,
        ];
        let counts = [self.records, self.unpriced_buckets, self.unpriced_records];
        if tokens
            .iter()
            .chain(counts.iter())
            .any(|count| !(0..=MAX_SAFE_COUNT).contains(count))
            || tokens.iter().copied().map(i128::from).sum::<i128>() > i128::from(MAX_SAFE_COUNT)
            || !self.priced_cost_usd.is_finite()
            || self.priced_cost_usd < 0.0
        {
            return Err(AiUsageError::NumericRange);
        }
        Ok(self)
    }
}
