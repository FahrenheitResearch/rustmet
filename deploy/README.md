# wx-server Deployment

## Docker (recommended)

```bash
# From the rustmet root directory:
docker compose -f deploy/docker-compose.yml up -d

# Check health:
curl http://localhost:8080/health

# View logs:
docker compose -f deploy/docker-compose.yml logs -f
```

## Docker Build Only

```bash
docker build -f deploy/Dockerfile -t wx-server .
docker run -p 8080:8080 wx-server
```

## Systemd (bare metal)

```bash
# Build
cargo build --release -p wx-server -p wx-pro -p wx-lite

# Install binaries
sudo cp target/release/wx-server /usr/local/bin/
sudo cp target/release/wx-pro /usr/local/bin/
sudo cp target/release/wx-lite /usr/local/bin/

# Create service user
sudo useradd -r -s /bin/false wx

# Install service
sudo cp deploy/systemd/wx-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now wx-server

# Check status
sudo systemctl status wx-server
curl http://localhost:8080/health
```

## Nginx Reverse Proxy (optional)

```nginx
server {
    listen 443 ssl http2;
    server_name api.yourweathersite.com;

    ssl_certificate /etc/letsencrypt/live/api.yourweathersite.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.yourweathersite.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;

        # SSE support
        proxy_set_header Connection '';
        proxy_http_version 1.1;
        proxy_buffering off;
        proxy_cache off;
        chunked_transfer_encoding off;
    }

    # Tile caching
    location /tiles/ {
        proxy_pass http://127.0.0.1:8080;
        proxy_cache_valid 200 5m;
        add_header X-Cache-Status $upstream_cache_status;
    }
}
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |
| `WRF_GEODATA` | — | Path to Natural Earth geodata for basemaps |

## Resource Requirements

- **Minimum**: 512MB RAM, 1 CPU core
- **Recommended**: 2GB RAM, 2 CPU cores
- **Disk**: ~50MB for binaries, tiles cached to `~/.wx-pro/tiles/`
- **Network**: Outbound HTTPS to NOMADS, AWS S3, aviationweather.gov, api.weather.gov
