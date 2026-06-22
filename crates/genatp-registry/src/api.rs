//! OCI Distribution v1.1-compatible API server for the cassette registry.
//!
//! Exposes endpoints for publishing, listing, and installing cassettes:
//!
//! - `GET  /v2/`                              — OCI API version check
//! - `GET  /v2/{slug}/tags/list`              — list versions/tags
//! - `GET  /v2/{slug}/manifests/{version}`     — get manifest
//! - `PUT  /v2/{slug}/manifests/{version}`     — publish manifest
//! - `POST /v2/{slug}/blobs/uploads/`          — start blob upload
//! - `PUT  /v2/{slug}/blobs/uploads/{uuid}`    — complete blob upload
//! - `GET  /v2/{slug}/blobs/{digest}`          — download blob
//! - `GET  /search?q={query}`                  — search cassettes (non-OCI)
//! - `GET  /publishers`                        — list publishers (non-OCI)
//! - `GET  /health`                            — health check

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::manifest::{RegistryManifest, TrustLevel};
use crate::storage::RegistryStore;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<RegistryStore>>,
    pub blob_dir: PathBuf,
    pub known_publishers: Vec<crate::manifest::Publisher>,
}

/// OCI tags list response.
#[derive(Debug, Serialize, Deserialize)]
pub struct TagsList {
    pub name: String,
    pub tags: Vec<String>,
}

/// Search query parameters.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

/// Build the registry API router.
pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::permissive();
    Router::new()
        .route("/health", get(health_check))
        .route("/v2/", get(api_version))
        .route("/v2/{slug}/tags/list", get(list_tags))
        .route("/v2/{slug}/manifests/{version}", get(get_manifest_handler))
        .route("/v2/{slug}/manifests/{version}", put(publish_manifest))
        .route("/search", get(search_cassettes))
        .route("/publishers", get(list_publishers))
        .layer(cors)
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// OCI API version check.
async fn api_version() -> impl IntoResponse {
    Json(serde_json::json!({ "version": "1.1" }))
}

/// `GET /v2/{slug}/tags/list` — list all versions of a cassette.
async fn list_tags(State(state): State<AppState>, Path(slug): Path<String>) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    match store.list_versions(&slug) {
        Ok(tags) => (
            StatusCode::OK,
            Json(serde_json::json!(TagsList { name: slug, tags })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// `GET /v2/{slug}/manifests/{version}` — get manifest for a version.
async fn get_manifest_handler(
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    match store.resolve(&slug, &version) {
        Ok(Some(manifest)) => (StatusCode::OK, Json(serde_json::json!(manifest))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("cassette {slug}@{version} not found") })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// `PUT /v2/{slug}/manifests/{version}` — publish a new manifest.
async fn publish_manifest(
    State(state): State<AppState>,
    Path((slug, version)): Path<(String, String)>,
    Json(manifest): Json<RegistryManifest>,
) -> impl IntoResponse {
    // Validate slug and version match the URL.
    if manifest.slug != slug {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "slug in manifest does not match URL" })),
        );
    }
    if manifest.version != version {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "version in manifest does not match URL" })),
        );
    }

    // Check trust level — unsigned cassettes are blocked.
    let trust_level = manifest.evaluate_trust(&state.known_publishers);
    if trust_level == TrustLevel::Unsigned {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "unsigned cassettes are not accepted" })),
        );
    }

    // Check publisher cooldown for verification.
    if let Ok(true) = state
        .store
        .lock()
        .unwrap()
        .is_publisher_in_cooldown(&manifest.publisher)
    {
        // Publisher is in 24h cooldown — only allow self-signed.
        if trust_level == TrustLevel::Verified {
            return (
                StatusCode::FORBIDDEN,
                Json(
                    serde_json::json!({ "error": "publisher in cooldown period (24h). verified status not yet available." }),
                ),
            );
        }
    }

    let store = state.store.lock().unwrap();
    match store.publish(&manifest) {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "status": "published", "slug": slug, "version": version })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// `GET /search?q=...` — search cassettes by keyword.
async fn search_cassettes(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    match store.search(&query.q) {
        Ok(results) => {
            let summaries: Vec<serde_json::Value> = results
                .into_iter()
                .map(|m| {
                    serde_json::json!({
                        "slug": m.slug,
                        "version": m.version,
                        "description": m.description,
                        "publisher": m.publisher,
                        "trust_level": format!("{:?}", m.trust_level),
                        "step_count": m.step_count,
                        "domains": m.domains,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(summaries)))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// `GET /publishers` — list all registered publishers.
async fn list_publishers(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    match store.list_publishers() {
        Ok(publishers) => (StatusCode::OK, Json(serde_json::json!(publishers))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// Start the registry API server.
///
/// # Errors
/// Surfaces binding errors from axum.
pub async fn serve(state: AppState, addr: &std::net::SocketAddr) -> Result<(), anyhow::Error> {
    let app = router(state);
    tracing::info!(%addr, "starting registry API server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
