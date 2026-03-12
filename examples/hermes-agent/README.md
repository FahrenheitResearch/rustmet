# Storm Guardian — AI Weather Agent Example

Deploy an autonomous AI weather agent using [Hermes Agent](https://github.com/NousResearch/hermes-agent) and rustmet's `wx-mcp` tool server. The agent monitors weather conditions on a schedule, answers natural-language questions about outdoor activities, and escalates severe weather alerts via Telegram.

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
| Single GPU | 1x H200 | 141 GB | NVFP4 quantization, fits in one card |
| Multi-GPU | 2x H100-80GB | 160 GB | Tensor parallel across two cards |
| Budget | 2x A100-80GB | 160 GB | Slower but works with FP8 |

Smaller models (70B, 8B) work on consumer hardware but reduce agent quality. Any OpenAI-compatible endpoint works — use OpenRouter if you don't have a GPU.

## Quick Start

### 1. Install Hermes Agent

```bash
git clone --recurse-submodules https://github.com/NousResearch/hermes-agent.git
cd hermes-agent
uv venv venv --python 3.11
uv pip install -e ".[all]"
```

### 2. Copy config files

```bash
# Main config
cp examples/hermes-agent/config/hermes_config.yaml ~/.hermes/config.yaml

# Agent personality
cp examples/hermes-agent/config/SOUL.md ~/.hermes/SOUL.md

# User profile (goes in memories/)
mkdir -p ~/.hermes/memories
cp examples/hermes-agent/config/USER.md ~/.hermes/memories/USER.md

# Environment variables
cp examples/hermes-agent/config/dot-env.example ~/.hermes/.env
```

Edit `~/.hermes/memories/USER.md` with your location, preferences, and activities.
Edit `~/.hermes/.env` with your API keys and Telegram credentials.

### 3. Install skills

```bash
cp -r examples/hermes-agent/skills/weather ~/.hermes/skills/weather
```

### 4. Set up Telegram gateway

```bash
hermes gateway setup    # Interactive wizard
hermes gateway install  # Install as systemd/launchd service
```

### 5. Start vLLM (if self-hosting)

```bash
vllm serve nvidia/Llama-3.1-Nemotron-3-Super-120B-v1 \
  --port 8000 \
  --max-model-len 8192 \
  --enable-auto-tool-choice \
  --tool-call-parser hermes
```

### 6. Set up cron jobs

Cron jobs are managed via CLI, not config files:

```bash
# Morning briefing at 6 AM
hermes cron add --name morning_briefing \
  --schedule "0 6 * * *" \
  --deliver telegram \
  --prompt "Generate a morning weather briefing for the user."

# Severe weather check every 30 minutes
hermes cron add --name severe_check \
  --schedule "every 30m" \
  --deliver telegram \
  --prompt "Check for severe weather alerts at the user's location. Only notify if there are active warnings or watches."

# Evening summary at 8 PM
hermes cron add --name evening_summary \
  --schedule "0 20 * * *" \
  --deliver telegram \
  --prompt "Give a brief overnight weather summary and tomorrow's outlook."

# Fire weather check at noon
hermes cron add --name fire_weather_check \
  --schedule "0 12 * * *" \
  --deliver telegram \
  --prompt "Check fire weather conditions. Alert if RH is below 20% or Red Flag Warnings are active."
```

### 7. (Optional) Start the tile server

```bash
wx-server --port 8080 --cache-size 512
```

Enables the live map dashboard and XYZ tile streaming for visual weather overlays.

### 8. Start the agent

```bash
hermes
```

The agent will:
- Connect to `wx-mcp` via MCP (stdio)
- Load your user profile from `~/.hermes/memories/USER.md`
- Run cron-scheduled weather checks
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

### Visualization & Specialized

| Tool | Size | Description |
|------|------|-------------|
| `wx_tiles` | ~50 KB/tile | XYZ PNG tiles for Leaflet/Mapbox overlays |
| `wx_radar_image` | ~200 KB | Rendered NEXRAD PPI as PNG file |
| `wx_model_image` | ~200 KB | Rendered model field (CAPE, temp, etc.) as PNG |
| `wx_point` | ~5 KB | Single grid-point value extraction |
| `wx_scan` | ~10 KB | Grid extrema search (max CAPE, min pressure, etc.) |
| `wx_timeseries` | ~20 KB | Multi-hour trend for a variable at a point |
| `wx_sounding` | ~50 KB | HRRR/RAP model-derived convective parameters |
| `wx_evidence` | ~30 KB | Multi-source confidence assessment (METAR vs HRRR vs NWS) |

## Skills

Skills teach the agent how to handle specific weather scenarios. They live in `~/.hermes/skills/weather/` using the standard Hermes skill format (SKILL.md with YAML frontmatter).

### Included Skills

| Skill | Directory | Purpose |
|-------|-----------|---------|
| Morning Briefing | `weather/morning-briefing/` | Daily 6 AM weather summary with activity advice |
| Severe Weather Alert | `weather/severe-weather-alert/` | 4-tier escalation: GREEN/YELLOW/ORANGE/RED |
| Activity Check | `weather/activity-check/` | "Can I mow/spray/burn/fish today?" with thresholds |
| Fire Weather | `weather/fire-weather/` | Fire risk assessment: LOW through EXTREME |

The agent can also write new skills when it encounters novel problems (`auto_evolve` in Hermes config).

## Cron Schedule

| Job | Schedule | Purpose |
|-----|----------|---------|
| `morning_briefing` | 6:00 AM daily | Weather summary + activity advice |
| `severe_check` | Every 30 min | Alert monitoring (notifies only on active warnings) |
| `evening_summary` | 8:00 PM daily | Overnight outlook + tomorrow preview |
| `fire_weather_check` | 12:00 PM daily | Peak fire weather assessment |

Manage with `hermes cron list`, `hermes cron add`, `hermes cron remove`. All times are in the system's local timezone.

## File Structure

```
examples/hermes-agent/
├── README.md
├── config/
│   ├── hermes_config.yaml     # → ~/.hermes/config.yaml
│   ├── dot-env.example        # → ~/.hermes/.env
│   ├── SOUL.md                # → ~/.hermes/SOUL.md
│   └── USER.md                # → ~/.hermes/memories/USER.md
└── skills/
    └── weather/               # → ~/.hermes/skills/weather/
        ├── morning-briefing/
        │   └── SKILL.md
        ├── severe-weather-alert/
        │   └── SKILL.md
        ├── activity-check/
        │   └── SKILL.md
        └── fire-weather/
            └── SKILL.md
```

## Deployment on Vast.ai (H200)

1. Rent a single H200 instance (141 GB VRAM)
2. Install vLLM: `pip install vllm`
3. Start the model:
   ```bash
   vllm serve nvidia/Llama-3.1-Nemotron-3-Super-120B-v1 \
     --port 8000 \
     --max-model-len 8192 \
     --enable-auto-tool-choice \
     --tool-call-parser hermes
   ```
4. On your local machine (or the same instance), set `OPENAI_BASE_URL` to point at the vLLM endpoint
5. Place `wx-mcp` and `wx-pro` binaries in your PATH
6. Start Hermes Agent: `hermes`

If running Hermes locally and vLLM remotely, update `~/.hermes/.env`:
```bash
OPENAI_BASE_URL=http://<vast-ai-ip>:8000/v1
```
