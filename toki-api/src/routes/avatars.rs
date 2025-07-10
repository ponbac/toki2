use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use bytes::Bytes;
use tracing::instrument;

use crate::app_state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/:user_hash", get(serve_avatar))
}

#[instrument(name = "GET /avatars/:user_hash", skip(app_state))]
async fn serve_avatar(
    State(app_state): State<AppState>,
    Path(user_hash): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    // Validate user hash (should be hexadecimal and 16 characters)
    if user_hash.len() != 16 || !user_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid user hash format".to_string(),
        ));
    }

    match app_state.avatar_cache.get_cached_image(&user_hash).await {
        Ok((image_bytes, content_type)) => Ok(create_image_response(image_bytes, content_type)),
        Err(e) => {
            tracing::warn!("Failed to serve avatar for hash {}: {}", user_hash, e);
            Err((StatusCode::NOT_FOUND, "Avatar not found".to_string()))
        }
    }
}

fn create_image_response(image_bytes: Bytes, content_type: String) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, image_bytes.len())
        .header(header::CACHE_CONTROL, "public, max-age=3600") // Cache for 1 hour
        .header(
            header::ETAG,
            format!("\"{}\"", format!("{:x}", md5::compute(&image_bytes))),
        )
        .body(axum::body::Body::from(image_bytes))
        .unwrap()
}
