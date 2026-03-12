# Morning Weather Briefing

## When to Use
Every morning at the scheduled time, or when user asks "what's the weather today?", "morning briefing", "what's it look like out there?", or similar.

## Steps
1. Call `wx_brief` tool with user's home location (from USER.md)
2. If `alerts_active > 0`, call `wx_hazards` for detailed categorization
3. If `hazard_level` is "high" or "extreme", call `wx_severe` for full SPC assessment
4. Check the user's known activities in USER.md for activity-specific advice
5. Format a plain-English briefing covering:
   - Current temperature and conditions
   - Today's high/low and precipitation chance
   - Any active alerts with plain-English descriptions
   - Wind forecast (important for rural activities)
   - Activity-specific advice if known (e.g., "good day to mow" or "too windy for spraying")
   - Any notable changes from yesterday

## Formatting Rules
- Lead with current conditions — that's what people want first
- Use conversational tone, not weather-service jargon
- Translate wind speeds into impacts ("gusty enough to blow trash cans over")
- Always include timing for weather changes ("rain starts around 3 PM")
- Keep it under 200 words unless severe weather warrants more

## Example Output
"Good morning! It's 62°F and mostly cloudy in Norman. Today's high will be 78°F with a 40% chance of afternoon thunderstorms. There's a Wind Advisory until 7 PM — gusts up to 45 mph. I'd hold off on spraying today, but mowing this morning should be fine before the wind picks up."

## Example Output (Severe Day)
"Good morning. Heads up — today is a significant severe weather day. SPC has issued a Moderate Risk for your area. Current temp is 71°F with dewpoints in the upper 60s — that's a lot of fuel for storms. Expect a quiet morning, but a line of storms will develop around 4 PM with damaging winds and a tornado threat. I'll be watching this all day and will alert you immediately if warnings are issued. Make sure your phone is charged and you know your shelter plan."

## Error Handling
- If `wx_brief` fails (network timeout), retry once after 30 seconds
- If retry fails, send: "I couldn't fetch weather data this morning — network issue. I'll try again in 30 minutes."
- Never send a briefing with stale data without noting it: "Based on data from [time], which is [X] hours old"
