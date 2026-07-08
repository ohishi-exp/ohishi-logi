# Packaging-only Dockerfile (rust-flickr / rust-alc-api と同方式)。
#
# cargo build は CI ランナー側で実行する (sccache + Swatinem/rust-cache が効く)。
# ここは musl static binary を scratch に COPY するだけ — OS レイヤなしの極小イメージ。
#
# CI は ctx/ に以下を用意して `docker build ctx` する:
#   ctx/ohishi-logi  ... x86_64-unknown-linux-musl の release binary (strip 済)
#   ctx/Dockerfile   ... 本ファイル
#
# ローカルで組む場合:
#   cargo build --release --target x86_64-unknown-linux-musl --locked
#   mkdir -p ctx && cp target/x86_64-unknown-linux-musl/release/ohishi-logi ctx/ && cp Dockerfile ctx/
#   docker build -t ohishi-logi ctx
FROM scratch
COPY ohishi-logi /ohishi-logi
EXPOSE 8080
ENTRYPOINT ["/ohishi-logi"]
