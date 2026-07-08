# CLAUDE.md

`ohishi-logi` — カメラ SD カード scrape (Digest 認証巡回) + Flickr upload の実処理
backend。Cloud Run (Rust/Axum) で稼働し、`ippoan/cf-flickr-cam-worker` の cron
trigger / 薄い proxy から RPC で呼ばれる想定。設計の背景は
[Issue #1](https://github.com/ohishi-exp/ohishi-logi/issues/1) を参照。

移植元: `ippoan/rust-flickr` (Digest 認証 / SD XML 巡回 / Flickr OAuth1 の実装元。
cutover 確認後に rust-flickr 側の該当コードは撤去予定)。

## 現状 / 未着手

- ✅ repo scaffold (Cargo.toml / Dockerfile / CI)
- ✅ `src/cam.rs` (Digest 認証 RFC2617 + SD XML 巡回) / `src/oauth1.rs` (OAuth1.0a
  署名) / `src/flickr.rs` (Flickr API クライアント) を rust-flickr から移植
  (`SOURCE-MIRROR` 明記、テスト込み)
- ✅ `/health` のみの axum skeleton
- ❌ **未着手 (Issue #1 の未確定事項が解消してから)**: 新規 GCP project 作成 /
  Supabase 新 schema 設計 / DB 層 (`db.rs`) / `cf-flickr-cam-worker` から呼ばれる
  RPC endpoint / SD_ZOMBIE センチネル・upload floor 計算 / Cloud Run デプロイ配線
  (WIF・secret Manager)。詳細は Issue #1 の「前提 / 未確定事項」を参照

## ビルド / テスト

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

`Cargo.lock` は commit する (CI は `--locked` でビルド)。

## GitHub 自動化

- `main` に直接 push しない。PR を作る。
- PR / commit は `Refs #N` を使う (`Closes/Fixes/Resolves` は禁止 — auto-close 防止)。
- `mcp__github__enable_pr_auto_merge` を reflex で呼ばない (user 明示指示時のみ)。
- PR 作成後は同じ turn で `subscribe_pr_activity` を呼び CI を watch する。
- cross-org (ohishi-exp) の `ippoan/ci-workflows` 呼び出しは `secrets: inherit`
  不可 — `CI_APP_ID` / `CI_APP_PRIVATE_KEY` を named secret で明示渡し
  (`.github/workflows/ci.yml` 参照)。

---

_共通項を直すときは [`ippoan/claude-md`](https://github.com/ippoan/claude-md) の
`CLAUDE.md.template` を更新すること。_
