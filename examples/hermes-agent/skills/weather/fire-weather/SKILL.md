---
name: fire-weather
description: Assess fire weather risk from LOW to EXTREME with actionable guidance
version: 1.0.0
metadata:
  hermes:
    tags: [weather, fire, safety, burns]
    category: weather
    requires_tools: [wx_conditions, wx_alerts, wx_forecast]
---

# Fire Weather Assessment

## When to Use
- User asks about burning brush, controlled burns, or fire safety
- Red Flag Warnings appear in any alert check
- Fire Weather Watch is issued for user's area
- When relative humidity drops below 20% in routine checks

## Steps
1. Call `wx_conditions` for current temperature, wind speed/direction, and relative humidity
2. Call `wx_alerts` to check for Red Flag Warnings or Fire Weather Watches
3. Call `wx_forecast` for wind and humidity trend over next 24-48 hours
4. Assess fire risk using the criteria below
5. Provide clear, actionable guidance with specific safe or unsafe windows

## Fire Risk Assessment

### LOW (Green)
- Relative humidity: > 40%
- Wind: < 10 mph
- No fire weather alerts
- Recent rainfall in last 48 hours
- **Guidance:** "Conditions are favorable for controlled burning. Standard precautions apply."

### MODERATE (Yellow)
- Relative humidity: 25-40%
- Wind: 10-20 mph
- **Guidance:** "Burning is possible but use extra caution. Keep your burn small."

### ELEVATED (Orange)
- Relative humidity: 15-25%
- Wind: 15-25 mph
- **Guidance:** "I'd recommend against burning today. Wait for better conditions."

### CRITICAL (Red)
- Relative humidity: < 15%
- Wind: > 20 mph
- Active Red Flag Warning
- **Guidance:** "DO NOT BURN. Any fire could spread rapidly and become uncontrollable."

### EXTREME
- Relative humidity: < 10%
- Wind: > 30 mph
- **Guidance:** "EXTREME fire danger. Do not use outdoor equipment in dry vegetation. Have a go-bag ready."

## Key Factors
- **Relative Humidity**: Most important single factor. Below 25%, dead fuels dry rapidly. Below 15%, fires become erratic.
- **Wind**: Provides oxygen, drives spread, carries embers. Low RH + high wind = critical fire weather.
- **Red Flag Warning**: Issued when sustained wind >= 20 mph AND RH <= 15%, or dry lightning expected.

## Proactive Alerts
- If routine check shows RH dropping below 20%, mention fire risk in next briefing
- If Red Flag Warning is issued, send a YELLOW-tier notification even if user didn't ask about fire
