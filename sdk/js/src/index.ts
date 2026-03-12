/**
 * wx-tools — Weather data SDK
 *
 * Usage:
 *   const wx = new WxClient("http://localhost:8080");
 *   const conditions = await wx.conditions(35.22, -97.44);
 *   const tileUrl = wx.tileUrl("hrrr", "cape", "surface", 0);
 */

export interface WxConfig {
  baseUrl: string;
  timeout?: number;
}

export class WxClient {
  private baseUrl: string;
  private timeout: number;

  constructor(baseUrl: string = "http://localhost:8080", config?: Partial<WxConfig>) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.timeout = config?.timeout ?? 30000;
  }

  // ─── Tile URL builders (for Leaflet/Mapbox/Google Maps) ───

  /** Get tile URL template for use with Leaflet/Mapbox
   *  Returns: "http://host/tiles/hrrr/cape/surface/f00/{z}/{x}/{y}.png"
   */
  tileUrl(model: string, variable: string, level: string = "surface", fhour: number = 0): string {
    return `${this.baseUrl}/tiles/${model}/${variable}/${level}/f${String(fhour).padStart(2, '0')}/{z}/{x}/{y}.png`;
  }

  /** Convenience: HRRR composite reflectivity tiles */
  radarTileUrl(fhour: number = 0): string {
    return this.tileUrl("hrrr", "refc", "surface", fhour);
  }

  /** Convenience: HRRR CAPE tiles */
  capeTileUrl(fhour: number = 0): string {
    return this.tileUrl("hrrr", "cape", "surface", fhour);
  }

  /** Convenience: HRRR 2m temperature tiles */
  tempTileUrl(fhour: number = 0): string {
    return this.tileUrl("hrrr", "temp", "2m", fhour);
  }

  // ─── JSON API methods ───

  async conditions(lat: number, lon: number): Promise<any> {
    return this.get(`/api/conditions?lat=${lat}&lon=${lon}`);
  }

  async forecast(lat: number, lon: number, hourly: boolean = false): Promise<any> {
    return this.get(`/api/forecast?lat=${lat}&lon=${lon}${hourly ? '&hourly=true' : ''}`);
  }

  async alerts(options: { lat?: number; lon?: number; state?: string }): Promise<any> {
    const params = new URLSearchParams();
    if (options.lat !== undefined) params.set("lat", String(options.lat));
    if (options.lon !== undefined) params.set("lon", String(options.lon));
    if (options.state) params.set("state", options.state);
    return this.get(`/api/alerts?${params}`);
  }

  async metar(station: string): Promise<any> {
    return this.get(`/api/metar?station=${station}`);
  }

  async radar(options: { site?: string; lat?: number; lon?: number }): Promise<any> {
    const params = new URLSearchParams();
    if (options.site) params.set("site", options.site);
    if (options.lat !== undefined) params.set("lat", String(options.lat));
    if (options.lon !== undefined) params.set("lon", String(options.lon));
    return this.get(`/api/radar?${params}`);
  }

  async scan(variable: string, options?: { model?: string; mode?: string; level?: string; topN?: number }): Promise<any> {
    const params = new URLSearchParams({ var: variable });
    if (options?.model) params.set("model", options.model);
    if (options?.mode) params.set("mode", options.mode);
    if (options?.level) params.set("level", options.level);
    if (options?.topN) params.set("top_n", String(options.topN));
    return this.get(`/api/scan?${params}`);
  }

  async point(lat: number, lon: number, model: string, variable: string, level: string = "surface"): Promise<any> {
    return this.get(`/api/point?lat=${lat}&lon=${lon}&model=${model}&var=${variable}&level=${level}`);
  }

  async severe(options: { lat?: number; lon?: number; state?: string }): Promise<any> {
    const params = new URLSearchParams();
    if (options.lat !== undefined) params.set("lat", String(options.lat));
    if (options.lon !== undefined) params.set("lon", String(options.lon));
    if (options.state) params.set("state", options.state);
    return this.get(`/api/severe?${params}`);
  }

  async health(): Promise<any> {
    return this.get("/health");
  }

  // ─── Internal ───

  private async get(path: string): Promise<any> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.timeout);
    try {
      const resp = await fetch(`${this.baseUrl}${path}`, { signal: controller.signal });
      if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${await resp.text()}`);
      return resp.json();
    } finally {
      clearTimeout(timeout);
    }
  }
}

// Default export for quick use
export default WxClient;
