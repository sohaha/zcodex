use anyhow::Result;
use axum::Json;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use codex_zmemory::compat::BrowseNodePayload;
use codex_zmemory::compat::CompatService;
use codex_zmemory::compat::DeleteOrphanResponse;
use codex_zmemory::compat::DomainSummary;
use codex_zmemory::compat::ErrorDetailResponse;
use codex_zmemory::compat::GlossaryListResponse;
use codex_zmemory::compat::HealthResponse;
use codex_zmemory::compat::OrphanDetailResponse;
use codex_zmemory::compat::OrphanListItemResponse;
use codex_zmemory::compat::RebuildSearchResponse;
use codex_zmemory::compat::ReviewDeprecatedResponse;
use codex_zmemory::compat::ReviewDiffResponse;
use codex_zmemory::compat::ReviewGroupItemResponse;
use codex_zmemory::compat::SuccessMessageResponse;
use codex_zmemory::compat::UpdateNodeResponse;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    service: Arc<CompatService>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNodeBody {
    pub content: Option<String>,
    pub priority: Option<i64>,
    pub disclosure: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NodeQuery {
    pub domain: Option<String>,
    pub path: Option<String>,
    pub nav_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct NamespaceQuery {
    pub domain: Option<String>,
    pub path: Option<String>,
}

pub async fn serve_compat(bind: String, service: CompatService) -> Result<()> {
    let state = AppState {
        service: Arc::new(service),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/api/browse/domains", get(list_domains))
        .route("/api/browse/namespaces", get(list_namespaces))
        .route("/api/browse/node", get(get_node).put(update_node))
        .route("/api/browse/glossary", get(get_glossary))
        .route("/api/review/groups", get(review_groups))
        .route(
            "/api/review/groups/{node_uuid}/diff",
            get(review_group_diff),
        )
        .route(
            "/api/review/groups/{node_uuid}/rollback",
            post(review_write_not_implemented),
        )
        .route(
            "/api/review/groups/{node_uuid}",
            delete(review_write_not_implemented),
        )
        .route("/api/review", delete(review_write_not_implemented))
        .route("/api/review/deprecated", get(review_deprecated))
        .route("/api/maintenance/stats", get(admin_stats))
        .route("/api/maintenance/doctor", get(admin_doctor))
        .route("/api/maintenance/orphans", get(list_orphans))
        .route(
            "/api/maintenance/orphans/{memory_id}",
            get(orphan_detail).delete(delete_orphan),
        )
        .route("/api/maintenance/rebuild-search", post(rebuild_search))
        .with_state(state);

    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(state.service.health())
}

async fn list_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<Vec<DomainSummary>> {
    Ok(Json(
        state
            .service
            .list_domains_for_namespace(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

async fn list_namespaces(State(state): State<AppState>) -> CompatResult<Vec<String>> {
    Ok(Json(
        state.service.list_namespaces().map_err(map_compat_err)?,
    ))
}

async fn get_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NodeQuery>,
) -> CompatResult<BrowseNodePayload> {
    let domain = query.domain.unwrap_or_else(|| "core".to_string());
    let path = query.path.unwrap_or_default();
    Ok(Json(
        state
            .service
            .browse_node(
                requested_namespace(&headers),
                &domain,
                &path,
                query.nav_only.unwrap_or(false),
            )
            .map_err(map_compat_err)?,
    ))
}

async fn update_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<NamespaceQuery>,
    Json(body): Json<UpdateNodeBody>,
) -> CompatResult<UpdateNodeResponse> {
    let domain = query.domain.unwrap_or_else(|| "core".to_string());
    let path = query.path.unwrap_or_default();
    if path.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorDetailResponse {
                detail: "path query parameter is required".to_string(),
            }),
        ));
    }
    Ok(Json(
        state
            .service
            .update_node(
                requested_namespace(&headers),
                &domain,
                &path,
                body.content,
                body.priority,
                body.disclosure,
            )
            .map_err(map_compat_err)?,
    ))
}

async fn get_glossary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<GlossaryListResponse> {
    Ok(Json(
        state
            .service
            .list_glossary(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

async fn review_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<Vec<ReviewGroupItemResponse>> {
    Ok(Json(
        state
            .service
            .review_groups(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

async fn review_group_diff(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(node_uuid): Path<String>,
) -> CompatResult<ReviewDiffResponse> {
    Ok(Json(
        state
            .service
            .review_group_diff(requested_namespace(&headers), &node_uuid)
            .map_err(map_compat_err)?,
    ))
}

async fn review_write_not_implemented() -> CompatResult<SuccessMessageResponse> {
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorDetailResponse {
            detail: "local compatibility adapter currently exposes review inspection only"
                .to_string(),
        }),
    ))
}

async fn review_deprecated(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<ReviewDeprecatedResponse> {
    Ok(Json(
        state
            .service
            .review_deprecated(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

async fn admin_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<codex_zmemory::compat::AdminStatsResponse> {
    Ok(Json(
        state
            .service
            .admin_stats(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

async fn admin_doctor(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<codex_zmemory::compat::AdminDoctorResponse> {
    Ok(Json(
        state
            .service
            .admin_doctor(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

async fn list_orphans(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<Vec<OrphanListItemResponse>> {
    Ok(Json(
        state
            .service
            .list_orphans(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

async fn orphan_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(memory_id): Path<i64>,
) -> CompatResult<OrphanDetailResponse> {
    let detail = state
        .service
        .orphan_detail(requested_namespace(&headers), memory_id)
        .map_err(map_compat_err)?
        .ok_or_else(not_found)?;
    Ok(Json(detail))
}

async fn delete_orphan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(memory_id): Path<i64>,
) -> CompatResult<DeleteOrphanResponse> {
    Ok(Json(
        state
            .service
            .delete_orphan(requested_namespace(&headers), memory_id)
            .map_err(map_compat_err)?,
    ))
}

async fn rebuild_search(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> CompatResult<RebuildSearchResponse> {
    Ok(Json(
        state
            .service
            .rebuild_search(requested_namespace(&headers))
            .map_err(map_compat_err)?,
    ))
}

type CompatResult<T> = std::result::Result<Json<T>, (StatusCode, Json<ErrorDetailResponse>)>;

fn requested_namespace(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("X-Namespace")
        .and_then(|value| value.to_str().ok())
}

fn not_found() -> (StatusCode, Json<ErrorDetailResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorDetailResponse {
            detail: "not found".to_string(),
        }),
    )
}

fn map_compat_err(err: anyhow::Error) -> (StatusCode, Json<ErrorDetailResponse>) {
    let status = if err.to_string().contains("not found") {
        StatusCode::NOT_FOUND
    } else if err.to_string().contains("required") || err.to_string().contains("cannot") {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    (
        status,
        Json(ErrorDetailResponse {
            detail: err.to_string(),
        }),
    )
}
