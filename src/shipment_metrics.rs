use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ShipmentEvent {
    Delivered {
        shipment_id: String,
        carrier: String,
        transit_hours: f64,
    },
    ProofOfDeliveryStored {
        shipment_id: String,
        carrier: String,
        proof: ProofOfDelivery,
    },
    ExceptionOpened {
        shipment_id: String,
        carrier: String,
        exception: DeliveryException,
    },
    ExceptionResolved {
        shipment_id: String,
        carrier: String,
        exception: DeliveryException,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProofOfDelivery {
    pub document_id: String,
    pub media_type: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeliveryException {
    pub code: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetricPoint {
    pub name: String,
    pub value: f64,
    #[serde(rename = "type")]
    pub kind: MetricKind,
    pub tags: BTreeMap<String, String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricKind {
    Counter,
    Gauge,
}

#[derive(Debug, Error, PartialEq)]
pub enum DecisionError {
    #[error("shipment_id and carrier must be non-empty")]
    MissingIdentity,
    #[error("transit_hours must be finite and non-negative")]
    InvalidTransitHours,
    #[error("proof-of-delivery document_id and media_type must be non-empty")]
    InvalidProof,
    #[error("exception code must be non-empty")]
    InvalidException,
}

pub fn metrics_for(event: &ShipmentEvent) -> Result<Vec<MetricPoint>, DecisionError> {
    let (shipment_id, carrier) = identity(event);
    if shipment_id.trim().is_empty() || carrier.trim().is_empty() {
        return Err(DecisionError::MissingIdentity);
    }

    let base_tags = BTreeMap::from([("carrier".to_owned(), carrier.to_owned())]);
    let point = match event {
        ShipmentEvent::Delivered { transit_hours, .. } => {
            if !transit_hours.is_finite() || *transit_hours < 0.0 {
                return Err(DecisionError::InvalidTransitHours);
            }
            MetricPoint {
                name: "logistics.shipment.transit_hours".into(),
                value: *transit_hours,
                kind: MetricKind::Gauge,
                tags: base_tags,
                idempotency_key: format!("shipment:{shipment_id}:delivered"),
            }
        }
        ShipmentEvent::ProofOfDeliveryStored { proof, .. } => {
            if proof.document_id.trim().is_empty() || proof.media_type.trim().is_empty() {
                return Err(DecisionError::InvalidProof);
            }
            let mut tags = base_tags;
            tags.insert("media_type".into(), proof.media_type.clone());
            MetricPoint {
                name: "logistics.proof_of_delivery.bytes".into(),
                value: proof.bytes as f64,
                kind: MetricKind::Counter,
                tags,
                idempotency_key: format!("proof:{}:stored", proof.document_id),
            }
        }
        ShipmentEvent::ExceptionOpened { exception, .. }
        | ShipmentEvent::ExceptionResolved { exception, .. } => {
            if exception.code.trim().is_empty() {
                return Err(DecisionError::InvalidException);
            }
            let mut tags = base_tags;
            tags.insert("exception_code".into(), exception.code.clone());
            tags.insert("retryable".into(), exception.retryable.to_string());
            let (value, state) = match event {
                ShipmentEvent::ExceptionOpened { .. } => (1.0, "open"),
                ShipmentEvent::ExceptionResolved { .. } => (0.0, "resolved"),
                _ => unreachable!(),
            };
            MetricPoint {
                name: "logistics.shipment.exception_open".into(),
                value,
                kind: MetricKind::Gauge,
                tags,
                idempotency_key: format!(
                    "shipment:{shipment_id}:exception:{}:{state}",
                    exception.code
                ),
            }
        }
    };
    Ok(vec![point])
}

fn identity(event: &ShipmentEvent) -> (&str, &str) {
    match event {
        ShipmentEvent::Delivered {
            shipment_id,
            carrier,
            ..
        }
        | ShipmentEvent::ProofOfDeliveryStored {
            shipment_id,
            carrier,
            ..
        }
        | ShipmentEvent::ExceptionOpened {
            shipment_id,
            carrier,
            ..
        }
        | ShipmentEvent::ExceptionResolved {
            shipment_id,
            carrier,
            ..
        } => (shipment_id, carrier),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_exception_clears_the_open_gauge() {
        let exception = DeliveryException {
            code: "address_review".into(),
            retryable: true,
        };
        let opened = ShipmentEvent::ExceptionOpened {
            shipment_id: "SHP-42".into(),
            carrier: "northline".into(),
            exception: exception.clone(),
        };
        let resolved = ShipmentEvent::ExceptionResolved {
            shipment_id: "SHP-42".into(),
            carrier: "northline".into(),
            exception,
        };

        let open_point = metrics_for(&opened).unwrap().remove(0);
        let resolved_point = metrics_for(&resolved).unwrap().remove(0);

        assert_eq!(open_point.value, 1.0);
        assert_eq!(resolved_point.value, 0.0);
        assert_eq!(open_point.name, resolved_point.name);
        assert_eq!(resolved_point.kind, MetricKind::Gauge);
        assert_ne!(open_point.idempotency_key, resolved_point.idempotency_key);
    }
}
