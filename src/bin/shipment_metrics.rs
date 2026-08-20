use logistics_business_metrics::{
    infrai_metrics::{MetricsClient, MetricsError},
    shipment_metrics::{metrics_for, DecisionError, ShipmentEvent},
};
use std::{env, process::ExitCode};
use thiserror::Error;

#[derive(Debug, Error)]
enum ServiceError {
    #[error("pass one shipment event as a JSON argument")]
    MissingEvent,
    #[error("invalid shipment event: {0}")]
    InvalidEvent(#[from] serde_json::Error),
    #[error(transparent)]
    Decision(#[from] DecisionError),
    #[error(transparent)]
    Metrics(#[from] MetricsError),
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(count) => {
            println!("reported {count} shipment metric(s)");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<usize, ServiceError> {
    let raw = env::args().nth(1).ok_or(ServiceError::MissingEvent)?;
    let event: ShipmentEvent = serde_json::from_str(&raw)?;
    let points = metrics_for(&event)?;
    let client = MetricsClient::from_env()?;
    for point in &points {
        let receipt = client.report(point).await?;
        let _audit = (&receipt.data, &receipt.metadata);
    }
    Ok(points.len())
}
