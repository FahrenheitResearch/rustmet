# Storm Guardian Demo Script (3 minutes)

## Setup (10 sec)

**Screen:** Terminal showing Hermes Agent starting up, connecting to wx-tools MCP server.

**Voiceover:**
"Storm Guardian is an AI weather sentinel powered by 50,000 lines of pure Rust. It connects to real NOAA data — HRRR model output, NEXRAD radar, NWS alerts — through a set of MCP tools that download and decode weather data faster than anything else available. Let me show you what it does."

---

## Scene 1: Morning Briefing (45 sec)

**Screen:** Phone showing Telegram. A message arrives from Storm Guardian at 6:00 AM.

**Message content:**
> Good morning! It's 62°F and mostly cloudy in Norman. Today's high will be 78°F with a 40% chance of afternoon thunderstorms. Winds pick up to 20 mph after noon.
>
> Mowing: Get it done this morning — winds are light now but it'll be gusty by lunch.
> Spraying: Not today. Winds above 10 mph by 10 AM and rain likely this afternoon.
>
> Heads up: SPC has a Slight Risk for our area this afternoon. I'll keep watching.

**Voiceover:**
"Every morning at 6 AM, Storm Guardian sends a plain-English briefing. It fetched real HRRR model data and NWS forecasts in under 2 seconds, checked for active alerts, and translated everything into advice tailored to what this user cares about — mowing and spraying. No jargon. No scrolling through apps. Just the answer."

**Show:** Quick flash of the terminal showing `wx_brief` call completing in 1.4 seconds.

---

## Scene 2: Natural Language Question (45 sec)

**Screen:** Phone. User types a message.

**User:** "Can I spray the orchard tomorrow?"

**Screen:** Terminal showing the agent reasoning — calling `wx_forecast`, checking hourly wind speeds, checking precipitation timing.

**Storm Guardian responds:**
> Tomorrow looks good for spraying between 7-11 AM. Winds will be 5-8 mph from the south, no rain until late evening. Temperature in the mid-60s — ideal for application.
>
> After noon, winds pick up to 15-20 mph, so plan to wrap up by lunch. No inversions expected, so drift risk is low in the morning window.

**Voiceover:**
"The user asks a natural question and gets a specific, actionable answer. The agent checked hourly wind forecasts, precipitation timing, temperature, and even inversion risk — all things that matter for pesticide application. And the first time it answered a spray question, it wrote a skill for it. Now it has that knowledge permanently."

---

## Scene 3: Severe Weather Escalation (60 sec)

**Screen:** Fast-forward to afternoon. Split screen: phone on left, terminal on right.

**2:00 PM — Cron check fires.**
Terminal shows `wx_alerts` call. SPC upgrades to Moderate Risk.

**Phone notification (YELLOW):**
> A Tornado Watch has been issued for Cleveland County until 10 PM. Conditions support supercells with large hail and a few tornadoes. Keep your phone charged and know where your shelter is.

**4:30 PM — Cron check fires again.**
Terminal shows `wx_severe` call, then `wx_radar` call.

**Phone notification (ORANGE):**
> ⚠️ SEVERE THUNDERSTORM WARNING for Cleveland County until 5:15 PM. 70 mph wind gusts and golf-ball size hail. Move indoors now and stay away from windows.

**5:15 PM — Tornado Warning issued.**
Terminal shows immediate detection.

**Phone notification (RED):**
> 🚨 TORNADO WARNING — Take shelter NOW. Radar confirms rotation 12 miles southwest of Norman, moving northeast at 35 mph. Get to your interior room on the lowest floor. Do not wait.

**5:45 PM — Warning expires.**

**Phone notification:**
> All clear. The Tornado Warning has expired and the storm has moved east of your area. You're safe.

**Voiceover:**
"Watch how the agent escalates automatically. A watch gets a calm heads-up. A warning gets an urgent push. A tornado warning overrides Do Not Disturb and tells you exactly what to do. And when it's over, it tells you that too. The entire event used 15 megabytes of bandwidth — versus 500 megabytes or more if you were refreshing radar on a weather app every 30 seconds."

**Show:** Bandwidth counter in corner: 15.2 MB total.

---

## Scene 4: Skill Evolution (20 sec)

**Screen:** File browser showing the skills directory.

```
skills/
├── morning_briefing.md
├── severe_weather_alert.md
├── activity_check.md
├── fire_weather.md
├── spray_check.md        ← agent wrote this
└── livestock_cold.md     ← agent wrote this
```

**Voiceover:**
"Every time Storm Guardian solves a new problem, it writes a skill. Morning briefings. Severe weather escalation. Spray conditions. Fire weather. Two of these skills were written by the agent itself after answering novel questions. It gets smarter every day, and those skills persist across restarts."

---

## Closing (10 sec)

**Screen:** The architecture diagram from the README, then the Telegram chat showing the day's messages.

**Voiceover:**
"I built Storm Guardian to protect my dad. He lives in rural Oklahoma on satellite internet. Weather apps don't work for him. This does. It's a Hermes Agent, pure-Rust weather tools, MCP, and Telegram — and it might just save his life."

**Title card:**
> **Storm Guardian**
> Because your family's safety shouldn't depend on bandwidth.

---

## Technical Notes for Recording

- Use real weather data if possible (pick a recent severe weather day)
- Terminal should show actual tool calls and response times
- Phone can be a Telegram desktop preview or actual phone screen capture
- Keep transitions fast — the 3-minute limit is tight
- The emotional hook is the closing — dad's safety. Don't rush it.
