# ohishi-logi

カメラ (SD カード CGI) の Digest 認証巡回を行う **無状態の Cloud Run 上の
Rust/Axum サービス**。`ippoan/rust-flickr` の当該実処理を、Cloudflare Workers
側で Workers runtime での MD5 対応可否がコスト高だったため、そのまま実装を移植
する形で切り出したもの。設計の背景・完了条件は
[Issue #1](https://github.com/ohishi-exp/ohishi-logi/issues/1) を参照。

## アーキテクチャ (2026-07-08 更新: データ層を Cloudflare D1+R2 に決定)

```
Cloud Scheduler / cf-flickr-cam-worker (cron trigger)
  → auth-worker (device JWT 検証 → OIDC mint、follow-up)
  → ohishi-logi (本repo, Cloud Run, Rust/Axum、Cloud Run IAM lockdown)
      → カメラ CGI (CF Access + Digest認証 + SD XML 巡回 + jpg/mp4 download)
      ← 写真バイナリを RPC レスポンスとして返す (本 repo は状態を持たない)

  cf-flickr-cam-worker (Cloudflare Worker)
      → 写真バイナリを Flickr へ OAuth1.0a multipart upload
      → Flickr access token を KV に永続化
      → cam_files 相当のメタデータ (upload 済みか等) を D1 (当日分) + R2
        (日次 JSON アーカイブ) に保持。画像バイナリ自体は Flickr が正で
        D1/R2 には置かない
```

本 repo は **無状態の camera fetcher**。カメラ写真データの永続化 (メタデータの
D1/R2 アーカイブ) は `cf-flickr-cam-worker`
([ippoan/cf-flickr-cam-worker](https://github.com/ippoan/cf-flickr-cam-worker))
側が持つ。**Flickr OAuth1.0a 認可フロー・multipart upload・access token 永続化も
Worker 側の責務** (MD5 と違い OAuth1.0a の HMAC-SHA1 は Workers runtime で問題
なく動くため)。認証は Cloud Run IAM lockdown (`--no-allow-unauthenticated` +
`run.invoker` を auth-worker の impersonate SA に限定) に委ね、本 repo 自身は
token 検証コードを持たない (rust-alc-api#434 と同方式、follow-up)。

## 現在の実装状態

| コンポーネント | 状態 | 備考 |
| --- | --- | --- |
| `src/cam.rs` (Digest認証 RFC2617/MD5 + SD XML 巡回 + download) | ✅ 移植済 | `ippoan/rust-flickr:src/cam.rs` から SOURCE-MIRROR、テスト込み |
| `GET /health`, `GET /healthz` | ✅ | 死活監視。`/healthz` は Google フロントが intercept するため外形監視には `/health` を使う |
| `GET /cam/dates` / `.../hours` / `.../files` / `.../files/{name}` (RPC) | ✅ | カメラ日付/時間/ファイル一覧・ファイル本体 download。認証コードなし (Cloud Run IAM lockdown 前提、follow-up) |
| Flickr OAuth1.0a / multipart upload / token 永続化 | ➡️ 移管 | `cf-flickr-cam-worker` (Cloudflare Worker) 側で実装 (本 repo には持たない) |
| カメラ写真メタデータの永続化 (D1/R2) | ➡️ 移管 | `cf-flickr-cam-worker` 側 (本 repo は無状態、follow-up) |
| `auth-worker` 経由の認証 (device JWT → OIDC mint → Cloud Run IAM lockdown) | ❌ 未着手 | `auth-worker` 側の新 route/role 実装が先 |
| Cloud Run デプロイ (WIF + Secret Manager + IAM lockdown) | ❌ 未着手 | 新規 GCP project の詳細 (project ID / region) 未確定 |

## 未確定事項 (次に着手する前に確認が必要)

- 新規 GCP project の詳細 (project ID・region 等) — 新規に作るか既存 project の空き枠か
- `auth-worker` 側の新 route/role 設計 (device-data-proxy の multi-backend 一般化)
- Cloud Run の GCP project / secret 名の命名規則

## ローカル開発

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo run            # → http://localhost:8080/health
PORT=3000 cargo run  # ポート変更
```

現時点では `CAM_*` env は未設定でも起動できる (boot 時 optional)。`/health` 以外の
endpoint は未実装。

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
