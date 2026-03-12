# Fire Weather Assessment

## When to Use
- User asks about burning brush, controlled burns, or fire safety
- Red Flag Warnings appear in any alert check
- Fire Weather Watch is issued for user's area
- During wildfire season (varies by region; typically spring and fall in the Plains)
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
- **Guidance:** "Conditions are favorable for controlled burning. Standard precautions apply — have water ready, clear a perimeter, and have a phone nearby."

### MODERATE (Yellow)
- Relative humidity: 25-40%
- Wind: 10-20 mph
- No fire weather alerts
- **Guidance:** "Burning is possible but use extra caution. Winds could push a fire faster than expected. Keep your burn small and have help nearby."

### ELEVATED (Orange)
- Relative humidity: 15-25%
- Wind: 15-25 mph
- Fire Weather Watch may be issued
- **Guidance:** "I'd recommend against burning today. Humidity is low and winds are strong enough to carry embers. Wait for better conditions."

### CRITICAL (Red)
- Relative humidity: < 15%
- Wind: > 20 mph
- Active Red Flag Warning
- **Guidance:** "DO NOT BURN. A Red Flag Warning is in effect — any fire could spread rapidly and become uncontrollable. Avoid any activity that could start a fire, including welding, grinding, or using equipment in dry grass."

### EXTREME
- Relative humidity: < 10%
- Wind: > 30 mph
- Active Red Flag Warning with critical fire weather
- **Guidance:** "EXTREME fire danger. Do not burn, do not use outdoor equipment in dry vegetation, and be aware that wildfires could develop rapidly in your area. Have a go-bag ready if you're in a rural area with grass or timber."

## Key Factors Explained

### Relative Humidity (RH)
The most important single factor for fire behavior. Below 25%, dead fuels dry rapidly. Below 15%, fires become erratic and difficult to control.

### Wind
Wind provides oxygen, drives fire spread, and carries embers ahead of the fire. The combination of low RH and high wind is what creates critical fire weather.

### Fine Fuel Moisture
Grass and dead leaves respond to humidity changes within hours. If RH has been below 25% for several hours, assume fine fuels are dry enough to burn readily.

### Red Flag Warning
Issued by the NWS when conditions are critical for wildfire development:
- Sustained wind >= 20 mph AND relative humidity <= 15%
- OR dry lightning expected
This is the official "do not burn" signal.

### Fire Weather Watch
Conditions may develop into Red Flag criteria in the next 12-48 hours. Plan accordingly — if you want to burn, do it before the watch takes effect.

## Proactive Alerts
- If a routine weather check shows RH dropping below 20%, mention fire risk in the next briefing
- If a Red Flag Warning is issued, send a YELLOW-tier notification even if the user didn't ask about fire
- During spring/fall in the Plains, include fire weather in morning briefings when conditions are dry

## Response Examples

**User:** "Can I burn my brush pile today?"
**Response (LOW):** "Today's a good day for it. Humidity is 45%, winds are light at 6 mph from the south, and no fire weather alerts. Forecast shows conditions staying favorable until evening. Standard precautions — clear a 10-foot perimeter, have a hose or water tank ready, and keep your phone on you."

**Response (CRITICAL):** "Absolutely not today. There's a Red Flag Warning until 8 PM — winds are gusting to 35 mph and humidity is only 12%. Any fire you start could escape within minutes. Looking ahead, Thursday looks much better — winds drop to 8 mph and humidity recovers to 40% after Wednesday night's rain."

**Proactive alert (in morning briefing):** "Fire weather note: humidity is expected to drop below 15% this afternoon with 25 mph winds. A Red Flag Warning goes into effect at noon. Avoid any burning or spark-producing activity. If you have livestock in grass pastures, be aware of wildfire risk."
