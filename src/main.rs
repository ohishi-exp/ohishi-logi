mod cam;
mod error;
mod routes;

use cam::CamConfig;
use routes::AppState;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// env から AppState を構築する。CAM_* は **boot 時 optional**
/// (rust-flickr / secrets-inventory-gcp と同方式): 未設定でも起動は成功し、
/// 該当機能だけが未提供になる。`/health` は常に動く = secret 配線前でも
/// deploy が落ちない。
fn state_from_env() -> AppState {
    let cam = CamConfig::from_env();
    if cam.is_none() {
        tracing::warn!("CAM_* not fully set — camera scrape is unavailable");
    }

    AppState { cam }
}

#[tokio::main]
async fn main() {
    // rust-ci.yml の smoke test (`<binary> --help`) が exit 0 で返るようにする
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!("ohishi-logi {VERSION} — cam scrape backend (axum)");
        println!();
        println!("env:");
        println!("  PORT                    listen port (default: 8080, Cloud Run が注入)");
        println!("  CAM_DIGEST_USER         camera digest auth user");
        println!("  CAM_DIGEST_PASS         camera digest auth password");
        println!("  CAM_MACHINE_NAME        camera machine name");
        println!("  CAM_SDCARD_CGI          camera SD-card listing CGI base URL");
        println!("  CAM_MP4_CGI             camera mp4 download CGI base URL");
        println!("  CAM_JPG_CGI             camera jpg download CGI base URL");
        println!("  CAM_CF_ACCESS_CLIENT_ID     CF Access service token id (optional)");
        println!("  CAM_CF_ACCESS_CLIENT_SECRET CF Access service token secret (optional)");
        return;
    }

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let state = state_from_env();

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|e| panic!("failed to bind 0.0.0.0:{port}: {e}"));

    tracing::info!("ohishi-logi {VERSION} listening on 0.0.0.0:{port}");

    axum::serve(listener, routes::app(state))
        .await
        .expect("server error");
}
