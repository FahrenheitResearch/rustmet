# Storm Guardian

**An AI weather sentinel built on a full-stack pure-Rust weather platform — 50K+ lines of code, 18 crates, 5 binaries, a tile server, SDKs, and a Hermes Agent that learns what you care about and keeps you safe.**

---

## The Story

I built this to protect my dad.

He lives in rural Oklahoma — tornado alley. His internet is satellite. Weather apps chew through his data cap in a day during storm season. The NWS website times out. Radar won't load. And when a tornado warning drops at 2 AM, he might not hear his phone buzz.

I'm a meteorologist and a systems programmer. I spent a year building a weather platform from scratch — 50,000+ lines of pure Rust across 18 workspace crates. GRIB2 decoding, NEXRAD radar parsing, NWS product ingestion, XYZ tile generation, an HTTP server, MCP tools, JavaScript and Python SDKs. Five binaries, zero C dependencies, the whole thing compiles to under 20MB total.

But tools aren't enough. Someone still has to run them. Someone still has to interpret the output and decide what to do.

Storm Guardian is the missing piece. It's a Hermes Agent that sits on top of the platform and acts as a tireless weather sentinel. It checks conditions every 30 minutes. It learns what my dad cares about — mowing, spraying, burning brush. It sends plain-English briefings to his phone via Telegram. And when severe weather threatens, it escalates automatically — from gentle reminders to urgent shelter commands.

The arc: personal problem → built tools to solve it → tools became a platform → platform enables the agent → agent protects people.

**He doesn't need to understand meteorology. He just needs to trust the agent that does.**

---

## Architecture

```
                        ┌──────────────────────┐
                        │   STORM GUARDIAN      │
                        │   (Hermes Agent)      │
                        │                       │
                        │  Memory │ Skills │Cron│
                        └──────────┬───────────┘
                                   │
                        ┌──────────▼───────────┐
                        │    MCP Server         │
                        │   (wx-mcp, 20 tools)  │
                        └──────────┬───────────┘
                                   │
          ┌────────────────────────┼────────────────────────┐
          │                        │                        │
 ┌────────▼────────┐   ┌──────────▼──────────┐   ┌────────▼────────┐
 │    wx-pro        │   │    wx-server         │   │    wx-lite       │
 │  26 commands     │   │  HTTP + Tiles + SSE  │   │  bandwidth-opt   │
 │  6.0MB binary    │   │  1.7MB binary        │   │  3.6MB binary    │
 └────────┬────────┘   └──────────┬──────────┘   └────────┬────────┘
          │                        │                        │
          │            ┌──────────▼──────────┐              │
          │            │   XYZ Tile Engine    │              │
          │            │  256x256 PNG tiles   │              │
          │            │  19 colormaps        │              │
          │            │  Web Mercator proj   │              │
          │            │  512MB tile cache    │              │
          │            └─────────────────────┘              │
          │                                                 │
   ┌──────▼─────────────────────────────────────────────────▼──────┐
   │                   NOAA / NWS / AWS S3                         │
   │   HRRR • GFS • NAM • RAP • NEXRAD • MRMS • Alerts • METARs  │
   └───────────────────────────────────────────────────────────────┘

          ┌──────────────────┐     ┌──────────────────┐
          │   JS SDK (npm)   │     │  Python SDK (pip) │
          │  wx-tools        │     │  wx-tools         │
          │  Leaflet integ.  │     │  folium integ.    │
          └────────┬─────────┘     └────────┬─────────┘
                   │                         │
          ┌────────▼─────────────────────────▼─────────┐
          │              Web Dashboard                  │
          │   Leaflet map + weather overlays            │
          │   demo/index.html                           │
          └─────────────────────────────────────────────┘

          ┌──────────────────┐
          │     Telegram     │◄──── Push alerts
          │    (Gateway)     │      to user's phone
          └──────────────────┘
```

---

## Features

### Proactive Protection
- **Morning briefings** — Wake up to a plain-English weather summary every day at 6 AM
- **Severe weather escalation** — Automatic alerts that match the threat level, from watches to tornado warnings
- **Evening outlook** — Know what's coming overnight and tomorrow before bed

### Natural Language Intelligence
- "Can I spray the back 40 tomorrow?" → Checks hourly wind/precip, gives a specific window
- "Is it safe to burn brush this weekend?" → Checks Red Flag Warnings, humidity, wind forecast
- "What does the radar look like?" → Generates a map link or describes what it sees

### Adaptive Learning
- **Skill evolution** — Every new type of question becomes a reusable skill
- **User modeling** — Learns your activities, sensitivities, and preferences
- **Activity-aware advice** — "Too windy for spraying, but mowing is fine this morning"

### Bandwidth-Conscious
- **Tiered data access** — Starts with wx_brief (~50KB), only escalates when needed
- **Total event cost** — 15MB for an entire severe weather event vs 500MB+ for traditional monitoring
- **Satellite-friendly** — Designed for users with limited, expensive bandwidth

### Visual Weather (New)
- **Live map dashboard** — Full-screen interactive Leaflet map with real-time weather overlays
- **XYZ tile streaming** — Standard 256x256 transparent PNGs, compatible with any mapping library
- **Raw data layers** — Basemap-free compositing for custom visualizations
- **SSE real-time events** — Live streaming for model runs, alerts, radar updates

