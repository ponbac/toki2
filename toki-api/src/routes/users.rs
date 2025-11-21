use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};

use crate::{
    auth::AuthSession, domain::User, repositories::UserRepository,
    utils::image::compress_image_webp, AppState,
};

const MAX_AVATAR_SIZE: usize = 5 * 1024 * 1024; // 5MiB
const DEFAULT_AVATAR_MIME: &str = "image/webp";
const AVATAR_CACHE_CONTROL: &str = "private, max-age=3600";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UserProfileResponse {
    #[serde(flatten)]
    user: User,
    avatar_url: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route(
            "/me/avatar",
            get(my_avatar)
                .post(upload_my_avatar)
                .delete(delete_my_avatar),
        )
        .route("/:user_id/avatar", get(user_avatar))
}

async fn me(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Json<UserProfileResponse>, StatusCode> {
    let user = auth_session.user.clone().ok_or(StatusCode::UNAUTHORIZED)?;

    let avatar_url = avatar_url_if_present(&app_state, user.id).await?;

    Ok(Json(UserProfileResponse { user, avatar_url }))
}

async fn my_avatar(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<Response, StatusCode> {
    let user_id = auth_session
        .user
        .as_ref()
        .ok_or(StatusCode::UNAUTHORIZED)?
        .id;
    avatar_response(&app_state, user_id).await
}

async fn user_avatar(
    Path(user_id): Path<i32>,
    State(app_state): State<AppState>,
) -> Result<Response, StatusCode> {
    avatar_response(&app_state, user_id).await
}

async fn upload_my_avatar(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let user = auth_session.user.as_ref().ok_or(StatusCode::UNAUTHORIZED)?;

    let image = extract_image_from_multipart(&mut multipart).await?;

    let compressed_webp_image = compress_image_webp(&image, 512, 80.0, None)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    app_state
        .user_repo
        .set_user_avatar(user.id, compressed_webp_image, "image/webp".to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_my_avatar(
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    let user = auth_session.user.as_ref().ok_or(StatusCode::UNAUTHORIZED)?;

    app_state
        .user_repo
        .clear_user_avatar(user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn avatar_response(app_state: &AppState, user_id: i32) -> Result<Response, StatusCode> {
    let avatar = app_state
        .user_repo
        .get_user_avatar(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let Some(avatar) = avatar else {
        return Err(StatusCode::NOT_FOUND);
    };

    let mut response = Response::new(Body::from(avatar.image));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&avatar.mime_type)
            .unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_AVATAR_MIME)),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(AVATAR_CACHE_CONTROL),
    );

    Ok(response)
}

async fn avatar_url_if_present(
    app_state: &AppState,
    user_id: i32,
) -> Result<Option<String>, StatusCode> {
    let has_avatar = app_state
        .user_repo
        .has_user_avatar(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !has_avatar {
        return Ok(None);
    }

    let url = app_state
        .user_avatar_url(user_id)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Some(url))
}

async fn extract_image_from_multipart(multipart: &mut Multipart) -> Result<Vec<u8>, StatusCode> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        if field.name() != Some("avatar") {
            continue;
        }

        let content_type = field
            .content_type()
            .map(|ct| ct.to_string())
            .unwrap_or_else(|| "image/png".to_string());

        if !content_type.starts_with("image/") {
            return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        }

        let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

        if bytes.len() > MAX_AVATAR_SIZE {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }

        return Ok(bytes.to_vec());
    }

    Err(StatusCode::BAD_REQUEST)
}
