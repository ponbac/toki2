use std::sync::Arc;

use async_trait::async_trait;
use rand::Rng;
use time::OffsetDateTime;
use url::Url;

use crate::domain::{
    models::{
        is_valid_verifier, parse_client_name, parse_redirect_uris, AuthorizationErrorCode,
        AuthorizationOutcome, AuthorizationParams, AuthorizationResponse, ClientIdUrl,
        ClientRegistration, CodeDecision, CodeExchange, CodeRedemption, IssuedTokens,
        NewAuthorizationCode, NewAuthorizationRequest, NewRegisteredClient, NewToken, NewTokenPair,
        OAuthClient, OAuthGrant, OAuthGrantId, OAuthSecret, OAuthSecretKind, PkceChallenge,
        RefreshDecision, RefreshRotation, RegisteredClient, StoredAuthorizationCode,
        StoredRefreshToken, TokenRefresh, UserId, ValidatedAuthorization, ACCESS_TOKEN_LIFETIME,
        AUTHORIZATION_CODE_LIFETIME, AUTHORIZATION_REQUEST_LIFETIME, GRANT_LIFETIME,
        REFRESH_REUSE_GRACE, REFRESH_TOKEN_IDLE_LIFETIME,
    },
    ports::{
        inbound::{OAuthAccessTokenAuthenticator, OAuthService},
        outbound::{ClientMetadataResolver, OAuthRepository},
    },
    AuthorizationError, OAuthError, UserPrincipal,
};

const SUPPORTED_GRANT_TYPES: &[&str] = &["authorization_code", "refresh_token"];
const SUPPORTED_RESPONSE_TYPES: &[&str] = &["code"];
const REGISTERED_CLIENT_ID_BYTES: usize = 16;

/// The authorization server behind `/oauth/*`, issuing tokens for one
/// resource: the MCP endpoint.
pub struct OAuthServiceImpl<R, F> {
    repository: Arc<R>,
    resolver: Arc<F>,
    /// The canonical URL of the MCP endpoint, `{api_url}/mcp`.
    resource: String,
}

impl<R, F> OAuthServiceImpl<R, F> {
    pub fn new(repository: Arc<R>, resolver: Arc<F>, resource: String) -> Self {
        Self {
            repository,
            resolver,
            resource,
        }
    }
}

impl<R: OAuthRepository, F: ClientMetadataResolver> OAuthServiceImpl<R, F> {
    /// A metadata-document client for an HTTPS client_id, otherwise a
    /// registered one.
    async fn resolve_client(&self, client_id: Option<&str>) -> Result<OAuthClient, OAuthError> {
        let client_id = client_id
            .filter(|client_id| !client_id.is_empty())
            .ok_or_else(|| OAuthError::InvalidRequest("client_id is required.".to_string()))?;
        if client_id.starts_with("https://") {
            let url = ClientIdUrl::parse(client_id).ok_or_else(|| {
                OAuthError::InvalidClient("The client_id is not a valid metadata URL.".to_string())
            })?;
            let document = self.resolver.resolve(&url).await?;
            return OAuthClient::from_metadata_document(&url, &document);
        }

        self.repository
            .find_registered_client(client_id)
            .await?
            .ok_or_else(|| OAuthError::InvalidClient("The client is not registered.".to_string()))
    }
}

/// Whether a `resource` a client sent names `canonical`, comparing parsed URLs
/// so that scheme and host case and default ports do not matter.
fn same_resource(presented: &str, canonical: &str) -> bool {
    match (Url::parse(presented), Url::parse(canonical)) {
        (Ok(presented), Ok(canonical)) => presented.as_str() == canonical.as_str(),
        _ => false,
    }
}

fn new_token_pair(now: OffsetDateTime) -> (OAuthSecret, OAuthSecret, NewTokenPair) {
    let access = OAuthSecret::generate(OAuthSecretKind::AccessToken);
    let refresh = OAuthSecret::generate(OAuthSecretKind::RefreshToken);
    let tokens = NewTokenPair {
        access: NewToken {
            hash: access.hash(),
            expires_at: now + ACCESS_TOKEN_LIFETIME,
        },
        refresh: NewToken {
            hash: refresh.hash(),
            expires_at: now + REFRESH_TOKEN_IDLE_LIFETIME,
        },
    };
    (access, refresh, tokens)
}

