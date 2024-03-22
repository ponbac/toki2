use std::str::FromStr;

use serde::Deserialize;
use serde_with::serde_as;
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use strum::{Display, EnumString};

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub database: DatabaseSettings,
    pub auth: AuthSettings,
}

#[serde_as]
#[derive(Deserialize, Clone)]
pub struct ApplicationSettings {
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub port: u16,
    pub host: String,
    pub app_url: String,
    pub disable_auth: bool,
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
