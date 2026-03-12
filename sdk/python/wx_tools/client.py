"""wx-tools Python SDK — Weather data for developers."""

import requests
from typing import Optional, Dict, Any, List


class WxClient:
    """Client for the wx-tools weather API.

    Usage:
        wx = WxClient("http://localhost:8080")
        conditions = wx.conditions(35.22, -97.44)
        tile_url = wx.tile_url("hrrr", "cape")
    """

    def __init__(self, base_url: str = "http://localhost:8080", timeout: int = 30):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self._session = requests.Session()

    # ─── Tile URL builders ───

    def tile_url(self, model: str, var: str, level: str = "surface", fhour: int = 0) -> str:
        """Get tile URL template with {z}/{x}/{y} placeholders.

        Use with folium, ipyleaflet, or any mapping library:
            tile_url = wx.tile_url("hrrr", "cape")
            folium.TileLayer(tiles=tile_url, attr="wx-tools").add_to(m)
        """
        return f"{self.base_url}/tiles/{model}/{var}/{level}/f{fhour:02d}/{{z}}/{{x}}/{{y}}.png"

    def radar_tile_url(self, fhour: int = 0) -> str:
        """HRRR composite reflectivity tile URL template."""
        return self.tile_url("hrrr", "refc", "surface", fhour)

    def cape_tile_url(self, fhour: int = 0) -> str:
        """HRRR CAPE tile URL template."""
        return self.tile_url("hrrr", "cape", "surface", fhour)

    def temp_tile_url(self, fhour: int = 0) -> str:
        """HRRR 2m temperature tile URL template."""
        return self.tile_url("hrrr", "temp", "2m", fhour)

    # ─── JSON API methods ───

    def conditions(self, lat: float, lon: float) -> Dict[str, Any]:
        """Current conditions (METAR + alerts + station)."""
        return self._get("/api/conditions", lat=lat, lon=lon)

    def forecast(self, lat: float, lon: float, hourly: bool = False) -> Dict[str, Any]:
        """NWS 7-day or hourly forecast (US only)."""
        params = {"lat": lat, "lon": lon}
        if hourly:
            params["hourly"] = "true"
        return self._get("/api/forecast", **params)

    def alerts(self, lat: Optional[float] = None, lon: Optional[float] = None,
               state: Optional[str] = None) -> Dict[str, Any]:
        """Active NWS weather alerts."""
        params = {}
        if lat is not None:
            params["lat"] = lat
        if lon is not None:
            params["lon"] = lon
        if state:
            params["state"] = state
        return self._get("/api/alerts", **params)

    def metar(self, station: str) -> Dict[str, Any]:
        """Current METAR observation."""
        return self._get("/api/metar", station=station)

    def radar(self, site: Optional[str] = None, lat: Optional[float] = None,
              lon: Optional[float] = None) -> Dict[str, Any]:
        """NEXRAD radar volume scan data."""
        params = {}
        if site:
            params["site"] = site
        if lat is not None:
            params["lat"] = lat
        if lon is not None:
            params["lon"] = lon
        return self._get("/api/radar", **params)

    def scan(self, var: str, model: str = "hrrr", mode: str = "max",
             level: str = "surface", top_n: int = 10) -> Dict[str, Any]:
        """Scan model grid for extreme values."""
        return self._get("/api/scan", var=var, model=model, mode=mode,
                         level=level, top_n=top_n)

    def point(self, lat: float, lon: float, model: str, var: str,
              level: str = "surface") -> Dict[str, Any]:
        """Single model variable at a point."""
        return self._get("/api/point", lat=lat, lon=lon, model=model,
                         var=var, level=level)

    def severe(self, lat: Optional[float] = None, lon: Optional[float] = None,
               state: Optional[str] = None) -> Dict[str, Any]:
        """SPC severe weather assessment."""
        params = {}
        if lat is not None:
            params["lat"] = lat
        if lon is not None:
            params["lon"] = lon
        if state:
            params["state"] = state
        return self._get("/api/severe", **params)

    def evidence(self, lat: float, lon: float) -> Dict[str, Any]:
        """Multi-source evidence and confidence assessment."""
        return self._get("/api/evidence", lat=lat, lon=lon)

    def timeseries(self, lat: float, lon: float, var: str,
                   model: str = "hrrr", level: str = "surface",
                   hours: int = 18) -> Dict[str, Any]:
        """Time evolution of a variable at a point."""
        return self._get("/api/timeseries", lat=lat, lon=lon, var=var,
                         model=model, level=level, hours=hours)

    def health(self) -> Dict[str, Any]:
        """Server health check."""
        return self._get("/health")

    # ─── Tile download (for offline/batch) ───

    def download_tile(self, model: str, var: str, z: int, x: int, y: int,
                      level: str = "surface", fhour: int = 0) -> bytes:
        """Download a single tile as PNG bytes."""
        url = f"{self.base_url}/tiles/{model}/{var}/{level}/f{fhour:02d}/{z}/{x}/{y}.png"
        resp = self._session.get(url, timeout=self.timeout)
        resp.raise_for_status()
        return resp.content

    # ─── Internal ───

    def _get(self, path: str, **params) -> Dict[str, Any]:
        url = f"{self.base_url}{path}"
        resp = self._session.get(url, params=params, timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()
