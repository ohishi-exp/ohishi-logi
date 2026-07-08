//! API エラー型。「黙って 200」禁止 — 失敗は必ず明示的な HTTP ステータス +
//! JSON body で返し、詳細は tracing に残す (ippoan/rust-flickr と同方針)。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug)]
pub enum ApiError {
    /// 400 — org ヘッダ欠落 / 不正な body 等、呼び出し側の誤り
    #[allow(dead_code)] // RPC endpoint (follow-up) で使用予定
    BadRequest(String),
    /// 424 — カメラ / Flickr API (上流) がエラーを返した
    Upstream(String),
    /// 500 — 内部エラー
    #[allow(dead_code)] // DB 層 (follow-up) で使用予定
    Internal(String),
    /// 503 — 必要な env (CAM_* / FLICKR_*) が未設定
    #[allow(dead_code)] // RPC endpoint (follow-up) で使用予定
    NotConfigured(&'static str),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Upstream(_) => StatusCode::FAILED_DEPENDENCY,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::NotConfigured(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn message(&self) -> String {
        match self {
            ApiError::BadRequest(m) => m.clone(),
            ApiError::Upstream(m) => m.clone(),
            // 内部詳細 (接続文字列等が混ざり得る) は echo しない。log にだけ出す
            ApiError::Internal(_) => "internal error".to_string(),
            ApiError::NotConfigured(what) => format!("endpoint not configured: {what} is not set"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        match &self {
            ApiError::Internal(detail) => tracing::error!(%status, detail, "request failed"),
            other => tracing::warn!(%status, ?other, "request failed"),
        }
        let body = serde_json::json!({
            "error": status.canonical_reason().unwrap_or("error"),
            "message": self.message(),
        });
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_json(err: ApiError) -> (StatusCode, serde_json::Value) {
        let res = err.into_response();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), 4096).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn statuses_are_explicit() {
        let (s, _) = body_json(ApiError::BadRequest("x".into())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let (s, _) = body_json(ApiError::Upstream("camera down".into())).await;
        assert_eq!(s, StatusCode::FAILED_DEPENDENCY);
        let (s, _) = body_json(ApiError::NotConfigured("CAM_DIGEST_USER")).await;
        assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn internal_error_does_not_echo_detail() {
        let (s, b) = body_json(ApiError::Internal("postgres://user:pass@host".into())).await;
        assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
        let msg = b["message"].as_str().unwrap();
        assert!(!msg.contains("postgres://"));
        assert_eq!(msg, "internal error");
    }
}
