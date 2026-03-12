# wx-tools

Weather data for Python developers. HRRR, GFS, RAP, NAM tiles + JSON API. Zero API keys.

## Install

```bash
pip install wx-tools
```

## Quick Start

```python
from wx_tools import WxClient

wx = WxClient("http://localhost:8080")

# Current conditions
conditions = wx.conditions(35.22, -97.44)

# METAR
metar = wx.metar("KOKC")

# Scan for highest CAPE in CONUS
hotspots = wx.scan("cape", mode="max", top_n=10)

# Time series of temperature
ts = wx.timeseries(35.22, -97.44, "temp", level="2m")
```

## Map Tiles (Folium)

```python
import folium

m = folium.Map(location=[39, -98], zoom_start=5, tiles="cartodbdark_matter")
folium.TileLayer(
    tiles=wx.tile_url("hrrr", "cape"),
    attr="wx-tools",
    opacity=0.7
).addTo(m)
m.save("weather_map.html")
```

## Available Variables

cape, refc, temp, dewpoint, rh, gust, wind_u, wind_v, helicity, uh, precip, cloud, snow, vis, pwat, mslp

## Models

| Model | Resolution | Coverage | Update |
|-------|-----------|----------|--------|
| HRRR  | 3km       | CONUS    | Hourly |
| RAP   | 13km      | N. America | Hourly |
| GFS   | 0.25°     | Global   | 6-hourly |
| NAM   | 12km      | N. America | 6-hourly |
