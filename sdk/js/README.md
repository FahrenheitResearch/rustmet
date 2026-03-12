# wx-tools

Weather data for developers. HRRR, GFS, RAP, NAM tiles + JSON API. Zero API keys. Pure Rust backend.

## Quick Start

```bash
npm install wx-tools
```

```javascript
import { WxClient } from 'wx-tools';

const wx = new WxClient('http://localhost:8080');

// Get conditions
const conditions = await wx.conditions(35.22, -97.44);

// METAR
const metar = await wx.metar('KOKC');

// Scan for highest CAPE in CONUS
const hotspots = await wx.scan('cape', { mode: 'max', topN: 10 });
```

## Map Tiles (Leaflet)

```javascript
L.tileLayer(wx.tileUrl('hrrr', 'cape', 'surface', 0), {
    opacity: 0.7
}).addTo(map);
```

## Available Variables

cape, refc (reflectivity), temp, dewpoint, rh, gust, wind_u, wind_v, helicity, uh, precip, cloud, snow, vis, pwat, mslp

## Models

| Model | Resolution | Coverage | Update |
|-------|-----------|----------|--------|
| HRRR  | 3km       | CONUS    | Hourly |
| RAP   | 13km      | N. America | Hourly |
| GFS   | 0.25deg   | Global   | 6-hourly |
| NAM   | 12km      | N. America | 6-hourly |
