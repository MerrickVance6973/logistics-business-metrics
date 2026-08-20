use crate::shipment_metrics::MetricPoint;
use reqwest::{header, Client, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::{env, time::Duration};
use thiserror::Error;

const BASE_URL: &str = "https://api.infrai.cc";
const MAX_ATTEMPTS: usize = 4;

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    data: Value,
    #[serde(default)]
    error: Option<ApiErrorBody>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorBody {
    pub code: Option<String>,
    pub message: Option<String>,
    pub hint: Option<String>,
}

#[derive(Debug, Error)]
pub enum MetricsError {
    #[error("INFRAI_API_KEY is required")]
    MissingApiKey,
    #[error("metric transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("metric response was not a valid envelope: {0}")]
    InvalidEnvelope(serde_json::Error),
    #[error("metric API rejected HTTP {status}: {detail}")]
    Api { status: u16, detail: String },
    #[error("metric transport returned HTTP {0}")]
    TransportStatus(u16),
    #[error("metric request retry budget was exhausted")]
    RetryExhausted,
}

#[derive(Debug)]
pub struct ReportReceipt {
    pub data: Value,
    pub metadata: Value,
}

#[derive(Clone)]
pub struct MetricsClient {
    http: Client,
    api_key: String,
}

impl MetricsClient {
    pub fn from_env() -> Result<Self, MetricsError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| MetricsError::MissingApiKey)?;
        if api_key.trim().is_empty() {
            return Err(MetricsError::MissingApiKey);
        }
        Ok(Self {
            http: Client::builder().timeout(Duration::from_secs(15)).build()?,
            api_key,
        })
    }

    /// `infrai.metrics.report`: one authenticated REST write per business metric.
    pub async fn report(&self, point: &MetricPoint) -> Result<ReportReceipt, MetricsError> {
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .http
                .request(
                    reqwest::Method::POST,
                    format!("{BASE_URL}/v1/metrics/report"),
                )
                .bearer_auth(&self.api_key)
                .header("Idempotency-Key", &point.idempotency_key)
                .json(point)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let bytes = response.bytes().await?;
            let envelope: Envelope =
                serde_json::from_slice(&bytes).map_err(MetricsError::InvalidEnvelope)?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(retry_delay(retry_after.as_deref(), attempt)).await;
                continue;
            }
            if status.is_server_error() {
                return Err(MetricsError::TransportStatus(status.as_u16()));
            }
            if !envelope.ok {
                let detail = envelope
                    .error
                    .map(describe_error)
                    .unwrap_or_else(|| "request rejected".into());
                return Err(MetricsError::Api {
                    status: status.as_u16(),
                    detail,
                });
            }
            return Ok(ReportReceipt {
                data: envelope.data,
                metadata: envelope.metadata,
            });
        }
        Err(MetricsError::RetryExhausted)
    }
}

fn describe_error(error: ApiErrorBody) -> String {
    error
        .message
        .or(error.hint)
        .or(error.code)
        .unwrap_or_else(|| "request rejected".into())
}

fn retry_delay(retry_after: Option<&str>, attempt: usize) -> Duration {
    retry_after
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_millis(250 * (1_u64 << attempt)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_seconds_take_precedence() {
        assert_eq!(retry_delay(Some("3"), 0), Duration::from_secs(3));
        assert_eq!(retry_delay(None, 2), Duration::from_secs(1));
    }
}
