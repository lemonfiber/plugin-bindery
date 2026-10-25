# The adapter's image: built from this repository, distroless, non-root, with no
# shell and no package manager. Its health check is the binary asking itself.

FROM rust:1.99.0-alpine3.22@sha256:c3a5ad77ff2e5ec99fffaf62cb518e61a9dcbd9e1d7526d281a380db0be3bb3b AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin bindery-adapter && cp target/release/bindery-adapter /bindery-adapter

FROM gcr.io/distroless/static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab
COPY --from=build /bindery-adapter /bindery-adapter
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 CMD ["/bindery-adapter", "health"]
ENTRYPOINT ["/bindery-adapter"]
