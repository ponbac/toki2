use std::str::FromStr;

use serde::Deserialize;
use serde_with::serde_as;
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use strum::{Display, EnumString};
use time::OffsetDateTime;

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub auth: AuthSettings,
    pub kleer: KleerSettings,
    #[serde(default)]
    pub observability: ObservabilitySettings,
    #[serde(default)]
    pub ai_usage: AiUsageSettings,
    #[serde(default)]
    pub github: GithubSettings,
}

#[serde_as]
#[derive(Deserialize, Clone)]
pub struct ApplicationSettings {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub port: u16,
    pub host: String,
    pub app_url: String,
    pub api_url: String,
    pub disable_auth: bool,
    #[serde(default)]
    pub cors_allowed_origin_suffix: Option<String>,
}

#[serde_as]
#[derive(Deserialize, Clone, Debug)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

#[derive(Deserialize, Clone)]
pub struct AuthSettings {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_url: String,
}

#[derive(Deserialize, Clone)]
pub struct KleerSettings {
    pub token: Option<String>,
    pub company_id: Option<String>,
    #[serde(default = "default_kleer_base_url")]
    pub base_url: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ObservabilitySettings {
    #[serde(default)]
    pub capture_request_bodies: bool,
    #[serde(default = "default_request_body_max_logged_bytes")]
    pub request_body_max_logged_bytes: usize,
    #[serde(default = "default_request_body_max_buffered_bytes")]
    pub request_body_max_buffered_bytes: usize,
}

impl Default for ObservabilitySettings {
    fn default() -> Self {
        Self {
            capture_request_bodies: false,
            request_body_max_logged_bytes: default_request_body_max_logged_bytes(),
            request_body_max_buffered_bytes: default_request_body_max_buffered_bytes(),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct AiUsageSettings {
    /// IANA time zone that defines AI usage days and billing months.
    #[serde(default = "default_ai_usage_time_zone")]
    pub time_zone: String,
    #[serde(default)]
    pub exchange_rates: ExchangeRateSettings,
    /// The earliest hour a reprice-only upload may change, an RFC 3339
    /// instant. Stored costs before it stay as they are.
    #[serde(
        default = "default_ai_usage_reprice_floor",
        with = "time::serde::rfc3339"
    )]
    pub reprice_floor: OffsetDateTime,
}

impl Default for AiUsageSettings {
    fn default() -> Self {
        Self {
            time_zone: default_ai_usage_time_zone(),
            exchange_rates: ExchangeRateSettings::default(),
            reprice_floor: default_ai_usage_reprice_floor(),
        }
    }
}

/// Where billing fetches exchange rates to the billing currency: the
/// Riksbank's SWEA API.
#[derive(Deserialize, Clone, Debug)]
pub struct ExchangeRateSettings {
    #[serde(default = "default_exchange_rate_base_url")]
    pub base_url: String,
    /// An optional SWEA API subscription key, for higher rate limits. Provide
    /// it through `TOKI_AI_USAGE__EXCHANGE_RATES__SUBSCRIPTION_KEY`.
    #[serde(default)]
    pub subscription_key: Option<String>,
}

impl Default for ExchangeRateSettings {
    fn default() -> Self {
        Self {
            base_url: default_exchange_rate_base_url(),
            subscription_key: None,
        }
    }
}

/// The GitHub organization whose Copilot seats Toki reads, through a GitHub App
/// installed on it. Off unless the app's private key is set, through
/// `TOKI_GITHUB__APP_PRIVATE_KEY`.
#[serde_as]
#[derive(Deserialize, Clone)]
pub struct GithubSettings {
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default = "default_github_api_base")]
    pub api_base: String,
    /// The GitHub App's client ID, the issuer of the JWTs it signs.
    #[serde(default)]
    pub app_client_id: Option<String>,
    /// The app's installation on `organization`. A YAML integer and an
    /// environment string both parse.
    #[serde_as(as = "Option<serde_with::DisplayFromStr>")]
    #[serde(default)]
    pub installation_id: Option<u64>,
    /// The app's private key in PEM, multi-line or with literal `\n`.
    /// Provide it through the environment only.
    #[serde(default)]
    pub app_private_key: Option<String>,
}

impl Default for GithubSettings {
    fn default() -> Self {
        Self {
            organization: None,
            api_base: default_github_api_base(),
            app_client_id: None,
            installation_id: None,
            app_private_key: None,
        }
    }
}

impl GithubSettings {
    /// The GitHub App's credentials. `Ok(None)` without a private key: the
    /// account directory is off. An error when a key is set but the
    /// organization, client ID or installation ID is missing or blank. The
    /// key itself is parsed by the adapter.
    pub fn app_credentials(
        &self,
    ) -> Result<Option<crate::adapters::outbound::github::GithubAppCredentials>, String> {
        let Some(private_key_pem) = self
            .app_private_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
        else {
            return Ok(None);
        };
        let required = |value: &Option<String>, name: &str| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("github.{name} must be set with the app's private key"))
        };
        let organization = required(&self.organization, "organization")?;
        let client_id = required(&self.app_client_id, "app_client_id")?;
        let installation_id = self.installation_id.ok_or_else(|| {
            "github.installation_id must be set with the app's private key".to_string()
        })?;

        Ok(Some(
            crate::adapters::outbound::github::GithubAppCredentials {
                organization,
                api_base: self.api_base.trim().to_string(),
                client_id,
                installation_id,
                private_key_pem: private_key_pem.to_string(),
            },
        ))
    }
}

