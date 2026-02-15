use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    routing::get,
    Router,
};

use crate::{
    app_state::AppState,
    auth::AuthUser,
    domain::{models::UserId, AvatarError},
    routes::ApiError,
};

const DEFAULT_AVATAR_MIME: &str = "image/webp";
const AVATAR_CACHE_CONTROL: &str = "private, max-age=3600";
// Allow multipart overhead while keeping the actual avatar payload policy at 5 MiB.
const AVATAR_UPLOAD_BODY_LIMIT: usize = 6 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/me/avatar",
            get(my_avatar)
                .post(upload_my_avatar)
                .delete(delete_my_avatar),
        )
        .route_layer(DefaultBodyLimit::max(AVATAR_UPLOAD_BODY_LIMIT))
        .route("/:user_id/avatar", get(user_avatar))
}

async fn my_avatar(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Response, ApiError> {
    avatar_response(&app_state, user.id).await
}

async fn user_avatar(
    Path(user_id): Path<i32>,
    State(app_state): State<AppState>,
) -> Result<Response, ApiError> {
    avatar_response(&app_state, UserId::from(user_id)).await
}

async fn upload_my_avatar(
    user: AuthUser,
    State(app_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    let (image, content_type) = extract_image_from_multipart(&mut multipart).await?;

    app_state
        .avatar_service
        .upload_avatar(&user.id, image, content_type)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_my_avatar(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<StatusCode, ApiError> {
    app_state
        .avatar_service
        .delete_avatar(&user.id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn avatar_response(app_state: &AppState, user_id: UserId) -> Result<Response, ApiError> {
    let avatar = app_state
        .avatar_service
        .get_avatar(&user_id)
        .await?
        .ok_or(AvatarError::NotFound)?;

    let mut response = Response::new(Body::from(avatar.bytes));
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

async fn extract_image_from_multipart(
    multipart: &mut Multipart,
) -> Result<(Vec<u8>, Option<String>), ApiError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::bad_request("failed to parse multipart field"))?
    {
        if field.name() != Some("avatar") {
            continue;
        }

        let content_type = field.content_type().map(str::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|_| ApiError::bad_request("failed to read avatar payload"))?;

        return Ok((bytes.to_vec(), content_type));
    }

    Err(ApiError::bad_request("missing avatar file field"))
}
