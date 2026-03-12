---
name: activity-check
description: Assess weather conditions for outdoor activities with thresholds
version: 1.0.0
metadata:
  hermes:
    tags: [weather, activities, outdoor, planning]
    category: weather
    requires_tools: [wx_brief, wx_forecast]
---

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

### Spraying (Herbicide/Pesticide)
- Wind: < 10 mph (critical — drift liability)
- Rain: No rain in next 4 hours (product needs to dry)
- Temperature: 45-85°F (efficacy drops outside this range)
- Inversion: Check for temperature inversions (calm + warm above cool = drift risk)

### Burning Brush / Controlled Burn
- **FIRST**: Check for Red Flag Warnings — if active, answer is always NO
- Wind: 5-15 mph (too calm = smoke hangs, too strong = fire escapes)
- Relative humidity: > 25% (below this, fire behavior becomes erratic)

### Driving (Long Distance)
- Visibility: > 1 mile (check fog advisories)
- Ice: No ice storm warnings or freezing rain advisories along route
- Wind: Note if > 40 mph (high-profile vehicles/trailers)

### Fishing
- Wind: < 20 mph (boat safety)
- Lightning: No thunderstorms within 50 miles (water is dangerous)
- Barometric trend: Falling pressure = better bite (optional fun fact)

### Running / Biking
- Heat index: < 105°F
- Wind chill: > 0°F
- Lightning: No thunderstorms within 10 miles

## Response Format
- Lead with the answer: YES or NO (or "yes, but...")
- Give the specific conditions that drive the decision
- If timing matters, give specific hours
- If saying no, suggest the next good window

## Example
**User:** "Can I spray the back 40 tomorrow?"
**Response:** "Tomorrow looks good for spraying between 7-11 AM — winds will be under 8 mph and no rain is expected until evening. After noon, winds pick up to 15-20 mph from the south, so I'd plan to finish by lunch."
