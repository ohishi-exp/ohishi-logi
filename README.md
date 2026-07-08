# ohishi-logi

カメラ (SD カード CGI) の Digest 認証巡回 + Flickr upload を行う **Cloud Run 上の
Rust/Axum サービス**。`ippoan/rust-flickr` の当該実処理を、Cloudflare Workers 側で
Supabase 直接接続と Workers runtime での MD5 対応可否の 2 つを解消するコストが高い
と判断し、そのまま実装を移植する形で切り出したもの。設計の背景・完了条件は
[Issue #1](https://github.com/ohishi-exp/ohishi-logi/issues/1) を参照。

## アーキテクチャ (最終形、Issue #1 より)

```
Cloud Scheduler / cf-flickr-cam-worker (cron trigger, 薄い proxy)
  → ohishi-logi (本repo, Cloud Run, Rust/Axum)
      → カメラ CGI (CF Access + Digest認証 + SD XML 巡回 + jpg/mp4 download)
      → Flickr OAuth1 multipart upload
      → Supabase Postgres (新 schema、既存 `logi` スキーマとは独立)
```

`cf-flickr-cam-worker` ([ippoan/cf-flickr-cam-worker](https://github.com/ippoan/cf-flickr-cam-worker))
は cron trigger / 薄い proxy 層として残り、実処理は本 repo の RPC を呼ぶ。

## 現在の実装状態

このセッションでは、GCP project / Supabase schema といった**未確定のインフラ意思決定を
要する部分は含めず**、決定に依存しない移植可能なロジックのみを先行して切り出した:

| コンポーネント | 状態 | 備考 |
| --- | --- | --- |
| `src/cam.rs` (Digest認証 RFC2617/MD5 + SD XML 巡回 + download) | ✅ 移植済 | `ippoan/rust-flickr:src/cam.rs` から SOURCE-MIRROR、テスト込み |
| `src/oauth1.rs` (OAuth1.0a HMAC-SHA1 署名) | ✅ 移植済 | 同上 (`src/oauth1.rs`) |
| `src/flickr.rs` (Flickr API クライアント、OAuth token 取得 + multipart upload) | ✅ 移植済 | 同上 (`src/flickr.rs`)。token 永続化は含まない (DB 層は follow-up) |
| `GET /health`, `GET /healthz` | ✅ | 死活監視。`/healthz` は Google フロントが intercept するため外形監視には `/health` を使う |
| DB 層 (`db.rs`)・新 schema | ❌ 未着手 | Supabase 新 schema 設計 (未確定) 待ち |
| `cf-flickr-cam-worker` から呼ばれる RPC endpoint (`/sync` 相当) | ❌ 未着手 | 認証方式・DB スキーマ確定後 |
| Cloud Run デプロイ (WIF + Secret Manager) | ❌ 未着手 | 新規 GCP project の詳細 (project ID / region) 未確定 |
| Flickr access token の Supabase 永続化 | ❌ 未着手 | schema 設計待ち |

## 未確定事項 (Issue #1 より、次に着手する前に確認が必要)

- 新規 GCP project の詳細 (project ID・region 等) — 新規に作るか既存 project の空き枠か
- Supabase 新 schema の名称・DB ロール設計 (`ohishi_logi_app` 等)
- 既存 `cam_files` (logi スキーマ、258,078行) のデータ移行方針
- `cf-flickr-cam-worker` との RPC 境界の詳細設計 (認証方式・endpoint 仕様)
- Cloud Run の GCP project / secret 名の命名規則

## ローカル開発

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo run            # → http://localhost:8080/health
PORT=3000 cargo run  # ポート変更
```

現時点では `CAM_*` / `FLICKR_*` env は未設定でも起動できる (boot 時 optional)。
`/health` 以外の endpoint は未実装。

## イメージ (scratch + musl static)

`Dockerfile` は packaging-only (`rust-flickr` / `rust-alc-api` と同方式)。cargo build
は CI ランナー側で行う。

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl --locked
mkdir -p ctx && cp target/x86_64-unknown-linux-musl/release/ohishi-logi ctx/ && cp Dockerfile ctx/
docker build -t ohishi-logi ctx
docker run --rm -p 8080:8080 ohishi-logi
```

## CI

`ippoan/ci-workflows` の `rust-ci.yml` reusable (fmt / clippy / test / build) +
`auto-merge.yml` (cross-org named secret 渡し)。Cloud Run デプロイ job は GCP project
確定後に追加する。
