use std::sync::Arc;

use azure_devops_rust_api::Credential;
use azure_identity::DefaultAzureCredentialBuilder;

pub fn get_credential() -> Credential {
    // Get authentication credential either from a PAT ("ADO_TOKEN") or via the az cli
    match std::env::var("ADO_TOKEN") {
        Ok(token) if !token.is_empty() => {
            println!("Authenticate using PAT provided via $ADO_TOKEN");
            Credential::from_pat(token)
        }
        _ => authenticate_with_default_credential(),
    }
}

fn authenticate_with_default_credential() -> Credential {
    println!("Authenticate using auto-refreshing DefaultAzureCredential");
    // `DefaultAzureCredential` can authenticate using one of:
    // - `EnvironmentCredential`
    // - `ManagedIdentityCredential`
    // - `AzureCliCredential`
    // For examples we just want to use AzureCliCredential, so exclude the
    // other mechanisms.
    // It would be simpler to directly create `AzureCliCredential` here, but I want to
    // demonstrate use of `DefaultAzureCredentialBuilder`.
    let default_azure_credential = Arc::new(
        DefaultAzureCredentialBuilder::new()
            .exclude_environment_credential()
            .exclude_managed_identity_credential()
            .build(),
    );

    Credential::from_token_credential(default_azure_credential)
}
