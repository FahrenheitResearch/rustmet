# Storm Guardian — AI Weather Agent Example

Deploy an autonomous AI weather agent using [Hermes Agent](https://github.com/NousResearch/hermes-agent) and rustmet's `wx-mcp` tool server. The agent monitors weather conditions on a cron schedule, answers natural-language questions about outdoor activities, and escalates severe weather alerts via Telegram.

## Architecture

```
┌─────────────────────────────┐
│      Hermes Agent           │
│  (Nemotron-3-Super / vLLM)  │
│                             │
│  Memory │ Skills │ Cron     │
└────────────┬────────────────┘
             │ MCP (stdio)
┌────────────▼────────────────┐
│        wx-mcp               │
│   20 weather tools          │
└────────────┬────────────────┘
             │
┌────────────▼────────────────┐
│  wx-pro / wx-lite           │
│  GRIB2, NEXRAD, NWS, MRMS  │
└────────────┬────────────────┘
             │
┌────────────▼────────────────┐
│  NOAA / NWS / AWS S3        │
│  HRRR  GFS  NAM  RAP       │
│  NEXRAD  MRMS  Alerts       │
└─────────────────────────────┘

        ┌───────────┐
        │ Telegram  │◄── Push alerts
        │ (Gateway) │    to user
        └───────────┘
```

## Prerequisites

- **Hermes Agent** — [github.com/NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent)
- **wx-mcp** binary — the MCP server that exposes rustmet's weather tools
- **wx-pro** or **wx-lite** binary — the underlying weather CLI (wx-mcp delegates to these)
- **LLM endpoint** — any OpenAI-compatible API (vLLM, Ollama, OpenRouter, etc.)
- **Telegram bot token** — create one via [@BotFather](https://t.me/BotFather)

### Hardware (self-hosted LLM)

The default config uses Nemotron-3-Super-120B via vLLM:

| Setup | GPU | VRAM | Notes |
|-------|-----|------|-------|
| Single GPU | 1x H200 | 141 GB | FP8 quantization, fits in one card |
| Multi-GPU | 2x H100-80GB | 160 GB | Tensor parallel across two cards |
| Budget | 2x A100-80GB | 160 GB | Slower but works with FP8 |

Smaller models (70B, 8B) work on consumer hardware but reduce agent quality. Any OpenAI-compatible endpoint works — adjust `base_url` and `model` in the config.

## Quick Start

### 1. Set environment variables

```bash
# For vLLM, any non-empty string works as the key
export OPENAI_API_KEY="not-needed"

# Telegram push notifications
export TELEGRAM_BOT_TOKEN="your-bot-token"
export TELEGRAM_CHAT_ID="your-chat-id"
```

### 2. Copy and edit config files

```bash
# Copy example configs to Hermes config directory
cp examples/hermes-agent/config/hermes_config.yaml ~/.hermes/config.yaml
cp examples/hermes-agent/config/SOUL.md ~/.hermes/SOUL.md
cp examples/hermes-agent/config/USER.md ~/.hermes/USER.md

# Edit USER.md with your location, preferences, and activities
$EDITOR ~/.hermes/USER.md
```

### 3. Install skills

```bash
cp examples/hermes-agent/skills/*.md ~/.hermes/skills/
```

### 4. (Optional) Start the tile server

```bash
wx-server --port 8080 --cache-size 512
```

Enables the live map dashboard and XYZ tile streaming for visual weather overlays.

### 5. Start the agent

```bash
hermes-agent --config ~/.hermes/config.yaml
```

The agent will:
- Connect to `wx-mcp` via MCP (stdio)
- Load your user profile from USER.md
- Begin cron-scheduled weather checks (briefings, severe weather, fire weather)
- Listen for Telegram messages

## MCP Tool Reference

20 tools exposed by `wx-mcp`, organized by bandwidth cost:

### Lightweight (< 50 KB per call)

| Tool | Size | Description |
|------|------|-------------|
| `wx_conditions` | ~6 KB | Current temperature, wind, humidity, pressure |
| `wx_metar` | ~3 KB | Raw/decoded METAR observations |
| `wx_station` | ~5 KB | Station info and metadata |
| `wx_alerts` | ~10 KB | Active NWS alerts for a location |
| `wx_hazards` | ~20 KB | Categorized hazard assessment |

### Medium (50-200 KB per call)

| Tool | Size | Description |
|------|------|-------------|
| `wx_global` | ~30 KB | Global weather via Open-Meteo |
| `wx_history` | ~40 KB | Historical weather data |
| `wx_brief` | ~50 KB | Quick conditions + forecast (answers most questions) |
| `wx_forecast` | ~50 KB | Hourly/daily forecast for planning |
| `wx_severe` | ~200 KB | Full SPC/severe weather analysis |

### Heavy (1 MB+ per call)

| Tool | Size | Description |
|------|------|-------------|
| `wx_radar` | ~15 MB | NEXRAD radar composite (use sparingly) |
| `wx_briefing` | ~15 MB | Comprehensive weather briefing |

### Specialized

| Tool | Description |
|------|-------------|
| `wx_tiles` | XYZ map tiles (~50 KB/tile) for Leaflet/Mapbox overlay |
| `wx_rotation` | Mesocyclone/rotation detection from radar |
| `wx_mrms` | Multi-Radar Multi-Sensor composites |
| `wx_watchbox` | SPC watch box geometry |
| `wx_sounding` | Upper-air sounding data |
| `wx_sse` | Subscribe to real-time SSE event stream |
| `wx_radar --raw` | Basemap-free radar for compositing |
| `wx_metar --raw` | Raw METAR string, no decode |

## Skills

Skills are markdown files that teach the agent how to handle specific weather scenarios. The agent can also write new skills when it encounters novel problems (`auto_evolve: true`).

### Included Skills

| Skill | File | Purpose |
|-------|------|---------|
| Morning Briefing | `morning_briefing.md` | Daily 6 AM weather summary with activity advice |
| Severe Weather Alert | `severe_weather_alert.md` | 4-tier escalation: GREEN/YELLOW/ORANGE/RED |
| Activity Check | `activity_check.md` | "Can I mow/spray/burn/fish today?" with thresholds |
| Fire Weather | `fire_weather.md` | Fire risk assessment: LOW through EXTREME |

### Adding Custom Skills

Create a markdown file in `~/.hermes/skills/` with this structure:

```markdown
# Skill Name

## When to Use
Describe the triggers — user questions, cron events, or alert conditions.

## Steps
1. Which wx tools to call and in what order
2. How to interpret the results
3. How to format the response

## Thresholds
Define numeric thresholds for decisions (wind speed, temperature, etc.)

## Example Output
Show what a good response looks like.
```

The agent loads all `.md` files from the skills directory at startup. New skills take effect on the next agent restart, or immediately if the agent writes them via `auto_evolve`.

## Cron Schedule

| Job | Schedule | Purpose |
|-----|----------|---------|
| `morning_briefing` | 6:00 AM daily | Weather summary + activity advice |
| `severe_check` | Every 30 min | Alert monitoring (notifies only on active warnings) |
| `evening_summary` | 8:00 PM daily | Overnight outlook + tomorrow preview |
| `fire_weather_check` | 12:00 PM daily | Peak fire weather assessment |

Edit schedules in `hermes_config.yaml`. All times are in the system's local timezone.

## File Structure

```
examples/hermes-agent/
├── README.md
├── config/
│   ├── hermes_config.yaml   # Agent config (model, cron, MCP, gateway)
│   ├── SOUL.md              # Agent identity and behavioral principles
│   └── USER.md              # Template user profile (edit with your info)
└── skills/
    ├── morning_briefing.md      # Daily weather briefing
    ├── severe_weather_alert.md  # Alert escalation tiers
    ├── activity_check.md        # Outdoor activity advisor
    └── fire_weather.md          # Fire weather assessment
```