fn default_github_api_base() -> String {
    crate::adapters::outbound::github::DEFAULT_API_BASE.to_string()
}

fn default_exchange_rate_base_url() -> String {
    crate::adapters::outbound::riksbank::DEFAULT_BASE_URL.to_string()
}

fn default_ai_usage_time_zone() -> String {
    "Europe/Stockholm".to_string()
}

fn default_ai_usage_reprice_floor() -> OffsetDateTime {
    time::macros::datetime!(2026-10-01 00:00 UTC)
}

fn default_request_body_max_logged_bytes() -> usize {
    16_384
}

fn default_request_body_max_buffered_bytes() -> usize {
    65_536
}

impl KleerSettings {
    pub fn credentials(&self) -> Result<kleer::KleerCredentials, String> {
        let token = self
            .token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing Kleer integration token".to_string())?;
        let company_id = self
            .company_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing Kleer company id".to_string())?;
        let base_url = self.base_url.trim();

        Ok(kleer::KleerCredentials::new(
            token,
            company_id,
            Some(base_url),
        ))
    }
}

fn default_kleer_base_url() -> String {
    kleer::DEFAULT_BASE_URL.to_string()
}

impl DatabaseSettings {
    pub fn without_db(&self) -> PgConnectOptions {
        let ssl_mode = if self.require_ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };

        PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .username(&self.username)
            .password(&self.password)
            .ssl_mode(ssl_mode)
    }

    pub fn with_db(&self) -> PgConnectOptions {
        self.without_db().database(&self.database_name)
    }
}

pub fn read_config() -> Result<Settings, config::ConfigError> {
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let config_directory = base_path.join("config");

    let environment = Environment::from_str(
        std::env::var("APP_ENVIRONMENT")
            .unwrap_or_else(|_| "local".into())
            .as_str(),
    )
    .expect("Failed to parse APP_ENVIRONMENT");
    let environment_filename = format!("{}.yaml", environment);

    let settings = config::Config::builder()
        .add_source(config::File::from(config_directory.join("base.yaml")))
        .add_source(config::File::from(
            config_directory.join(environment_filename),
        ))
        .add_source(
            config::Environment::with_prefix("TOKI")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()?;

    settings.try_deserialize::<Settings>()
}

#[derive(Display, Debug, EnumString)]
pub enum Environment {
    #[strum(ascii_case_insensitive, serialize = "local")]
    Local,
    #[strum(ascii_case_insensitive, serialize = "production")]
    Production,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use time::macros::datetime;

    use super::*;

    #[test]
    fn base_config_sets_the_reprice_floor() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("config/base.yaml");
        let settings = config::Config::builder()
            .add_source(config::File::from(base))
            .build()
            .unwrap();

        let ai_usage: AiUsageSettings = settings.get("ai_usage").unwrap();

        assert_eq!(ai_usage.reprice_floor, datetime!(2026-10-01 00:00 UTC));
    }

    fn base_config() -> config::Config {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("config/base.yaml");
        config::Config::builder()
            .add_source(config::File::from(base))
            .build()
            .unwrap()
    }

    #[test]
    fn base_config_names_the_github_app_but_leaves_it_off_without_a_key() {
        let github: GithubSettings = base_config().get("github").unwrap();

        assert_eq!(github.organization.as_deref(), Some("Spinit-AB"));
        assert_eq!(
            github.app_client_id.as_deref(),
            Some("Iv23liEuEJDWE4ifGVp8")
        );
        assert_eq!(github.installation_id, Some(174922684));
        assert_eq!(github.app_credentials().unwrap(), None);
    }

    #[test]
    fn a_github_key_needs_the_app_and_its_installation() {
        let mut github: GithubSettings = base_config().get("github").unwrap();
        github.app_private_key = Some("-----BEGIN RSA PRIVATE KEY-----".to_string());
        let credentials = github.app_credentials().unwrap().unwrap();
        assert_eq!(credentials.organization, "Spinit-AB");
        assert_eq!(credentials.installation_id, 174922684);

        let mut without_client = github.clone();
        without_client.app_client_id = Some(" ".to_string());
        assert!(without_client
            .app_credentials()
            .unwrap_err()
            .contains("app_client_id"));
        let mut without_installation = github.clone();
        without_installation.installation_id = None;
        assert!(without_installation
            .app_credentials()
            .unwrap_err()
            .contains("installation_id"));
        let mut without_organization = github;
        without_organization.organization = None;
        assert!(without_organization
            .app_credentials()
            .unwrap_err()
            .contains("organization"));
    }

    #[test]
    fn a_missing_github_block_defaults_to_the_public_api() {
        let settings = config::Config::builder()
            .set_override("github.installation_id", "42")
            .unwrap()
            .build()
            .unwrap();
        let github: GithubSettings = settings.get("github").unwrap();
        assert_eq!(github.installation_id, Some(42));
        assert_eq!(
            github.api_base,
            crate::adapters::outbound::github::DEFAULT_API_BASE
        );

        let default = GithubSettings::default();
        assert_eq!(
            default.api_base,
            crate::adapters::outbound::github::DEFAULT_API_BASE
        );
        assert_eq!(default.app_credentials().unwrap(), None);
    }
}