fn generate_client_id() -> String {
    let mut bytes = [0u8; REGISTERED_CLIENT_ID_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn missing_client_id() -> OAuthError {
    OAuthError::InvalidClient("client_id is required.".to_string())
}

#[async_trait]
impl<R: OAuthRepository, F: ClientMetadataResolver> OAuthService for OAuthServiceImpl<R, F> {
    async fn validate_authorization(
        &self,
        params: AuthorizationParams,
    ) -> Result<ValidatedAuthorization, AuthorizationError> {
        let client = self
            .resolve_client(params.client_id.as_deref())
            .await
            .map_err(AuthorizationError::Unredirectable)?;
        let presented = params.redirect_uri.ok_or_else(|| {
            AuthorizationError::Unredirectable(OAuthError::InvalidRequest(
                "redirect_uri is required.".to_string(),
            ))
        })?;
        let registered_redirect_uri =
            client
                .redirect_uri_for(&presented)
                .cloned()
                .ok_or_else(|| {
                    AuthorizationError::Unredirectable(OAuthError::InvalidRequest(
                        "redirect_uri is not one the client registered.".to_string(),
                    ))
                })?;

        // From here on the redirect URI is trusted, so errors go back to it.
        let state = params.state;
        let redirect_error = |code: AuthorizationErrorCode, description: &str| {
            AuthorizationError::Redirect(AuthorizationResponse {
                redirect_uri: presented.clone(),
                state: state.clone(),
                outcome: AuthorizationOutcome::Error {
                    code,
                    description: description.to_string(),
                },
            })
        };
        if params.response_type.as_deref() != Some("code") {
            return Err(redirect_error(
                AuthorizationErrorCode::UnsupportedResponseType,
                "response_type must be code.",
            ));
        }
        if params.code_challenge_method.as_deref() != Some("S256") {
            return Err(redirect_error(
                AuthorizationErrorCode::InvalidRequest,
                "code_challenge_method must be S256.",
            ));
        }
        let code_challenge = params
            .code_challenge
            .as_deref()
            .and_then(PkceChallenge::parse_s256)
            .ok_or_else(|| {
                redirect_error(
                    AuthorizationErrorCode::InvalidRequest,
                    "code_challenge must be an S256 challenge.",
                )
            })?;
        if let Some(resource) = params.resource.as_deref() {
            if !same_resource(resource, &self.resource) {
                return Err(redirect_error(
                    AuthorizationErrorCode::InvalidTarget,
                    "resource must be this server's MCP endpoint.",
                ));
            }
        }

        Ok(ValidatedAuthorization {
            client,
            registered_redirect_uri,
            redirect_uri: presented,
            code_challenge,
            resource: self.resource.clone(),
            state,
        })
    }

    async fn begin_consent(
        &self,
        user_id: &UserId,
        authorization: &ValidatedAuthorization,
    ) -> Result<OAuthSecret, OAuthError> {
        let handle = OAuthSecret::generate(OAuthSecretKind::AuthorizationRequest);
        let request = NewAuthorizationRequest {
            id: handle.hash(),
            user_id: *user_id,
            client_id: authorization.client.id.clone(),
            client_name: authorization.client.name.clone(),
            redirect_uri: authorization.redirect_uri.clone(),
            resource: authorization.resource.clone(),
            code_challenge: authorization.code_challenge.clone(),
            state: authorization.state.clone(),
            expires_at: OffsetDateTime::now_utc() + AUTHORIZATION_REQUEST_LIFETIME,
        };
        self.repository
            .insert_authorization_request(&request)
            .await?;

        Ok(handle)
    }

    async fn decide(
        &self,
        user_id: &UserId,
        request: &str,
        allow: bool,
    ) -> Result<AuthorizationResponse, OAuthError> {
        let request = OAuthSecret::parse(OAuthSecretKind::AuthorizationRequest, request)
            .ok_or(OAuthError::NotFound)?;

        if !allow {
            let decided = self
                .repository
                .take_authorization_request(&request.hash(), user_id, None)
                .await?
                .ok_or(OAuthError::NotFound)?;
            return Ok(AuthorizationResponse {
                redirect_uri: decided.redirect_uri,
                state: decided.state,
                outcome: AuthorizationOutcome::Error {
                    code: AuthorizationErrorCode::AccessDenied,
                    description: "The user denied the request.".to_string(),
                },
            });
        }

        let code = OAuthSecret::generate(OAuthSecretKind::AuthorizationCode);
        let new_code = NewAuthorizationCode {
            hash: code.hash(),
            expires_at: OffsetDateTime::now_utc() + AUTHORIZATION_CODE_LIFETIME,
        };
        let decided = self
            .repository
            .take_authorization_request(&request.hash(), user_id, Some(&new_code))
            .await?
            .ok_or(OAuthError::NotFound)?;

        Ok(AuthorizationResponse {
            redirect_uri: decided.redirect_uri,
            state: decided.state,
            outcome: AuthorizationOutcome::Code(code),
        })
    }

    async fn exchange_code(&self, exchange: CodeExchange) -> Result<IssuedTokens, OAuthError> {
        if exchange.client_id.is_empty() {
            return Err(missing_client_id());
        }
        let code = OAuthSecret::parse(OAuthSecretKind::AuthorizationCode, &exchange.code)
            .ok_or_else(|| {
                OAuthError::InvalidGrant("The authorization code is invalid.".to_string())
            })?;
        if !is_valid_verifier(&exchange.code_verifier) {
            return Err(OAuthError::InvalidRequest(
                "code_verifier must be 43 to 128 unreserved characters.".to_string(),
            ));
        }

        let now = OffsetDateTime::now_utc();
        let (access_token, refresh_token, tokens) = new_token_pair(now);
        let CodeExchange {
            redirect_uri,
            client_id,
            code_verifier,
            resource: requested_resource,
            ..
        } = exchange;
        let judge = move |stored: &StoredAuthorizationCode| {
            let reject = |reason: &str| CodeDecision::Reject(reason.to_string());
            if stored.consumed {
                return CodeDecision::RevokeGrant(
                    "The authorization code was already used; its connection is revoked."
                        .to_string(),
                );
            }
            if stored.expires_at <= OffsetDateTime::now_utc() {
                return reject("The authorization code has expired.");
            }
            if stored.client_id != client_id {
                return reject("The authorization code was issued to another client.");
            }
            if stored.redirect_uri != redirect_uri {
                return reject("redirect_uri does not match the authorization request.");
            }
            if requested_resource
                .as_deref()
                .is_some_and(|requested| !same_resource(requested, &stored.resource))
            {
                return reject("resource does not match the authorization request.");
            }
            if !stored.code_challenge.verify(&code_verifier) {
                return reject("code_verifier does not match the code_challenge.");
            }
            CodeDecision::Redeem
        };

        match self
            .repository
            .redeem_authorization_code(&code.hash(), &judge, now + GRANT_LIFETIME, &tokens)
            .await?
        {
            CodeRedemption::Redeemed => Ok(IssuedTokens {
                access_token,
                refresh_token,
                expires_in: ACCESS_TOKEN_LIFETIME,
            }),
            CodeRedemption::Unknown => Err(OAuthError::InvalidGrant(
                "The authorization code is invalid.".to_string(),
            )),
            CodeRedemption::Rejected(reason) => Err(OAuthError::InvalidGrant(reason)),
        }
    }

    async fn refresh(&self, refresh: TokenRefresh) -> Result<IssuedTokens, OAuthError> {
        if refresh.client_id.is_empty() {
            return Err(missing_client_id());
        }
        let presented = OAuthSecret::parse(OAuthSecretKind::RefreshToken, &refresh.refresh_token)
            .ok_or_else(|| {
            OAuthError::InvalidGrant("The refresh token is invalid.".to_string())
        })?;

        let now = OffsetDateTime::now_utc();
        let (access_token, refresh_token, tokens) = new_token_pair(now);
        let TokenRefresh {
            client_id,
            resource: requested_resource,
            ..
        } = refresh;
        let judge = move |stored: &StoredRefreshToken| {
            let now = OffsetDateTime::now_utc();
            let reject = |reason: &str| RefreshDecision::Reject(reason.to_string());
            if stored.grant_revoked || stored.grant_expires_at <= now {
                return reject("The connection is expired or revoked.");
            }
            if stored.client_id != client_id {
                return reject("The refresh token was issued to another client.");
            }
            if requested_resource
                .as_deref()
                .is_some_and(|requested| !same_resource(requested, &stored.resource))
            {
                return reject("resource does not match the refresh token's.");
            }
            // Reuse is judged before the token's own expiry, so a leaked
            // ancestor still revokes the connection after it expires.
            match stored.rotated_at {
                // Most likely the same client refreshing twice at once: its
                // other request got the successor, which stays valid.
                Some(rotated_at) if now - rotated_at < REFRESH_REUSE_GRACE => {
                    reject("The refresh token was just used; use its successor.")
                }
                Some(_) => RefreshDecision::RevokeGrant(
                    "The refresh token was already used; its connection is revoked.".to_string(),
                ),
                None if stored.expires_at <= now => reject("The refresh token has expired."),
                None => RefreshDecision::Rotate,
            }
        };

        match self
            .repository
            .rotate_refresh_token(&presented.hash(), &judge, &tokens)
            .await?
        {
            RefreshRotation::Rotated => Ok(IssuedTokens {
                access_token,
                refresh_token,
                expires_in: ACCESS_TOKEN_LIFETIME,
            }),
            RefreshRotation::Unknown => Err(OAuthError::InvalidGrant(
                "The refresh token is invalid.".to_string(),
            )),
            RefreshRotation::Rejected(reason) => Err(OAuthError::InvalidGrant(reason)),
        }
    }

    async fn revoke_token(&self, token: &str, client_id: Option<&str>) -> Result<(), OAuthError> {
        let client_id = client_id
            .filter(|client_id| !client_id.is_empty())
            .ok_or_else(missing_client_id)?;
        let Some(secret) = OAuthSecret::parse(OAuthSecretKind::AccessToken, token)
            .or_else(|| OAuthSecret::parse(OAuthSecretKind::RefreshToken, token))
        else {
            return Ok(());
        };

        self.repository
            .revoke_grant_of_token(&secret.hash(), Some(client_id))
            .await?;
        Ok(())
    }

    async fn register_client(
        &self,
        registration: ClientRegistration,
    ) -> Result<RegisteredClient, OAuthError> {
        let redirect_uris = registration
            .redirect_uris
            .as_deref()
            .and_then(parse_redirect_uris)
            .ok_or_else(|| {
                OAuthError::InvalidRedirectUri(
                    "redirect_uris must list 1 to 10 redirect URIs, each https, http on a \
                     loopback host, or a native app's private-use scheme."
                        .to_string(),
                )
            })?;
        let invalid = |reason: &str| OAuthError::InvalidClientMetadata(reason.to_string());
        let all_supported = |values: &[String], supported: &[&str]| {
            values
                .iter()
                .all(|value| supported.contains(&value.as_str()))
        };
        if let Some(grant_types) = &registration.grant_types {
            if !all_supported(grant_types, SUPPORTED_GRANT_TYPES) {
                return Err(invalid(
                    "grant_types may contain only authorization_code and refresh_token.",
                ));
            }
        }
        if let Some(response_types) = &registration.response_types {
            if !all_supported(response_types, SUPPORTED_RESPONSE_TYPES) {
                return Err(invalid("response_types may contain only code."));
            }
        }
        if !matches!(
            registration.token_endpoint_auth_method.as_deref(),
            None | Some("none")
        ) {
            return Err(invalid(
                "token_endpoint_auth_method must be none: Toki registers public clients only.",
            ));
        }
        let name = match registration.client_name.as_deref() {
            Some(name) => parse_client_name(name).ok_or_else(|| {
                invalid("client_name must be 1 to 100 characters without control characters.")
            })?,
            None => redirect_uris[0].display_destination(),
        };

        self.repository
            .insert_registered_client(&NewRegisteredClient {
                id: generate_client_id(),
                name,
                redirect_uris,
            })
            .await
    }

    async fn list_grants(&self, user_id: &UserId) -> Result<Vec<OAuthGrant>, OAuthError> {
        self.repository.list_active_grants(user_id).await
    }

    async fn revoke_grant(
        &self,
        user_id: &UserId,
        grant_id: &OAuthGrantId,
    ) -> Result<(), OAuthError> {
        self.repository.revoke_grant(user_id, grant_id).await
    }

    async fn remove_expired(&self) -> Result<(), OAuthError> {
        self.repository.delete_expired().await
    }
}

#[async_trait]
impl<R: OAuthRepository, F: ClientMetadataResolver> OAuthAccessTokenAuthenticator
    for OAuthServiceImpl<R, F>
{
    async fn authenticate(&self, presented: &str) -> Result<Option<UserPrincipal>, OAuthError> {
        let Some(secret) = OAuthSecret::parse(OAuthSecretKind::AccessToken, presented) else {
            return Ok(None);
        };

        Ok(self
            .repository
            .find_access_token(&secret.hash())
            .await?
            .filter(|grant| grant.resource == self.resource)
            .map(|grant| grant.principal))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fmt, sync::Mutex};

    use time::Duration;

    use crate::domain::{
        models::{
            AccessTokenGrant, ClientMetadataDocument, DecidedAuthorizationRequest, OAuthClientKind,
            OAuthSecretHash,
        },
        ports::outbound::{CodeJudge, RefreshJudge},
        Role,
    };

    use super::*;

    const RESOURCE: &str = "https://toki.example/mcp";
    const CLIENT_ID: &str = "https://claude.ai/oauth/claude-code-client-metadata";
    const OTHER_CLIENT_ID: &str = "https://vscode.dev/oauth/client-metadata.json";
    /// A loopback port of Claude Code's registered `http://localhost/callback`.
    const REDIRECT_URI: &str = "http://localhost:53682/callback";
    /// RFC 7636, appendix B.
    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
    const STATE: &str = "af0ifjsldkj";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TokenKind {
        Access,
        Refresh,
    }

    struct ClientRow {
        client: OAuthClient,
        created_at: OffsetDateTime,
        used: bool,
    }

    struct CodeRow {
        hash: OAuthSecretHash,
        code: StoredAuthorizationCode,
        grant_id: Option<OAuthGrantId>,
    }

    struct GrantRow {
        id: OAuthGrantId,
        user_id: UserId,
        client_id: String,
        client_name: String,
        redirect_uri: String,
        resource: String,
        created_at: OffsetDateTime,
        last_used_at: OffsetDateTime,
        expires_at: OffsetDateTime,
        revoked_at: Option<OffsetDateTime>,
    }

    impl GrantRow {
        fn is_active(&self, now: OffsetDateTime) -> bool {
            self.revoked_at.is_none() && self.expires_at > now
        }

        /// Whether this revoked it, rather than an earlier revocation.
        fn revoke(&mut self, now: OffsetDateTime) -> bool {
            let unrevoked = self.revoked_at.is_none();
            self.revoked_at.get_or_insert(now);
            unrevoked
        }
    }

    struct TokenRow {
        hash: OAuthSecretHash,
        grant_id: OAuthGrantId,
        kind: TokenKind,
        expires_at: OffsetDateTime,
        rotated_at: Option<OffsetDateTime>,
    }

    /// The tables. One lock around all of them stands in for a transaction.
    #[derive(Default)]
    struct Tables {
        clients: Vec<ClientRow>,
        requests: Vec<NewAuthorizationRequest>,
        codes: Vec<CodeRow>,
        grants: Vec<GrantRow>,
        tokens: Vec<TokenRow>,
        next_grant_id: i32,
    }

    fn push_tokens(
        tokens: &mut Vec<TokenRow>,
        grant_id: OAuthGrantId,
        pair: &NewTokenPair,
        grant_expires_at: OffsetDateTime,
    ) {
        tokens.push(TokenRow {
            hash: pair.access.hash,
            grant_id,
            kind: TokenKind::Access,
            expires_at: pair.access.expires_at,
            rotated_at: None,
        });
        tokens.push(TokenRow {
            hash: pair.refresh.hash,
            grant_id,
            kind: TokenKind::Refresh,
            expires_at: pair.refresh.expires_at.min(grant_expires_at),
            rotated_at: None,
        });
    }

    struct MemoryOAuth {
        tables: Mutex<Tables>,
        users: HashMap<UserId, UserPrincipal>,
    }

    impl MemoryOAuth {
        fn with_users(users: &[UserPrincipal]) -> Self {
            Self {
                tables: Mutex::new(Tables::default()),
                users: users.iter().map(|user| (user.id, user.clone())).collect(),
            }
        }

        fn update_token(&self, token: &OAuthSecret, update: impl FnOnce(&mut TokenRow)) {
            let mut tables = self.tables.lock().unwrap();
            let row = tables
                .tokens
                .iter_mut()
                .find(|row| row.hash == token.hash())
                .expect("token is stored");
            update(row);
        }

        fn expire_codes(&self) {
            let past = OffsetDateTime::now_utc() - Duration::seconds(1);
            for row in &mut self.tables.lock().unwrap().codes {
                row.code.expires_at = past;
            }
        }

        fn expire_requests(&self) {
            let past = OffsetDateTime::now_utc() - Duration::seconds(1);
            for request in &mut self.tables.lock().unwrap().requests {
                request.expires_at = past;
            }
        }

        fn expire_grants(&self) {
            let past = OffsetDateTime::now_utc() - Duration::seconds(1);
            for grant in &mut self.tables.lock().unwrap().grants {
                grant.expires_at = past;
            }
        }

        fn grant_count(&self) -> usize {
            self.tables.lock().unwrap().grants.len()
        }

        fn code_count(&self) -> usize {
            self.tables.lock().unwrap().codes.len()
        }

        fn client_used(&self, client_id: &str) -> bool {
            self.tables
                .lock()
                .unwrap()
                .clients
                .iter()
                .any(|row| row.client.id == client_id && row.used)
        }
    }

    #[async_trait]
    impl OAuthRepository for MemoryOAuth {
        async fn find_registered_client(
            &self,
            client_id: &str,
        ) -> Result<Option<OAuthClient>, OAuthError> {
            Ok(self
                .tables
                .lock()
                .unwrap()
                .clients
                .iter()
                .find(|row| row.client.id == client_id)
                .map(|row| row.client.clone()))
        }

        async fn insert_registered_client(
            &self,
            client: &NewRegisteredClient,
        ) -> Result<RegisteredClient, OAuthError> {
            let registered = RegisteredClient {
                client: OAuthClient {
                    id: client.id.clone(),
                    name: client.name.clone(),
                    kind: OAuthClientKind::Registered,
                    redirect_uris: client.redirect_uris.clone(),
                },
                created_at: OffsetDateTime::now_utc(),
            };
            self.tables.lock().unwrap().clients.push(ClientRow {
                client: registered.client.clone(),
                created_at: registered.created_at,
                used: false,
            });
            Ok(registered)
        }

        async fn insert_authorization_request(
            &self,
            request: &NewAuthorizationRequest,
        ) -> Result<(), OAuthError> {
            self.tables.lock().unwrap().requests.push(request.clone());
            Ok(())
        }

        async fn take_authorization_request(
            &self,
            id: &OAuthSecretHash,
            user_id: &UserId,
            code: Option<&NewAuthorizationCode>,
        ) -> Result<Option<DecidedAuthorizationRequest>, OAuthError> {
            let now = OffsetDateTime::now_utc();
            let mut tables = self.tables.lock().unwrap();
            let Some(index) = tables.requests.iter().position(|request| {
                request.id == *id && request.user_id == *user_id && request.expires_at > now
            }) else {
                return Ok(None);
            };
            let request = tables.requests.remove(index);
            if let Some(code) = code {
                tables.codes.push(CodeRow {
                    hash: code.hash,
                    code: StoredAuthorizationCode {
                        user_id: request.user_id,
                        client_id: request.client_id.clone(),
                        client_name: request.client_name.clone(),
                        redirect_uri: request.redirect_uri.clone(),
                        resource: request.resource.clone(),
                        code_challenge: request.code_challenge.clone(),
                        expires_at: code.expires_at,
                        consumed: false,
                    },
                    grant_id: None,
                });
            }
            Ok(Some(DecidedAuthorizationRequest {
                redirect_uri: request.redirect_uri,
                state: request.state,
            }))
        }

        async fn redeem_authorization_code(
            &self,
            code: &OAuthSecretHash,
            judge: &CodeJudge,
            grant_expires_at: OffsetDateTime,
            tokens: &NewTokenPair,
        ) -> Result<CodeRedemption, OAuthError> {
            let now = OffsetDateTime::now_utc();
            let mut tables = self.tables.lock().unwrap();
            let tables = &mut *tables;
            let Some(row) = tables.codes.iter_mut().find(|row| row.hash == *code) else {
                return Ok(CodeRedemption::Unknown);
            };
            match judge(&row.code) {
                CodeDecision::Redeem => {
                    row.code.consumed = true;
                    tables.next_grant_id += 1;
                    let grant_id = OAuthGrantId::new(tables.next_grant_id);
                    row.grant_id = Some(grant_id);
                    tables.grants.push(GrantRow {
                        id: grant_id,
                        user_id: row.code.user_id,
                        client_id: row.code.client_id.clone(),
                        client_name: row.code.client_name.clone(),
                        redirect_uri: row.code.redirect_uri.clone(),
                        resource: row.code.resource.clone(),
                        created_at: now,
                        last_used_at: now,
                        expires_at: grant_expires_at,
                        revoked_at: None,
                    });
                    if let Some(client) = tables
                        .clients
                        .iter_mut()
                        .find(|client| client.client.id == row.code.client_id)
                    {
                        client.used = true;
                    }
                    push_tokens(&mut tables.tokens, grant_id, tokens, grant_expires_at);
                    Ok(CodeRedemption::Redeemed)
                }
                CodeDecision::Reject(reason) => {
                    row.code.consumed = true;
                    Ok(CodeRedemption::Rejected(reason))
                }
                CodeDecision::RevokeGrant(reason) => {
                    if let Some(grant) = tables
                        .grants
                        .iter_mut()
                        .find(|grant| Some(grant.id) == row.grant_id)
                    {
                        grant.revoke(now);
                    }
                    Ok(CodeRedemption::Rejected(reason))
                }
            }
        }

        async fn rotate_refresh_token(
            &self,
            token: &OAuthSecretHash,
            judge: &RefreshJudge,
            tokens: &NewTokenPair,
        ) -> Result<RefreshRotation, OAuthError> {
            let now = OffsetDateTime::now_utc();
            let mut tables = self.tables.lock().unwrap();
            let tables = &mut *tables;
            let Some(row) = tables
                .tokens
                .iter_mut()
                .find(|row| row.hash == *token && row.kind == TokenKind::Refresh)
            else {
                return Ok(RefreshRotation::Unknown);
            };
            let grant = tables
                .grants
                .iter_mut()
                .find(|grant| grant.id == row.grant_id)
                .expect("a token's grant exists");
            let stored = StoredRefreshToken {
                client_id: grant.client_id.clone(),
                resource: grant.resource.clone(),
                expires_at: row.expires_at,
                rotated_at: row.rotated_at,
                grant_expires_at: grant.expires_at,
                grant_revoked: grant.revoked_at.is_some(),
            };
            match judge(&stored) {
                RefreshDecision::Rotate => {
                    row.rotated_at = Some(now);
                    grant.last_used_at = now;
                    let (grant_id, grant_expires_at) = (grant.id, grant.expires_at);
                    push_tokens(&mut tables.tokens, grant_id, tokens, grant_expires_at);
                    Ok(RefreshRotation::Rotated)
                }
                RefreshDecision::Reject(reason) => Ok(RefreshRotation::Rejected(reason)),
                RefreshDecision::RevokeGrant(reason) => {
                    grant.revoke(now);
                    Ok(RefreshRotation::Rejected(reason))
                }
            }
        }

        async fn find_access_token(
            &self,
            token: &OAuthSecretHash,
        ) -> Result<Option<AccessTokenGrant>, OAuthError> {
            let now = OffsetDateTime::now_utc();
            let tables = self.tables.lock().unwrap();
            let Some(row) = tables.tokens.iter().find(|row| {
                row.hash == *token && row.kind == TokenKind::Access && row.expires_at > now
            }) else {
                return Ok(None);
            };
            Ok(tables
                .grants
                .iter()
                .find(|grant| grant.id == row.grant_id && grant.is_active(now))
                .and_then(|grant| {
                    Some(AccessTokenGrant {
                        principal: self.users.get(&grant.user_id)?.clone(),
                        resource: grant.resource.clone(),
                    })
                }))
        }

        async fn revoke_grant_of_token(
            &self,
            token: &OAuthSecretHash,
            client_id: Option<&str>,
        ) -> Result<bool, OAuthError> {
            let now = OffsetDateTime::now_utc();
            let mut tables = self.tables.lock().unwrap();
            let tables = &mut *tables;
            let Some(row) = tables.tokens.iter().find(|row| row.hash == *token) else {
                return Ok(false);
            };
            let grant = tables
                .grants
                .iter_mut()
                .find(|grant| grant.id == row.grant_id)
                .expect("a token's grant exists");
            if client_id.is_some_and(|client_id| client_id != grant.client_id) {
                return Ok(false);
            }
            Ok(grant.revoke(now))
        }

        async fn list_active_grants(
            &self,
            user_id: &UserId,
        ) -> Result<Vec<OAuthGrant>, OAuthError> {
            let now = OffsetDateTime::now_utc();
            let tables = self.tables.lock().unwrap();
            // A grant whose refresh token idled out can never be used again.
            let refreshable = |grant_id: OAuthGrantId| {
                tables.tokens.iter().any(|token| {
                    token.grant_id == grant_id
                        && token.kind == TokenKind::Refresh
                        && token.rotated_at.is_none()
                        && token.expires_at > now
                })
            };
            Ok(tables
                .grants
                .iter()
                .rev()
                .filter(|grant| {
                    grant.user_id == *user_id && grant.is_active(now) && refreshable(grant.id)
                })
                .map(|grant| OAuthGrant {
                    id: grant.id,
                    client_id: grant.client_id.clone(),
                    client_name: grant.client_name.clone(),
                    redirect_uri: grant.redirect_uri.clone(),
                    created_at: grant.created_at,
                    last_used_at: grant.last_used_at,
                    expires_at: grant.expires_at,
                })
                .collect())
        }

        async fn revoke_grant(
            &self,
            user_id: &UserId,
            grant_id: &OAuthGrantId,
        ) -> Result<(), OAuthError> {
            let now = OffsetDateTime::now_utc();
            self.tables
                .lock()
                .unwrap()
                .grants
                .iter_mut()
                .find(|grant| {
                    grant.id == *grant_id && grant.user_id == *user_id && grant.is_active(now)
                })
                .ok_or(OAuthError::NotFound)?
                .revoke(now);
            Ok(())
        }

        async fn delete_expired(&self) -> Result<(), OAuthError> {
            let now = OffsetDateTime::now_utc();
            let day_ago = now - Duration::days(1);
            let month_ago = now - Duration::days(30);
            let mut tables = self.tables.lock().unwrap();
            tables.requests.retain(|request| request.expires_at > now);
            tables.codes.retain(|row| row.code.expires_at > day_ago);
            tables.grants.retain(|grant| {
                grant.expires_at > month_ago
                    && grant.revoked_at.is_none_or(|revoked| revoked > month_ago)
            });
            let grant_ids: Vec<_> = tables.grants.iter().map(|grant| grant.id).collect();
            tables
                .codes
                .retain(|row| row.grant_id.is_none_or(|id| grant_ids.contains(&id)));
            // Rotated refresh tokens stay while their grant does, so their
            // reuse is still detected.
            tables.tokens.retain(|row| {
                grant_ids.contains(&row.grant_id)
                    && (row.expires_at > day_ago || row.rotated_at.is_some())
            });
            tables
                .clients
                .retain(|row| row.used || row.created_at > month_ago);
            Ok(())
        }
    }

    /// Serves fixed metadata documents; any other client_id is unavailable.
    struct FixedDocuments(HashMap<String, Arc<ClientMetadataDocument>>);

    #[async_trait]
    impl ClientMetadataResolver for FixedDocuments {
        async fn resolve(
            &self,
            client_id: &ClientIdUrl,
        ) -> Result<Arc<ClientMetadataDocument>, OAuthError> {
            self.0.get(client_id.as_str()).cloned().ok_or_else(|| {
                OAuthError::ClientMetadataUnavailable("no document is served".to_string())
            })
        }
    }

    fn documents() -> FixedDocuments {
        let document = |client_id: &str, redirect_uris: &[&str]| ClientMetadataDocument {
            client_id: client_id.to_string(),
            client_name: None,
            redirect_uris: redirect_uris.iter().map(|uri| uri.to_string()).collect(),
            token_endpoint_auth_method: Some("none".to_string()),
            token_endpoint_auth_methods_supported: None,
        };
        let served = [
            (
                CLIENT_ID,
                ClientMetadataDocument {
                    client_name: Some("Claude Code".to_string()),
                    ..document(
                        CLIENT_ID,
                        &["http://localhost/callback", "http://127.0.0.1/callback"],
                    )
                },
            ),
            (
                OTHER_CLIENT_ID,
                document(OTHER_CLIENT_ID, &["http://localhost/callback"]),
            ),
            // Served at one URL but naming another.
            (
                "https://evil.example/client",
                document(CLIENT_ID, &["http://localhost/callback"]),
            ),
        ];
        FixedDocuments(
            served
                .into_iter()
                .map(|(url, document)| (url.to_string(), Arc::new(document)))
                .collect(),
        )
    }

    fn user(id: i32, email: &str) -> UserPrincipal {
        UserPrincipal {
            id: UserId::from(id),
            email: email.to_string(),
            roles: vec![Role::User],
        }
    }

    fn ada() -> UserPrincipal {
        user(7, "ada@example.com")
    }

    fn bob() -> UserPrincipal {
        user(8, "bob@example.com")
    }

    type Service = OAuthServiceImpl<MemoryOAuth, FixedDocuments>;

    fn service_on(repository: &Arc<MemoryOAuth>, resource: &str) -> Service {
        OAuthServiceImpl::new(
            repository.clone(),
            Arc::new(documents()),
            resource.to_string(),
        )
    }

    fn setup() -> (Service, Arc<MemoryOAuth>) {
        let repository = Arc::new(MemoryOAuth::with_users(&[ada(), bob()]));
        (service_on(&repository, RESOURCE), repository)
    }

    fn params() -> AuthorizationParams {
        AuthorizationParams {
            client_id: Some(CLIENT_ID.to_string()),
            redirect_uri: Some(REDIRECT_URI.to_string()),
            response_type: Some("code".to_string()),
            code_challenge: Some(CHALLENGE.to_string()),
            code_challenge_method: Some("S256".to_string()),
            resource: Some(RESOURCE.to_string()),
            state: Some(STATE.to_string()),
        }
    }

    fn with(change: impl FnOnce(&mut AuthorizationParams)) -> AuthorizationParams {
        let mut params = params();
        change(&mut params);
        params
    }

    /// Shows the default request to `user` for consent and returns its handle.
    async fn consent(service: &Service, user: &UserPrincipal) -> OAuthSecret {
        let authorization = service.validate_authorization(params()).await.unwrap();
        service
            .begin_consent(&user.id, &authorization)
            .await
            .unwrap()
    }

    async fn issue_code(service: &Service, user: &UserPrincipal) -> OAuthSecret {
        let request = consent(service, user).await;
        let response = service
            .decide(&user.id, request.as_str(), true)
            .await
            .unwrap();
        match response.outcome {
            AuthorizationOutcome::Code(code) => code,
            AuthorizationOutcome::Error { code, .. } => panic!("not allowed: {code:?}"),
        }
    }

    fn exchange(code: &OAuthSecret) -> CodeExchange {
        CodeExchange {
            code: code.as_str().to_string(),
            redirect_uri: REDIRECT_URI.to_string(),
            client_id: CLIENT_ID.to_string(),
            code_verifier: VERIFIER.to_string(),
            resource: Some(RESOURCE.to_string()),
        }
    }

    async fn connect(service: &Service, user: &UserPrincipal) -> IssuedTokens {
        let code = issue_code(service, user).await;
        service.exchange_code(exchange(&code)).await.unwrap()
    }

    fn refresh_with(token: &OAuthSecret) -> TokenRefresh {
        TokenRefresh {
            refresh_token: token.as_str().to_string(),
            client_id: CLIENT_ID.to_string(),
            resource: None,
        }
    }

    async fn authenticated(service: &Service, token: &OAuthSecret) -> Option<UserId> {
        service
            .authenticate(token.as_str())
            .await
            .unwrap()
            .map(|principal| principal.id)
    }

    fn assert_invalid_grant<T: fmt::Debug>(result: Result<T, OAuthError>, case: &str) {
        assert!(
            matches!(result, Err(OAuthError::InvalidGrant(_))),
            "{case}: {result:?}"
        );
    }

    #[tokio::test]
    async fn a_client_connects_authenticates_and_refreshes() {
        let (service, _) = setup();
        let ada = ada();

        let authorization = service.validate_authorization(params()).await.unwrap();
        assert_eq!(authorization.client.name, "Claude Code");
        assert_eq!(authorization.client.kind, OAuthClientKind::MetadataDocument);
        assert_eq!(
            authorization.registered_redirect_uri.as_str(),
            "http://localhost/callback"
        );
        assert_eq!(authorization.redirect_uri, REDIRECT_URI);
        assert_eq!(authorization.resource, RESOURCE);
        assert_eq!(authorization.state.as_deref(), Some(STATE));

        let request = service
            .begin_consent(&ada.id, &authorization)
            .await
            .unwrap();
        let response = service
            .decide(&ada.id, request.as_str(), true)
            .await
            .unwrap();
        assert_eq!(response.redirect_uri, REDIRECT_URI);
        assert_eq!(response.state.as_deref(), Some(STATE));
        let AuthorizationOutcome::Code(code) = response.outcome else {
            panic!("allowing should issue a code");
        };

        let issued = service
            .exchange_code(CodeExchange {
                // The resource matches after normalization.
                resource: Some("HTTPS://TOKI.example:443/mcp".to_string()),
                ..exchange(&code)
            })
            .await
            .unwrap();
        assert_eq!(issued.expires_in, ACCESS_TOKEN_LIFETIME);
        assert_eq!(
            authenticated(&service, &issued.access_token).await,
            Some(ada.id)
        );
        // A refresh token is not an access token.
        assert_eq!(authenticated(&service, &issued.refresh_token).await, None);

        let refreshed = service
            .refresh(refresh_with(&issued.refresh_token))
            .await
            .unwrap();
        assert_ne!(refreshed.refresh_token, issued.refresh_token);
        assert_eq!(
            authenticated(&service, &refreshed.access_token).await,
            Some(ada.id)
        );

        let grants = service.list_grants(&ada.id).await.unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].client_id, CLIENT_ID);
        assert_eq!(grants[0].client_name, "Claude Code");
        assert!(service.list_grants(&bob().id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn untrusted_clients_and_redirects_are_never_redirected_to() {
        let (service, _) = setup();
        let cases = [
            ("no client_id", with(|p| p.client_id = None)),
            (
                "empty client_id",
                with(|p| p.client_id = Some(String::new())),
            ),
            (
                "client_id URL without a path",
                with(|p| p.client_id = Some("https://claude.ai/".to_string())),
            ),
            (
                "unavailable document",
                with(|p| p.client_id = Some("https://unknown.example/client".to_string())),
            ),
            (
                "document naming another client_id",
                with(|p| p.client_id = Some("https://evil.example/client".to_string())),
            ),
            (
                "unregistered client",
                with(|p| p.client_id = Some("0123456789abcdef0123456789abcdef".to_string())),
            ),
            ("no redirect_uri", with(|p| p.redirect_uri = None)),
            (
                "unregistered redirect_uri",
                with(|p| p.redirect_uri = Some("https://evil.example/callback".to_string())),
            ),
            (
                "loopback host other than the registered ones",
                with(|p| p.redirect_uri = Some("http://[::1]:53682/callback".to_string())),
            ),
            (
                "bad client with otherwise redirectable errors",
                with(|p| {
                    p.client_id = Some("https://unknown.example/client".to_string());
                    p.response_type = Some("token".to_string());
                }),
            ),
        ];
        for (case, params) in cases {
            assert!(
                matches!(
                    service.validate_authorization(params).await,
                    Err(AuthorizationError::Unredirectable(_))
                ),
                "{case}"
            );
        }
    }

    #[tokio::test]
    async fn other_request_errors_go_back_to_the_redirect_uri_with_state() {
        let (service, _) = setup();
        let cases = [
            (
                "implicit flow",
                with(|p| p.response_type = Some("token".to_string())),
                AuthorizationErrorCode::UnsupportedResponseType,
            ),
            (
                "no response_type",
                with(|p| p.response_type = None),
                AuthorizationErrorCode::UnsupportedResponseType,
            ),
            (
                "plain PKCE",
                with(|p| p.code_challenge_method = Some("plain".to_string())),
                AuthorizationErrorCode::InvalidRequest,
            ),
            (
                "no code_challenge_method",
                with(|p| p.code_challenge_method = None),
                AuthorizationErrorCode::InvalidRequest,
            ),
            (
                "no code_challenge",
                with(|p| p.code_challenge = None),
                AuthorizationErrorCode::InvalidRequest,
            ),
            (
                "malformed code_challenge",
                with(|p| p.code_challenge = Some("too-short".to_string())),
                AuthorizationErrorCode::InvalidRequest,
            ),
            (
                "resource with a trailing slash",
                with(|p| p.resource = Some(format!("{RESOURCE}/"))),
                AuthorizationErrorCode::InvalidTarget,
            ),
            (
                "resource on another host",
                with(|p| p.resource = Some("https://other.example/mcp".to_string())),
                AuthorizationErrorCode::InvalidTarget,
            ),
            (
                "resource that is not a URL",
                with(|p| p.resource = Some("mcp".to_string())),
                AuthorizationErrorCode::InvalidTarget,
            ),
        ];
        for (case, params, expected) in cases {
            match service.validate_authorization(params).await {
                Err(AuthorizationError::Redirect(response)) => {
                    assert_eq!(response.redirect_uri, REDIRECT_URI, "{case}");
                    assert_eq!(response.state.as_deref(), Some(STATE), "{case}");
                    assert!(
                        matches!(
                            response.outcome,
                            AuthorizationOutcome::Error { code, .. } if code == expected
                        ),
                        "{case}: {:?}",
                        response.outcome
                    );
                }
                other => panic!("{case}: {other:?}"),
            }
        }

        match service
            .validate_authorization(with(|p| {
                p.state = None;
                p.response_type = None;
            }))
            .await
        {
            Err(AuthorizationError::Redirect(response)) => assert_eq!(response.state, None),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn an_absent_or_equivalent_resource_means_the_mcp_endpoint() {
        let (service, _) = setup();
        for resource in [
            None,
            Some("https://TOKI.EXAMPLE/mcp"),
            Some("https://toki.example:443/mcp"),
        ] {
            let authorization = service
                .validate_authorization(with(|p| p.resource = resource.map(str::to_string)))
                .await
                .unwrap_or_else(|error| panic!("{resource:?}: {error:?}"));
            assert_eq!(authorization.resource, RESOURCE);
        }
    }

    #[tokio::test]
    async fn a_registered_client_connects_and_is_marked_used() {
        let (service, repository) = setup();
        let registered = service
            .register_client(ClientRegistration {
                redirect_uris: Some(vec!["http://127.0.0.1/callback".to_string()]),
                client_name: Some("  Executor ".to_string()),
                grant_types: Some(vec![
                    "authorization_code".to_string(),
                    "refresh_token".to_string(),
                ]),
                response_types: Some(vec!["code".to_string()]),
                token_endpoint_auth_method: Some("none".to_string()),
            })
            .await
            .unwrap();
        let client_id = registered.client.id.clone();
        assert_eq!(registered.client.name, "Executor");
        assert_eq!(registered.client.kind, OAuthClientKind::Registered);

        let redirect_uri = "http://127.0.0.1:40123/callback";
        let authorization = service
            .validate_authorization(with(|p| {
                p.client_id = Some(client_id.clone());
                p.redirect_uri = Some(redirect_uri.to_string());
            }))
            .await
            .unwrap();
        assert_eq!(authorization.client.kind, OAuthClientKind::Registered);
        assert_eq!(authorization.client.name, "Executor");
        assert_eq!(authorization.client.verified_host(), None);

        let ada = ada();
        let request = service
            .begin_consent(&ada.id, &authorization)
            .await
            .unwrap();
        let AuthorizationOutcome::Code(code) = service
            .decide(&ada.id, request.as_str(), true)
            .await
            .unwrap()
            .outcome
        else {
            panic!("allowing should issue a code");
        };
        assert!(!repository.client_used(&client_id));
        let issued = service
            .exchange_code(CodeExchange {
                code: code.as_str().to_string(),
                redirect_uri: redirect_uri.to_string(),
                client_id: client_id.clone(),
                code_verifier: VERIFIER.to_string(),
                resource: None,
            })
            .await
            .unwrap();
        assert!(repository.client_used(&client_id));
        assert_eq!(
            authenticated(&service, &issued.access_token).await,
            Some(ada.id)
        );
    }

    #[tokio::test]
    async fn a_consent_request_is_decided_once_by_its_own_user() {
        let (service, repository) = setup();
        let (ada, bob) = (ada(), bob());
        let request = consent(&service, &ada).await;

        assert!(matches!(
            service.decide(&bob.id, request.as_str(), true).await,
            Err(OAuthError::NotFound)
        ));
        assert!(matches!(
            service.decide(&ada.id, "not-a-request", true).await,
            Err(OAuthError::NotFound)
        ));
        let code = OAuthSecret::generate(OAuthSecretKind::AuthorizationCode);
        assert!(matches!(
            service.decide(&ada.id, code.as_str(), true).await,
            Err(OAuthError::NotFound)
        ));

        // Another user's attempt leaves the request to its own user.
        service
            .decide(&ada.id, request.as_str(), true)
            .await
            .unwrap();
        for allow in [true, false] {
            assert!(matches!(
                service.decide(&ada.id, request.as_str(), allow).await,
                Err(OAuthError::NotFound)
            ));
        }
        assert_eq!(repository.code_count(), 1);

        let expired = consent(&service, &ada).await;
        repository.expire_requests();
        assert!(matches!(
            service.decide(&ada.id, expired.as_str(), true).await,
            Err(OAuthError::NotFound)
        ));
    }

    #[tokio::test]
    async fn denying_sends_access_denied_with_state() {
        let (service, repository) = setup();
        let ada = ada();
        let request = consent(&service, &ada).await;

        let response = service
            .decide(&ada.id, request.as_str(), false)
            .await
            .unwrap();
        assert_eq!(response.redirect_uri, REDIRECT_URI);
        assert_eq!(response.state.as_deref(), Some(STATE));
        assert!(matches!(
            response.outcome,
            AuthorizationOutcome::Error {
                code: AuthorizationErrorCode::AccessDenied,
                ..
            }
        ));
        assert!(matches!(
            service.decide(&ada.id, request.as_str(), true).await,
            Err(OAuthError::NotFound)
        ));
        assert_eq!(repository.code_count(), 0);
    }

    #[tokio::test]
    async fn a_mismatched_or_expired_code_exchange_fails_and_spends_the_code() {
        let (service, repository) = setup();
        let ada = ada();
        type Change = fn(&mut CodeExchange);
        let cases: [(&str, Change); 4] = [
            ("wrong verifier", |e| e.code_verifier = "a".repeat(43)),
            // Matches the registered loopback URI, but not the request's.
            ("other redirect_uri", |e| {
                e.redirect_uri = "http://localhost:1/callback".to_string()
            }),
            ("other client", |e| {
                e.client_id = OTHER_CLIENT_ID.to_string()
            }),
            ("other resource", |e| {
                e.resource = Some("https://other.example/mcp".to_string())
            }),
        ];
        for (case, change) in cases {
            let code = issue_code(&service, &ada).await;
            let mut wrong = exchange(&code);
            change(&mut wrong);
            assert_invalid_grant(service.exchange_code(wrong).await, case);
            assert_invalid_grant(service.exchange_code(exchange(&code)).await, case);
        }

        let code = issue_code(&service, &ada).await;
        repository.expire_codes();
        assert_invalid_grant(service.exchange_code(exchange(&code)).await, "expired");
        assert_invalid_grant(service.exchange_code(exchange(&code)).await, "expired");

        assert_eq!(repository.grant_count(), 0);
    }

    #[tokio::test]
    async fn malformed_exchanges_are_refused_before_the_code_is_spent() {
        let (service, _) = setup();
        let code = issue_code(&service, &ada()).await;

        assert!(matches!(
            service
                .exchange_code(CodeExchange {
                    client_id: String::new(),
                    ..exchange(&code)
                })
                .await,
            Err(OAuthError::InvalidClient(_))
        ));
        assert!(matches!(
            service
                .exchange_code(CodeExchange {
                    code_verifier: "too-short".to_string(),
                    ..exchange(&code)
                })
                .await,
            Err(OAuthError::InvalidRequest(_))
        ));
        for wrong_code in [
            "not-a-code".to_string(),
            OAuthSecret::generate(OAuthSecretKind::RefreshToken)
                .as_str()
                .to_string(),
            OAuthSecret::generate(OAuthSecretKind::AuthorizationCode)
                .as_str()
                .to_string(),
        ] {
            assert_invalid_grant(
                service
                    .exchange_code(CodeExchange {
                        code: wrong_code,
                        ..exchange(&code)
                    })
                    .await,
                "unknown code",
            );
        }

        service.exchange_code(exchange(&code)).await.unwrap();
    }

    #[tokio::test]
    async fn a_replayed_code_revokes_its_connection() {
        let (service, _) = setup();
        let ada = ada();
        let code = issue_code(&service, &ada).await;
        let issued = service.exchange_code(exchange(&code)).await.unwrap();
        assert!(authenticated(&service, &issued.access_token)
            .await
            .is_some());

        assert_invalid_grant(service.exchange_code(exchange(&code)).await, "replay");
        assert_eq!(authenticated(&service, &issued.access_token).await, None);
        assert_invalid_grant(
            service.refresh(refresh_with(&issued.refresh_token)).await,
            "refresh after replay",
        );
        assert!(service.list_grants(&ada.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn only_the_clients_own_refresh_tokens_refresh() {
        let (service, _) = setup();
        let issued = connect(&service, &ada()).await;

        assert_invalid_grant(
            service.refresh(refresh_with(&issued.access_token)).await,
            "access token",
        );
        assert_invalid_grant(
            service
                .refresh(TokenRefresh {
                    refresh_token: "tmcp_rt_nope".to_string(),
                    ..refresh_with(&issued.refresh_token)
                })
                .await,
            "malformed",
        );
        assert!(matches!(
            service
                .refresh(TokenRefresh {
                    client_id: String::new(),
                    ..refresh_with(&issued.refresh_token)
                })
                .await,
            Err(OAuthError::InvalidClient(_))
        ));
        // A refused client or resource changes nothing.
        assert_invalid_grant(
            service
                .refresh(TokenRefresh {
                    client_id: OTHER_CLIENT_ID.to_string(),
                    ..refresh_with(&issued.refresh_token)
                })
                .await,
            "other client",
        );
        assert_invalid_grant(
            service
                .refresh(TokenRefresh {
                    resource: Some("https://other.example/mcp".to_string()),
                    ..refresh_with(&issued.refresh_token)
                })
                .await,
            "other resource",
        );

        service
            .refresh(TokenRefresh {
                resource: Some(RESOURCE.to_string()),
                ..refresh_with(&issued.refresh_token)
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn refresh_tokens_rotate() {
        let (service, _) = setup();
        let ada = ada();
        let first = connect(&service, &ada).await;
        let second = service
            .refresh(refresh_with(&first.refresh_token))
            .await
            .unwrap();
        let third = service
            .refresh(refresh_with(&second.refresh_token))
            .await
            .unwrap();

        assert_eq!(
            authenticated(&service, &third.access_token).await,
            Some(ada.id)
        );
        // Rotation leaves earlier access tokens to expire on their own.
        assert_eq!(
            authenticated(&service, &first.access_token).await,
            Some(ada.id)
        );
        assert_eq!(service.list_grants(&ada.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn reuse_within_the_grace_period_fails_without_revoking() {
        let (service, _) = setup();
        let ada = ada();
        let first = connect(&service, &ada).await;
        let successor = service
            .refresh(refresh_with(&first.refresh_token))
            .await
            .unwrap();

        assert_invalid_grant(
            service.refresh(refresh_with(&first.refresh_token)).await,
            "concurrent reuse",
        );
        assert_eq!(
            authenticated(&service, &successor.access_token).await,
            Some(ada.id)
        );
        assert_eq!(service.list_grants(&ada.id).await.unwrap().len(), 1);
        service
            .refresh(refresh_with(&successor.refresh_token))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn reuse_after_the_grace_period_revokes_the_connection() {
        let (service, repository) = setup();
        let ada = ada();
        let first = connect(&service, &ada).await;
        let successor = service
            .refresh(refresh_with(&first.refresh_token))
            .await
            .unwrap();
        repository.update_token(&first.refresh_token, |row| {
            row.rotated_at = Some(OffsetDateTime::now_utc() - REFRESH_REUSE_GRACE);
        });

        assert_invalid_grant(
            service.refresh(refresh_with(&first.refresh_token)).await,
            "reuse",
        );
        assert_eq!(authenticated(&service, &successor.access_token).await, None);
        assert_invalid_grant(
            service
                .refresh(refresh_with(&successor.refresh_token))
                .await,
            "successor of a revoked connection",
        );
        assert!(service.list_grants(&ada.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reuse_of_an_expired_rotated_token_still_revokes() {
        let (service, repository) = setup();
        let first = connect(&service, &ada()).await;
        let successor = service
            .refresh(refresh_with(&first.refresh_token))
            .await
            .unwrap();
        repository.update_token(&first.refresh_token, |row| {
            let past = OffsetDateTime::now_utc() - Duration::days(31);
            row.rotated_at = Some(past);
            row.expires_at = past;
        });

        assert_invalid_grant(
            service.refresh(refresh_with(&first.refresh_token)).await,
            "expired reuse",
        );
        assert_eq!(authenticated(&service, &successor.access_token).await, None);
    }

    #[tokio::test]
    async fn expired_tokens_neither_authenticate_nor_refresh() {
        let (service, repository) = setup();
        let issued = connect(&service, &ada()).await;
        let past = OffsetDateTime::now_utc() - Duration::seconds(1);
        repository.update_token(&issued.access_token, |row| row.expires_at = past);
        repository.update_token(&issued.refresh_token, |row| row.expires_at = past);

        assert_eq!(authenticated(&service, &issued.access_token).await, None);
        assert_invalid_grant(
            service.refresh(refresh_with(&issued.refresh_token)).await,
            "expired refresh token",
        );
    }

    #[tokio::test]
    async fn revoked_and_expired_connections_stop_working() {
        let (service, repository) = setup();
        let (ada, bob) = (ada(), bob());
        let revoked = connect(&service, &ada).await;
        let grant_id = service.list_grants(&ada.id).await.unwrap()[0].id;

        assert!(matches!(
            service.revoke_grant(&bob.id, &grant_id).await,
            Err(OAuthError::NotFound)
        ));
        service.revoke_grant(&ada.id, &grant_id).await.unwrap();
        assert!(matches!(
            service.revoke_grant(&ada.id, &grant_id).await,
            Err(OAuthError::NotFound)
        ));
        assert_eq!(authenticated(&service, &revoked.access_token).await, None);
        assert_invalid_grant(
            service.refresh(refresh_with(&revoked.refresh_token)).await,
            "revoked",
        );

        let expired = connect(&service, &ada).await;
        repository.expire_grants();
        assert_eq!(authenticated(&service, &expired.access_token).await, None);
        assert_invalid_grant(
            service.refresh(refresh_with(&expired.refresh_token)).await,
            "expired",
        );
        assert!(service.list_grants(&ada.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn cleanup_keeps_what_live_connections_need() {
        let (service, repository) = setup();
        let ada = ada();
        let first = connect(&service, &ada).await;
        let successor = service
            .refresh(refresh_with(&first.refresh_token))
            .await
            .unwrap();
        repository.update_token(&first.refresh_token, |row| {
            let past = OffsetDateTime::now_utc() - Duration::days(31);
            row.rotated_at = Some(past);
            row.expires_at = past;
        });
        let abandoned = consent(&service, &ada).await;
        repository.expire_requests();

        service.remove_expired().await.unwrap();

        assert!(matches!(
            service.decide(&ada.id, abandoned.as_str(), true).await,
            Err(OAuthError::NotFound)
        ));
        assert_eq!(
            authenticated(&service, &successor.access_token).await,
            Some(ada.id)
        );
        // The long-expired ancestor is kept, so its reuse still revokes.
        assert_invalid_grant(
            service.refresh(refresh_with(&first.refresh_token)).await,
            "reuse after cleanup",
        );
        assert_eq!(authenticated(&service, &successor.access_token).await, None);
    }

    #[tokio::test]
    async fn tokens_for_another_resource_do_not_authenticate() {
        let (service, repository) = setup();
        let issued = connect(&service, &ada()).await;
        let elsewhere = service_on(&repository, "https://other.example/mcp");

        assert_eq!(authenticated(&elsewhere, &issued.access_token).await, None);
        assert!(authenticated(&service, &issued.access_token)
            .await
            .is_some());
        assert!(service.authenticate("toki_abc").await.unwrap().is_none());
    }

    fn registration(redirect_uris: &[&str]) -> ClientRegistration {
        ClientRegistration {
            redirect_uris: Some(redirect_uris.iter().map(|uri| uri.to_string()).collect()),
            ..ClientRegistration::default()
        }
    }

    fn strings(values: &[&str]) -> Option<Vec<String>> {
        Some(values.iter().map(|value| value.to_string()).collect())
    }

    #[tokio::test]
    async fn registrations_are_validated() {
        let (service, _) = setup();
        let valid = || registration(&["https://executor.example/callback"]);

        let redirect_cases = [
            ("no redirect_uris", ClientRegistration::default()),
            ("empty redirect_uris", registration(&[])),
            (
                "non-loopback http",
                registration(&["http://executor.example/cb"]),
            ),
            (
                "one invalid among valid",
                registration(&["https://executor.example/cb", "javascript:alert(1)"]),
            ),
            (
                "too many",
                registration(&["https://executor.example/cb"; 11]),
            ),
        ];
        for (case, registration) in redirect_cases {
            assert!(
                matches!(
                    service.register_client(registration).await,
                    Err(OAuthError::InvalidRedirectUri(_))
                ),
                "{case}"
            );
        }

        let metadata_cases = [
            (
                "client_credentials grant",
                ClientRegistration {
                    grant_types: strings(&["authorization_code", "client_credentials"]),
                    ..valid()
                },
            ),
            (
                "token response",
                ClientRegistration {
                    response_types: strings(&["code", "token"]),
                    ..valid()
                },
            ),
            (
                "secret-based authentication",
                ClientRegistration {
                    token_endpoint_auth_method: Some("client_secret_basic".to_string()),
                    ..valid()
                },
            ),
            (
                "blank name",
                ClientRegistration {
                    client_name: Some("   ".to_string()),
                    ..valid()
                },
            ),
            (
                "name with a control character",
                ClientRegistration {
                    client_name: Some("line\nbreak".to_string()),
                    ..valid()
                },
            ),
        ];
        for (case, registration) in metadata_cases {
            assert!(
                matches!(
                    service.register_client(registration).await,
                    Err(OAuthError::InvalidClientMetadata(_))
                ),
                "{case}"
            );
        }

        let first = service.register_client(valid()).await.unwrap();
        assert_eq!(first.client.name, "executor.example");
        assert_eq!(first.client.id.len(), 32);
        assert!(first
            .client
            .id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        let second = service
            .register_client(ClientRegistration {
                grant_types: strings(&["authorization_code"]),
                response_types: strings(&["code"]),
                token_endpoint_auth_method: Some("none".to_string()),
                ..valid()
            })
            .await
            .unwrap();
        assert_ne!(first.client.id, second.client.id);
    }

    #[tokio::test]
    async fn revoking_either_token_revokes_the_connection() {
        let (service, _) = setup();
        let ada = ada();

        let by_access = connect(&service, &ada).await;
        service
            .revoke_token(by_access.access_token.as_str(), Some(CLIENT_ID))
            .await
            .unwrap();
        assert_eq!(authenticated(&service, &by_access.access_token).await, None);
        assert_invalid_grant(
            service
                .refresh(refresh_with(&by_access.refresh_token))
                .await,
            "after revoking by access token",
        );

        let by_refresh = connect(&service, &ada).await;
        service
            .revoke_token(by_refresh.refresh_token.as_str(), Some(CLIENT_ID))
            .await
            .unwrap();
        assert_eq!(
            authenticated(&service, &by_refresh.access_token).await,
            None
        );

        assert!(service.list_grants(&ada.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn revocation_ignores_other_clients_and_unknown_tokens() {
        let (service, _) = setup();
        let ada = ada();
        let issued = connect(&service, &ada).await;

        service
            .revoke_token(issued.access_token.as_str(), Some(OTHER_CLIENT_ID))
            .await
            .unwrap();
        for token in [
            "garbage".to_string(),
            OAuthSecret::generate(OAuthSecretKind::AccessToken)
                .as_str()
                .to_string(),
            OAuthSecret::generate(OAuthSecretKind::AuthorizationCode)
                .as_str()
                .to_string(),
        ] {
            service.revoke_token(&token, Some(CLIENT_ID)).await.unwrap();
        }
        for client_id in [None, Some("")] {
            assert!(matches!(
                service
                    .revoke_token(issued.access_token.as_str(), client_id)
                    .await,
                Err(OAuthError::InvalidClient(_))
            ));
        }

        assert_eq!(
            authenticated(&service, &issued.access_token).await,
            Some(ada.id)
        );
    }
}
