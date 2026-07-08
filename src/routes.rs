//! axum ルーティング。現時点では死活監視のみ (Refs ohishi-exp/ohishi-logi#1)。
//!
//! `cf-flickr-cam-worker` から呼ばれる RPC endpoint (カメラ日付/時間/ファイル
//! 一覧・ファイル本体 download) は DB スキーマ設計 (Issue #1 の未確定事項) が
//! 決まってから追加する。OAuth1.0a 認可フロー・Flickr multipart upload・token
//! 永続化 (KV) は `cf-flickr-cam-worker` 側の責務に移した (MD5 と違い HMAC-SHA1
//! は Workers runtime で問題なく動くため)。

use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::cam::CamConfig;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)] // RPC endpoint (follow-up) で使用予定
    pub cam: Option<CamConfig>,
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "ohishi-logi",
        "version": VERSION,
    }))
}

pub fn app(state: AppState) -> Router {
    Router::new()
        // Cloud Run の Google フロントが `/healthz` をインターセプトして汎用 404
        // を返すため、外形監視は `/health` を使う (rust-flickr / gcp-cloud-run-
        // routing-traps skill と同じ運用)。`/healthz` はコンテナ内 / GCP 内部
        // 経路用に残す。
        .route("/health", get(health))
        .route("/healthz", get(health))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;

    fn empty_state() -> AppState {
        AppState { cam: None }
    }

    #[tokio::test]
    async fn health_returns_ok_status() {
        let response = app(empty_state())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "ohishi-logi");
    }

    #[tokio::test]
    async fn healthz_matches_health() {
        let response = app(empty_state())
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
