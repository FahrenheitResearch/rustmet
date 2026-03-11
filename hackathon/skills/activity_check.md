# Activity Weather Check

## When to Use
User asks about outdoor activities: mowing, spraying, fishing, driving, flying, burning brush, gardening, running, biking, golfing, outdoor events, etc. Any question in the form "can I do X today/tomorrow/this weekend?" or "when can I do X?" or "is it too [hot/cold/windy] to X?"

## Steps
1. Parse the activity from user's question
2. Identify the time window (today, tomorrow, this weekend, specific day)
3. Call `wx_brief` for current conditions and today's forecast
4. If asking about tomorrow or later, call `wx_forecast` for extended hourly data
5. Apply activity-specific thresholds (see below)
6. Give a clear yes/no with reasoning
7. If "no" for the requested time, suggest the best alternative window
8. Save any new activity preferences to USER.md for future reference

## Activity Thresholds

### Mowing
- Wind: < 20 mph
- Rain: No rain in next 2 hours
- Lightning: No thunderstorms within 30 miles
- Ground: Not saturated from recent heavy rain (check last 24hr precip)
- Note: Morning is usually calmest — suggest early if afternoon is windy

### Spraying (Herbicide/Pesticide)
- Wind: < 10 mph (critical — drift liability)
- Rain: No rain in next 4 hours (product needs to dry)
- Temperature: 45-85°F (efficacy drops outside this range)
- Inversion: Check for temperature inversions (calm + warm above cool = drift risk)
- Best window: Early morning, before thermal mixing starts

### Burning Brush / Controlled Burn
- **FIRST**: Check for Red Flag Warnings — if active, answer is always NO
- Wind: 5-15 mph (too calm = smoke hangs, too strong = fire escapes)
- Relative humidity: > 25% (below this, fire behavior becomes erratic)
- Rain in last 48hr: Helpful for controlling spread
- Fire Weather Watch: Do not burn
- Always recommend having water and a phone ready

### Driving (Long Distance)
- Visibility: > 1 mile (check fog advisories)
- Ice: No ice storm warnings or freezing rain advisories along route
- Flooding: No flash flood warnings along route
- Wind: Note if > 40 mph (especially for high-profile vehicles/trailers)
- Snow: Check winter storm warnings and road conditions

### Fishing
- Wind: < 20 mph (boat safety)
- Lightning: No thunderstorms within 50 miles (water is dangerous)
- Temperature: Note extreme heat warnings
- Barometric trend: Falling pressure = better bite (optional fun fact)

### Flying (VFR — Small Aircraft)
- Ceiling: > 3,000 ft AGL
- Visibility: > 5 statute miles
- Surface wind: < 25 knots, crosswind < 15 knots
- Gusts: < 30 knots
- Convective SIGMET: None in the area
- TFRs: Remind to check NOTAMs (outside our data)
- Turbulence: Note if strong winds aloft

### Running / Biking
- Heat index: < 105°F (NWS danger threshold)
- Wind chill: > 0°F
- Lightning: No thunderstorms within 10 miles
- Air quality: Note if AQI > 100 (unhealthy for sensitive groups)

### Outdoor Events (Weddings, BBQs, Parties)
- Rain probability: Note any chance > 20%
- Wind: Note if > 25 mph (tents, decorations)
- Temperature: Note extremes
- Suggest backup plan if rain > 50%

## Response Format
- Lead with the answer: YES or NO (or "yes, but...")
- Give the specific conditions that drive the decision
- If timing matters, give specific hours
- If saying no, suggest the next good window

## Example Interactions

**User:** "Can I spray the back 40 tomorrow?"
**Agent** calls `wx_forecast`, checks hourly wind and precip.
**Response:** "Tomorrow looks good for spraying between 7-11 AM — winds will be under 8 mph and no rain is expected until evening. After noon, winds pick up to 15-20 mph from the south, so I'd plan to finish by lunch. Temperature will be in the 60s-70s, which is ideal for application."

**User:** "Good day to mow?"
**Agent** calls `wx_brief`, checks conditions.
**Response:** "This morning is perfect for mowing — 72°F, light winds at 8 mph, and no rain until tonight. If you wait until this afternoon, winds pick up to 20+ mph and there's a 60% chance of storms after 4 PM. I'd go for it now."

**User:** "Can I burn my brush pile this weekend?"
**Agent** calls `wx_alerts`, `wx_forecast`.
**Response:** "Saturday looks safe for burning — winds 8-12 mph from the north, humidity around 40%, and no fire weather alerts. Sunday is a no-go: a Red Flag Warning is expected with winds gusting to 35 mph and humidity dropping below 15%. If you burn Saturday, plan to have it fully out by evening."

## Learning
- When a user asks about an activity for the first time, note it in USER.md
- Track which activities they ask about regularly
- Proactively include relevant activity advice in morning briefings
