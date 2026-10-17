use async_trait::async_trait;

use crate::domain::{
    models::{
        AiBillingMonth, BattleId, BattleView, BuddyBattle, BuddyBattleSummary, BuddyLeaderboard,
        BuddyOpponents, BuddyOverview, BuddyProfileUpdate, DailyTrainerStatus, UserId,
    },
    BuddyError,
};

/// Timer buddies: a user's buddy profile, the XP their buddies earn, and their battles
/// against colleagues' buddies and the daily trainer.
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

    /// The colleagues the user can battle and when they can start the next colleague
    /// battle. Needs an enrolled user (`NotEnrolled`) whose buddy is switched on
    /// (`Disabled`).
    async fn opponents(&self, user_id: &UserId) -> Result<BuddyOpponents, BuddyError>;

    /// Battles the colleague's buddies with the user's, and tells the colleague
    /// afterwards if they want to know.
    ///
    /// Both buddies must be switched on (`NotEnrolled`, `Disabled`,
    /// `OpponentUnavailable`), and a user cannot battle themselves (`Invalid`). Each
    /// colleague can be battled once a day (`BattledToday`), and colleague battles at
    /// most once per `COLLEAGUE_BATTLE_COOLDOWN` (`Cooldown`). Nothing is stored when a
    /// rule is broken.
    async fn battle_colleague(
        &self,
        user_id: &UserId,
        opponent_id: &UserId,
    ) -> Result<BattleView<BuddyBattle>, BuddyError>;

    /// Today's daily trainer as the user would meet it, and their battle against it if
    /// they fought it. Needs an enrolled user whose buddy is switched on.
    async fn trainer(&self, user_id: &UserId) -> Result<DailyTrainerStatus, BuddyError>;

    /// Battles today's daily trainer with the user's buddies, once a day
    /// (`TrainerAlreadyFought`) and apart from the colleague cooldown. Needs an enrolled
    /// user whose buddy is switched on.
    async fn battle_trainer(&self, user_id: &UserId)
        -> Result<BattleView<BuddyBattle>, BuddyError>;

    /// The battles the user started or defended in the last `RECENT_BATTLE_DAYS`,
    /// newest first, at most `RECENT_BATTLE_LIMIT`.
    async fn battles(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<BattleView<BuddyBattleSummary>>, BuddyError>;

    /// A battle the user took part in, with its replay. `BattleNotFound` for any other.
    async fn battle(
        &self,
        user_id: &UserId,
        battle_id: BattleId,
    ) -> Result<BattleView<BuddyBattle>, BuddyError>;

    /// Records that the user saw a battle they defended; does nothing for one they
    /// started. `BattleNotFound` when there is no such battle and `NotParticipant` when
    /// the user did not take part.
    async fn mark_battle_seen(
        &self,
        user_id: &UserId,
        battle_id: BattleId,
    ) -> Result<(), BuddyError>;

    /// The leaderboard of a local month, by default the current one.
    async fn leaderboard(
        &self,
        month: Option<AiBillingMonth>,
    ) -> Result<BuddyLeaderboard, BuddyError>;
}
