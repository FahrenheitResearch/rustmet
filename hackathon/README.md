# Storm Guardian

**An AI weather sentinel powered by pure-Rust meteorological tools that learns what you care about and keeps you safe — even on satellite internet.**

---

## The Story

I built this to protect my dad.

He lives in rural Oklahoma — tornado alley. His internet is satellite. Weather apps chew through his data cap in a day during storm season. The NWS website times out. Radar won't load. And when a tornado warning drops at 2 AM, he might not hear his phone buzz.

I'm a meteorologist and a systems programmer. I spent a year building the fastest weather data tools in existence — 50,000+ lines of pure Rust that download and decode GRIB2 model data, NEXRAD radar, and NWS products faster than anything else available. A full HRRR analysis in 1.4 seconds. Radar composites in under a second. No Python. No C dependencies. A single binary.

But fast tools aren't enough. Someone still has to run them. Someone still has to interpret the output and decide what to do.

Storm Guardian is the missing piece. It's a Hermes Agent that wraps my Rust weather tools via MCP and acts as a tireless weather sentinel. It checks conditions every 30 minutes. It learns what my dad cares about — mowing, spraying, burning brush. It sends plain-English briefings to his phone via Telegram. And when severe weather threatens, it escalates automatically — from gentle reminders to urgent shelter commands.

**He doesn't need to understand meteorology. He just needs to trust the agent that does.**

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    STORM GUARDIAN                        │
│                   (Hermes Agent)                         │
│                                                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │  Memory   │  │  Skills  │  │   Cron   │              │
│  │ (USER.md) │  │ (.md)    │  │ (checks) │              │
│  └──────────┘  └──────────┘  └──────────┘              │
│         │            │             │                    │
│         └────────────┼─────────────┘                    │
│                      │                                  │
│              ┌───────▼───────┐                          │
│              │   MCP Server  │                          │
│              │  (wx-tools)   │                          │
│              └───────┬───────┘                          │
└──────────────────────┼──────────────────────────────────┘
                       │
          ┌────────────┼────────────┐
          │            │            │
   ┌──────▼──────┐ ┌──▼───┐ ┌─────▼─────┐
   │   wx-pro    │ │wx-lite│ │ wx-radar  │
   │  (50K LOC)  │ │(6KB)  │ │ (NEXRAD)  │
   │  Pure Rust  │ │       │ │           │
   └──────┬──────┘ └──┬───┘ └─────┬─────┘
          │            │           │
   ┌──────▼────────────▼───────────▼─────┐
   │         NOAA / NWS / AWS S3         │
   │   HRRR • GFS • NAM • RAP • NEXRAD  │
   │   Alerts • Forecasts • METARs       │
   └─────────────────────────────────────┘

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
- "What's the weather doing?" → Instant conditions with context

### Adaptive Learning
- **Skill evolution** — Every new type of question becomes a reusable skill
- **User modeling** — Learns your activities, sensitivities, and preferences
- **Activity-aware advice** — "Too windy for spraying, but mowing is fine this morning"

### Bandwidth-Conscious
- **Tiered data access** — Starts with wx_brief (~50KB), only escalates when needed
- **Total event cost** — 15MB for an entire severe weather event vs 500MB+ for traditional monitoring
- **Satellite-friendly** — Designed for users with limited, expensive bandwidth

---

## Tech Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Agent | [Hermes Agent](https://github.com/NousResearch/hermes-agent) | Autonomous reasoning, memory, cron, skills |
| Model | Hermes 3 (Llama 3.1 70B) | Language understanding and generation |
| Weather Tools | wx-pro / wx-lite (Pure Rust, 50K+ LOC) | GRIB2 decode, NEXRAD radar, NWS products |
| Tool Protocol | MCP (Model Context Protocol) | Connects agent to weather tools |
| Gateway | Telegram Bot API | Push notifications to user's phone |
| Data Sources | NOAA/NWS via AWS S3 | HRRR, GFS, NAM, RAP, NEXRAD, alerts |

### Why Pure Rust?

- **Single binary** — No Python, no conda, no pip, no C libraries. One 1.4MB executable.
- **4.9x faster** than Python/cfgrib for HRRR download+decode (1.4s vs 7.0s)
- **Zero dependencies** — Runs on any Linux box, including a Raspberry Pi
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

### 4. Start the Agent

```bash
hermes-agent --config ~/.hermes/config.yaml
```

Storm Guardian will:
- Connect to the wx-tools MCP server
- Load your user profile from USER.md
- Begin cron-scheduled weather checks
- Listen for Telegram messages

---

## Demo Video

**Duration:** 3 minutes

The demo walks through four scenarios showing Storm Guardian in action:

1. **Morning Briefing** — A 6 AM Telegram message with today's forecast, translated into actionable advice
2. **Natural Language Question** — User asks "Can I spray tomorrow?" and gets a specific time window
3. **Severe Weather Escalation** — Watch → Warning → Tornado Warning, with automatic escalation at each tier
4. **Skill Evolution** — Show how the agent builds reusable skills from novel questions

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

**Because your family's safety shouldn't depend on bandwidth.**

---

*Built for the Nous Research Hermes Agent Hackathon by Drew — meteorologist, systems programmer, and a son who wants his dad to be safe.*
