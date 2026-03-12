# Web Dashboard

`index.html` is a self-contained weather dashboard that connects to `wx-server`. Open it in a browser after starting the tile server:

```bash
wx-server --port 8080 --cache-size 512
```

The dashboard uses Leaflet with real-time weather overlays (radar, model data, warning polygons) served as standard XYZ tiles. No build step required — just open the HTML file or serve it from any static file host.
