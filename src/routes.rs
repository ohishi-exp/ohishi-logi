//! axum ルーティング (Refs ohishi-exp/ohishi-logi#1)。
//!
//! `cf-flickr-cam-worker` から呼ばれる RPC endpoint (カメラ日付/時間/ファイル
//! 一覧・ファイル本体 download)。本 repo は無状態の camera fetcher —
//! カメラ写真データの永続化 (D1/R2 日次アーカイブ) は `cf-flickr-cam-worker`
//! 側が持つ (2026-07-08 方針決定)。認証は Cloud Run IAM lockdown
//! (`--no-allow-unauthenticated` + `run.invoker` を auth-worker の impersonate
//! SA に限定) に委ねる — 本 repo 自身は token 検証コードを持たない
//! (rust-alc-api#434 と同方式)。
//!
//! OAuth1.0a 認可フロー・Flickr multipart upload・token 永続化 (KV) は
//! `cf-flickr-cam-worker` 側の責務 (MD5 と違い HMAC-SHA1 は Workers runtime で
//! 問題なく動くため)。

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::cam::CamClient;
use crate::error::ApiError;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct AppState {
    pub cam: Option<CamClient>,
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "ohishi-logi",
        "version": VERSION,
    }))
}

fn require_cam(state: &AppState) -> Result<&CamClient, ApiError> {
    state.cam.as_ref().ok_or(ApiError::NotConfigured("CAM_*"))
}

/// SD カードのディレクトリ/ファイル名は `YYYYMMDD` (date) / `HHMMSS` (hour) /
/// `Event<...>.(jpg|mp4)` (name) の固定形式 (カメラ CGI 由来)。この形式から
/// 外れる値をそのままカメラ CGI の URL に埋め込むと path traversal / request
/// smuggling の入り口になるため、RPC 境界でバリデーションする。
fn valid_date(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())
}

fn valid_hour(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_digit())
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && !s.contains('/')
        && !s.contains('\\')
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

async fn list_dates(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let cam = require_cam(&state)?;
    let dates = cam.list_dates().await?;
    Ok(Json(json!({ "dates": dates })))
}

async fn list_hours(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let cam = require_cam(&state)?;
    if !valid_date(&date) {
        return Err(ApiError::BadRequest(format!("invalid date: {date}")));
    }
    let hours = cam.list_hours(&date).await?;
    Ok(Json(json!({ "hours": hours })))
}

