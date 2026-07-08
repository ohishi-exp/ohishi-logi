# CLAUDE.md

`ohishi-logi` — カメラ SD カード scrape (Digest 認証巡回) の実処理 backend。Cloud
Run (Rust/Axum) で稼働し、`ippoan/cf-flickr-cam-worker` の cron trigger / 薄い
proxy から RPC で呼ばれる想定。設計の背景・現状・未着手事項は
[README.md](./README.md) と [Issue #1](https://github.com/ohishi-exp/ohishi-logi/issues/1)
を参照。

移植元: `ippoan/rust-flickr` (Digest 認証 / SD XML 巡回の実装元)。

## repo 固有の invariant

- **Flickr OAuth1.0a / multipart upload / access token 永続化は本 repo に置かない**
  — `cf-flickr-cam-worker` (Cloudflare Worker) 側の責務 (MD5 と違い HMAC-SHA1 は
  Workers runtime で動くため、2026-07-08 方針決定)。本 repo はカメラ側 (Digest
  認証・SD 巡回・ファイル本体 download) のみ持つ。

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
- cross-org (ohishi-exp) の `ippoan/ci-workflows` 呼び出しは `secrets: inherit`
  不可 — `CI_APP_ID` / `CI_APP_PRIVATE_KEY` を named secret で明示渡し。

---

_共通項を直すときは [`ippoan/claude-md`](https://github.com/ippoan/claude-md) の
`CLAUDE.md.template` を更新すること。_
