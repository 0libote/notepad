FROM rust:1.98-bookworm AS build
ARG CARGO_BUILD_JOBS=2
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} NO_COLOR=true
WORKDIR /app
RUN rustup target add wasm32-unknown-unknown && cargo install --locked trunk --version 0.21.14
COPY Cargo.toml Cargo.lock Trunk.toml ./
COPY crates ./crates
RUN trunk build --release && cargo build --locked --release -p notepad-server

FROM debian:bookworm-slim
RUN useradd --uid 10001 --create-home notepad && mkdir /data && chown notepad:notepad /data
WORKDIR /app
COPY --from=build /app/target/release/notepad-server /usr/local/bin/notepad-server
COPY --from=build /app/dist ./dist
USER notepad
ENV NOTEPAD_BIND=0.0.0.0:3000 NOTEPAD_DATA=/data NOTEPAD_DIST=/app/dist
VOLUME /data
EXPOSE 3000
CMD ["notepad-server"]
