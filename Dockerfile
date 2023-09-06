FROM messense/cargo-zigbuild as builder

WORKDIR /usr/src/app
RUN rustup target add x86_64-unknown-linux-musl
COPY . .

# Will build and cache the binary and dependent crates in release mode
# RUN --mount=type=cache,target=/usr/local/cargo,from=rust:latest,source=/usr/local/cargo \
#     --mount=type=cache,target=target \
RUN cargo zigbuild --target x86_64-unknown-linux-musl --release && mv ./target/x86_64-unknown-linux-musl/release/sprite ./sprite

# Runtime image
FROM debian:bullseye-slim

# Run as "app" user
RUN useradd -ms /bin/bash app

USER app
WORKDIR /app

# Get compiled binaries from builder's cargo install directory
COPY --from=builder /usr/src/app/sprite /app/sprite

ENV DATABASE_URL="sqlite://data/sprite.db"
# Run the app
CMD ./sprite