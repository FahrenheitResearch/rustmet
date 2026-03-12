use axum::{
    extract::Query,
    response::sse::{Event, Sse},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Event types that can be streamed to connected clients.
///
/// Each variant maps to an SSE event name: `model_run`, `alert`, `radar`, `heartbeat`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type")]
pub enum WxEvent {
    /// A new model run has been detected.
    #[serde(rename = "model_run")]
    ModelRun {
        model: String,
        date: String,
        hour: u32,
        timestamp: String,
    },

    /// A new NWS alert has been issued.
    #[serde(rename = "alert")]
    Alert {
        event: String,
        severity: String,
        headline: String,
        lat: f64,
        lon: f64,
        timestamp: String,
    },

    /// A new radar volume scan is available.
    #[serde(rename = "radar")]
    Radar {
        site: String,
        timestamp: String,
    },

    /// Periodic keep-alive with connection stats.
    #[serde(rename = "heartbeat")]
    Heartbeat {
        timestamp: String,
        connected_clients: usize,
    },
}

impl WxEvent {
    /// Returns the SSE event name for this variant.
    pub fn event_type(&self) -> &'static str {
        match self {
            WxEvent::ModelRun { .. } => "model_run",
            WxEvent::Alert { .. } => "alert",
            WxEvent::Radar { .. } => "radar",
            WxEvent::Heartbeat { .. } => "heartbeat",
        }
    }
}

/// Broadcast hub for SSE events.
///
/// Wraps a `tokio::sync::broadcast` channel. All connected SSE clients receive
/// every event that passes their filter. Dropped events (client too slow) are
/// logged but do not disconnect the client.
pub struct EventHub {
    tx: broadcast::Sender<WxEvent>,
    _rx: broadcast::Receiver<WxEvent>, // kept alive so sender never closes
}

impl EventHub {
    /// Create a new hub with the given channel capacity.
    ///
    /// A capacity of 256–1024 is reasonable for weather events.
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = broadcast::channel(capacity);
        Self { tx, _rx: rx }
    }

    /// Subscribe to the event stream. Returns a new receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<WxEvent> {
        self.tx.subscribe()
    }

    /// Broadcast an event to all subscribers.
    ///
    /// Returns silently if there are no subscribers.
    pub fn broadcast(&self, event: WxEvent) {
        let _ = self.tx.send(event);
    }

    /// Get the current number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

/// Query parameters for the `/events` SSE endpoint.
#[derive(Deserialize)]
pub struct EventFilter {
    /// Comma-separated event types to receive: `model_run`, `alert`, `radar`,
    /// `heartbeat`, or `all`. Defaults to all types if omitted.
    pub types: Option<String>,
}

/// SSE handler — streams filtered weather events to connected clients.
///
/// Mount this on `GET /events`:
/// ```ignore
/// let hub = Arc::new(EventHub::new(512));
/// let app = Router::new().route("/events", get({
///     let hub = hub.clone();
///     move |query| handle_events(query, hub)
/// }));
/// ```
pub async fn handle_events(
    Query(filter): Query<EventFilter>,
    hub: Arc<EventHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = hub.subscribe();

    let allowed_types: Vec<String> = filter
        .types
        .unwrap_or_else(|| "model_run,alert,radar,heartbeat".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let etype = event.event_type();

                    if allowed_types.iter().any(|t| t == etype || t == "all") {
                        if let Ok(json) = serde_json::to_string(&event) {
                            yield Ok(Event::default().event(etype).data(json));
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("[sse] client lagged, skipped {n} events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

/// Background task: periodically checks for new model runs and broadcasts events.
///
/// Spawn this with `tokio::spawn(model_run_poller(hub.clone(), Duration::from_secs(60)))`.
pub async fn model_run_poller(hub: Arc<EventHub>, check_interval: Duration, wx_pro_path: std::path::PathBuf) {
    let mut last_runs: HashMap<String, String> = HashMap::new();
    let models = ["hrrr", "gfs", "rap", "nam"];

    loop {
        tokio::time::sleep(check_interval).await;

        for model in &models {
            let output = tokio::process::Command::new(&wx_pro_path)
                .args(["models"])
                .output()
                .await;

            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Future: parse stdout for latest run info per model,
                // compare against last_runs, and broadcast ModelRun events
                // when a new run is detected.
                //
                // Example detection logic:
                //   if let Some(run_id) = parse_latest_run(&stdout, model) {
                //       if last_runs.get(*model) != Some(&run_id) {
                //           last_runs.insert(model.to_string(), run_id.clone());
                //           let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                //           hub.broadcast(WxEvent::ModelRun {
                //               model: model.to_string(),
                //               date: run_id[..8].to_string(),
                //               hour: run_id[8..].parse().unwrap_or(0),
                //               timestamp: now,
                //           });
                //       }
                //   }
                let _ = (stdout, model, &mut last_runs); // suppress unused warnings
            }
        }

        // Heartbeat — lets clients (and monitoring) know the stream is alive.
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        hub.broadcast(WxEvent::Heartbeat {
            timestamp: now,
            connected_clients: hub.subscriber_count(),
        });
    }
}