async fn list_files(
    State(state): State<AppState>,
    Path((date, hour)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let cam = require_cam(&state)?;
    if !valid_date(&date) {
        return Err(ApiError::BadRequest(format!("invalid date: {date}")));
    }
    if !valid_hour(&hour) {
        return Err(ApiError::BadRequest(format!("invalid hour: {hour}")));
    }
    let files = cam.list_file_names(&date, &hour).await?;
    Ok(Json(json!({ "files": files })))
}

async fn download_file(
    State(state): State<AppState>,
    Path((date, hour, name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let cam = require_cam(&state)?;
    if !valid_date(&date) {
        return Err(ApiError::BadRequest(format!("invalid date: {date}")));
    }
    if !valid_hour(&hour) {
        return Err(ApiError::BadRequest(format!("invalid hour: {hour}")));
    }
    if !valid_name(&name) {
        return Err(ApiError::BadRequest(format!("invalid file name: {name}")));
    }
    let bytes = cam
        .download(&name, &date, &hour)
        .await
        .map_err(ApiError::Upstream)?;
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        Bytes::from(bytes),
    )
        .into_response())
}

pub fn app(state: AppState) -> Router {
    Router::new()
        // Cloud Run の Google フロントが `/healthz` をインターセプトして汎用 404
        // を返すため、外形監視は `/health` を使う (rust-flickr / gcp-cloud-run-
        // routing-traps skill と同じ運用)。`/healthz` はコンテナ内 / GCP 内部
        // 経路用に残す。
        .route("/health", get(health))
        .route("/healthz", get(health))
        .route("/cam/dates", get(list_dates))
        .route("/cam/dates/{date}/hours", get(list_hours))
        .route("/cam/dates/{date}/hours/{hour}/files", get(list_files))
        .route(
            "/cam/dates/{date}/hours/{hour}/files/{name}",
            get(download_file),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::cam::CamConfig;

    use super::*;

    fn empty_state() -> AppState {
        AppState { cam: None }
    }

    async fn state_against(server: &MockServer) -> AppState {
        let config = CamConfig {
            digest_user: "user".to_string(),
            digest_pass: "pass".to_string(),
            machine_name: "cam1".to_string(),
            sdcard_cgi: format!("{}/sd/", server.uri()),
            mp4_cgi: format!("{}/mp4/", server.uri()),
            jpg_cgi: format!("{}/jpg/", server.uri()),
            cf_access_client_id: None,
            cf_access_client_secret: None,
        };
        AppState {
            cam: Some(CamClient::new(config)),
        }
    }

    async fn get(state: AppState, uri: &str) -> Response {
        app(state)
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn health_returns_ok_status() {
        let response = get(empty_state(), "/health").await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "ohishi-logi");
    }

    #[tokio::test]
    async fn healthz_matches_health() {
        let response = get(empty_state(), "/healthz").await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn cam_routes_return_503_when_not_configured() {
        for uri in [
            "/cam/dates",
            "/cam/dates/20260101/hours",
            "/cam/dates/20260101/hours/120000/files",
            "/cam/dates/20260101/hours/120000/files/a.jpg",
        ] {
            let response = get(empty_state(), uri).await;
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        }
    }

    #[tokio::test]
    async fn list_dates_returns_camera_dates() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sd/cam1/Event"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"<List><Dir name="20260101"/></List>"#),
            )
            .mount(&server)
            .await;
        let response = get(state_against(&server).await, "/cam/dates").await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["dates"], json!(["20260101"]));
    }

    #[tokio::test]
    async fn list_hours_rejects_malformed_date() {
        let response = get(
            state_against(&MockServer::start().await).await,
            "/cam/dates/not-a-date/hours",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_hours_returns_camera_hours() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sd/cam1/Event/20260101"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"<List><Dir name="120000"/></List>"#),
            )
            .mount(&server)
            .await;
        let response = get(state_against(&server).await, "/cam/dates/20260101/hours").await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["hours"], json!(["120000"]));
    }

    #[tokio::test]
    async fn list_files_rejects_malformed_hour() {
        let response = get(
            state_against(&MockServer::start().await).await,
            "/cam/dates/20260101/hours/bad/files",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_files_returns_camera_files() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sd/cam1/Event/20260101/120000"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"<List><File><Name>Event20260101_120000.jpg</Name></File></List>"#,
            ))
            .mount(&server)
            .await;
        let response = get(
            state_against(&server).await,
            "/cam/dates/20260101/hours/120000/files",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["files"], json!(["Event20260101_120000.jpg"]));
    }

    #[tokio::test]
    async fn download_file_rejects_path_traversal_name() {
        let response = get(
            state_against(&MockServer::start().await).await,
            "/cam/dates/20260101/hours/120000/files/..%2f..%2fetc%2fpasswd",
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn download_file_streams_camera_bytes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jpg/cam1/Event/20260101/120000/a.jpg"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .set_body_bytes(vec![1u8, 2, 3]),
            )
            .mount(&server)
            .await;
        let response = get(
            state_against(&server).await,
            "/cam/dates/20260101/hours/120000/files/a.jpg",
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/octet-stream"
        );
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(bytes.as_ref(), &[1u8, 2, 3]);
    }

    #[tokio::test]
    async fn download_file_propagates_upstream_error_as_424() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jpg/cam1/Event/20260101/120000/a.jpg"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string("<html>login page</html>"),
            )
            .mount(&server)
            .await;
        let response = get(
            state_against(&server).await,
            "/cam/dates/20260101/hours/120000/files/a.jpg",
        )
        .await;
        assert_eq!(response.status(), StatusCode::FAILED_DEPENDENCY);
    }
}
