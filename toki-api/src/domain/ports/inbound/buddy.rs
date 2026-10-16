use async_trait::async_trait;

use crate::domain::{
    models::{BuddyOverview, BuddyProfileUpdate, UserId},
    BuddyError,
};

/// Timer buddies: a user's buddy profile and the XP their buddies earn.
#[async_trait]
pub trait BuddyService: Send + Sync + 'static {
    /// The user's buddy profile with their buddies' levels, or `None` before they
    /// enrol. Reads the user's recent logged hours from time tracking first while
    /// their buddy is switched on, at most once per `HOURS_SYNC_INTERVAL` whether or
    /// not the read works; when it fails, the stored hours are used.
    async fn get_profile(&self, user_id: &UserId) -> Result<Option<BuddyOverview>, BuddyError>;

    /// Applies `update` to the user's profile, enrolling them on the first update.
    ///
    /// Enrolment needs `enabled` and `lead_species_id` (`Invalid` otherwise) and starts
    /// counting logged hours from today. A partner species creates the partner if
    /// there is none yet, which needs a lead of at least `PARTNER_UNLOCK_LEVEL`
    /// (`PartnerLocked`), or changes its species. `partner_visible` needs a partner
    /// (`NoPartner`). Species must be buddy species (`UnknownSpecies`). Nothing
    /// changes when a rule is broken.
    async fn update_profile(
        &self,
        user_id: &UserId,
        update: BuddyProfileUpdate,
    ) -> Result<BuddyOverview, BuddyError>;
}
