---
name: severe-weather-alert
description: Escalate severe weather alerts with 4-tier system (GREEN/YELLOW/ORANGE/RED)
version: 1.0.0
metadata:
  hermes:
    tags: [weather, alerts, severe, safety]
    category: weather
    requires_tools: [wx_alerts]
---

# Severe Weather Alert Escalation

## When to Use
When cron job detects elevated weather risk, or when alerts appear in any wx tool output. This skill handles all alert escalation — from routine watches to life-threatening tornado warnings.

## Escalation Tiers

### GREEN — No Action
No active alerts. Include a weather note in the next scheduled briefing. Do not send a separate notification.

### YELLOW — Watch Issued
A Watch means conditions are favorable for severe weather. Send an extra update via Telegram.

**Triggers:** Tornado Watch, Severe Thunderstorm Watch, Winter Storm Watch, Fire Weather Watch

**Template:**
"A [WATCH TYPE] has been issued for your area until [TIME]. [SPECIFIC THREAT]. Keep your phone charged and know where your shelter is."

### ORANGE — Warning Issued
A Warning means severe weather is occurring or imminent. Send an immediate push notification.

**Triggers:** Severe Thunderstorm Warning, Flash Flood Warning, Winter Storm Warning, Blizzard Warning, Ice Storm Warning

**Template:**
"WARNING: [WARNING TYPE] for your county until [TIME]. [SPECIFIC HAZARD AND MAGNITUDE]. [ACTION ITEM]."

### RED — Tornado Warning / PDS
Maximum urgency. Life-threatening situation. Override Do Not Disturb.

**Triggers:** Tornado Warning, PDS Tornado Watch, PDS Severe Thunderstorm Warning, Tornado Emergency

**Template:**
"TORNADO WARNING — Take shelter NOW in your interior room on the lowest floor. [SPECIFIC INFO]. Do not wait."

## Steps
1. Call `wx_alerts` with user's lat/lon from USER.md
2. Parse each alert for type, severity, urgency, and certainty
3. Classify into escalation tier:
   - If `tornado_warning` or `pds` or `tornado_emergency`: **RED** — send immediately
   - If `severe_thunderstorm_warning` or `flash_flood_warning`: **ORANGE** — send immediately
   - If any Watch: **YELLOW** — send update
   - Otherwise: **GREEN** — note for next briefing
4. For RED and ORANGE alerts, include:
   - Specific timing (when it starts, when it expires)
   - Specific threats (wind speed, hail size, tornado likelihood)
   - Clear action items (shelter, move indoors, avoid travel)
   - Direction and speed of approaching storm if available
5. After the event passes, send an all-clear: "The Tornado Warning has expired. The storm has moved east of your area. You're clear."

## Do Not Disturb Override Rules
- GREEN and YELLOW: Respect DND hours (default 10 PM - 6 AM)
- ORANGE: Override DND only for Flash Flood Warning with "immediate" urgency
- RED: Always override DND. Tornado warnings at 2 AM save lives.

## Deduplication
- Do not send the same alert twice
- Track alert IDs to avoid duplicate notifications
- If an alert is extended or upgraded, send an update noting the change
- If a Watch is upgraded to a Warning, send the Warning as a new notification

## Rate Limiting
- Maximum 1 YELLOW notification per hour (batch multiple watches)
- No limit on ORANGE or RED — every warning matters
- Send all-clear messages only after the last active warning expires
