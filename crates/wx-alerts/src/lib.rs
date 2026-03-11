//! wx-alerts: NWS alerts, SPC outlooks, watches, and warnings
//!
//! Provides functions for fetching weather hazard data from
//! api.weather.gov and the Storm Prediction Center.

pub mod alerts;
pub mod spc;
pub mod forecast;
pub mod reports;

pub use alerts::{Alert, Severity, Certainty, Urgency, fetch_active_alerts, fetch_alerts_by_state, fetch_alerts_by_point, fetch_alerts_by_zone, filter_severe};
pub use spc::{ConvectiveOutlook, OutlookCategory, MesoscaleDiscussion, Watch, WatchType, fetch_day1_outlook, fetch_day2_outlook, fetch_mesoscale_discussions, fetch_active_watches, point_risk_level};
pub use forecast::{PointForecast, ForecastPeriod, fetch_forecast, fetch_hourly_forecast};
pub use reports::{StormReport, ReportType, fetch_today_reports, fetch_yesterday_reports};