---

## The Platform

Storm Guardian isn't just an agent — it's built on a full weather developer platform. The agent is the most visible piece, but the platform underneath is what makes it possible.

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Agent | [Hermes Agent](https://github.com/NousResearch/hermes-agent) | Autonomous reasoning, memory, cron, skills |
| Model | Hermes 3 (Llama 3.1 70B) | Language understanding and generation |
| Weather CLI | wx-pro (Pure Rust, 50K+ LOC) | 26 commands: GRIB2, NEXRAD, MRMS, NWS |
| Tile Server | wx-server (Pure Rust) | HTTP + XYZ tiles + SSE + JSON API |
| MCP Bridge | wx-mcp (Pure Rust) | 20 tools connecting agent to weather data |
| Lite Mode | wx-lite (Pure Rust) | Bandwidth-optimized subset for satellite |
| JS SDK | npm wx-tools | WxClient, tile URLs, Leaflet integration |
| Python SDK | pip wx-tools | 12 API methods, folium map integration |
| Gateway | Telegram Bot API | Push notifications to user's phone |
| Deployment | Docker + systemd + nginx | Multi-stage build, reverse proxy, TLS |
| Data Sources | NOAA/NWS via AWS S3 | HRRR, GFS, NAM, RAP, NEXRAD, MRMS |

### Why Pure Rust?

- **5 binaries, zero C deps** — wx (4.0MB), wx-pro (6.0MB), wx-lite (3.6MB), wx-mcp (694KB), wx-server (1.7MB)
- **4.9x faster** than Python/cfgrib for HRRR download+decode (1.4s vs 7.0s)
- **Single-file deployment** — Copy one binary. No runtime. No conda. No pip.
- **Bandwidth-optimized** — HTTP byte-range requests download only the GRIB2 fields needed

---

## Setup

### Prerequisites
- [Hermes Agent](https://github.com/NousResearch/hermes-agent) installed
- `wx-mcp` binary (the MCP server wrapping wx-pro tools)
- Telegram bot token (via [@BotFather](https://t.me/BotFather))
- An OpenRouter API key (or any OpenAI-compatible endpoint)

### 1. Configure the Agent

Copy the config files and edit for your setup:

```bash
cp hackathon/config/hermes_config.yaml ~/.hermes/config.yaml
cp hackathon/config/SOUL.md ~/.hermes/SOUL.md
cp hackathon/config/USER.md ~/.hermes/USER.md
```

Edit `USER.md` with your location, preferences, and activities.

### 2. Set Environment Variables

```bash
export OPENROUTER_API_KEY="your-key-here"
export TELEGRAM_BOT_TOKEN="your-telegram-bot-token"
```

### 3. Install Skills

```bash
cp hackathon/skills/*.md ~/.hermes/skills/
```

### 4. (Optional) Start the Tile Server

```bash
wx-server --port 8080 --cache-size 512
```

This enables the live map dashboard and XYZ tile streaming for visual weather.

### 5. Start the Agent

```bash
hermes-agent --config ~/.hermes/config.yaml
```

Storm Guardian will:
- Connect to the wx-tools MCP server (20 tools)
- Load your user profile from USER.md
- Begin cron-scheduled weather checks
- Listen for Telegram messages
- Optionally point users to the live map for visual context

---

## Demo Video

**Duration:** 3 minutes

The demo opens with the live weather map, then shows Storm Guardian in action:

1. **Live Map** — Full-screen interactive weather dashboard with real-time overlays
2. **Morning Briefing** — A 6 AM Telegram message with today's forecast, translated into actionable advice
3. **Natural Language Question** — User asks "Can I spray tomorrow?" and gets a specific time window
4. **Severe Weather Escalation** — Watch → Warning → Tornado Warning, with automatic escalation at each tier

See [`demo_script.md`](demo_script.md) for the full script.

---

## Project Structure

```
hackathon/
├── README.md              # This file
├── demo_script.md         # 3-minute demo walkthrough
├── config/
│   ├── hermes_config.yaml # Hermes Agent configuration
│   ├── SOUL.md            # Agent identity and principles
│   └── USER.md            # Template user profile
└── skills/
    ├── morning_briefing.md    # Daily weather briefing skill
    ├── severe_weather_alert.md # Alert escalation skill
    ├── activity_check.md      # "Can I do X?" skill
    └── fire_weather.md        # Fire weather assessment skill
```

---

## Why This Matters

Weather kills people. Not because the data isn't available — NOAA provides some of the best weather data in the world, for free. People die because:

1. **They can't access the data** — Bandwidth, technical literacy, app overload
2. **They can't interpret the data** — SPC mesoscale discussions aren't written for farmers
3. **They're asleep** — 2 AM tornado warnings need to wake someone up

Storm Guardian solves all three. It downloads only what it needs. It translates meteorology into plain English. And it pushes alerts to your phone at any hour when the threat demands it.

And now, with the tile server and SDKs, it's not just a personal safety tool — it's a platform other developers can build on.

**Because your family's safety shouldn't depend on bandwidth.**

---

*Built for the Nous Research Hermes Agent Hackathon by Drew — meteorologist, systems programmer, and a son who wants his dad to be safe.*
