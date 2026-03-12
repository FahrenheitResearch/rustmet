# Build stage
FROM rust:1.77-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p wx-server -p wx-pro -p wx-lite

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/wx-server /usr/local/bin/
COPY --from=builder /app/target/release/wx-pro /usr/local/bin/
COPY --from=builder /app/target/release/wx-lite /usr/local/bin/

# Geodata for basemaps (optional)
# COPY geodata/ /usr/local/share/wx/geodata/
# ENV WRF_GEODATA=/usr/local/share/wx/geodata

EXPOSE 8080
ENV RUST_LOG=info

CMD ["wx-server", "--port", "8080"]
