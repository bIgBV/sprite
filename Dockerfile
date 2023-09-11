FROM messense/cargo-zigbuild as builder

WORKDIR /usr/src/app
RUN rustup target add x86_64-unknown-linux-musl
COPY . .

RUN cargo install sqlx-cli
RUN cargo zigbuild --target x86_64-unknown-linux-musl --release && mv ./target/x86_64-unknown-linux-musl/release/sprite ./sprite

# Runtime image
FROM debian:bullseye-slim

# Run as "app" user
# RUN useradd -ms /bin/bash app
# RUN usermod -aG sudo app

# USER app
WORKDIR /app

# Get compiled binaries from builder's cargo install directory
COPY --from=builder /usr/src/app/sprite /app/sprite
COPY --from=builder /usr/src/app/entrypoint.sh /app/
COPY /assets/ /app/assets/
COPY --from=builder /usr/local/cargo/bin/sqlx /app/sqlx

ENV DATABASE_URL="sqlite:///data/sprite.db"
RUN chmod +x /app/entrypoint.sh

# Run the app
CMD ["/app/entrypoint.sh"]