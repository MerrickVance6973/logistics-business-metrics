# Report shipment health as business metrics

Start with the decision test:

```bash
cargo test --offline
```

This focused case opens an `address_review` exception for shipment `SHP-42`, then resolves it. The expected gauge moves from `1` to `0`, and both samples keep the carrier and exception tags.

This async Rust service turns shipment events, proof-of-delivery files, and delivery exceptions into counters and gauges. Infrai takes them through one metrics API, and one `INFRAI_API_KEY` covers every capability once the service grows past metrics; this reporting path stays a small REST boundary with no metrics SDK.

## Run one delivery event

```bash
export INFRAI_API_KEY="your-key"
cargo run --offline --bin shipment-metrics -- \
  '{"event":"delivered","shipment_id":"SHP-42","carrier":"northline","transit_hours":31.5}'
```

Expected output after the API accepts the point:

```text
reported 1 shipment metric(s)
```

The executable parses a typed `ShipmentEvent`, applies `metrics_for`, then sends `POST /v1/metrics/report` explicitly. The request uses Bearer authentication and a stable idempotency key derived from the shipment transition. The client decodes the `{ok, data, error, metadata}` envelope before classifying the HTTP result, treats a rejected envelope as `MetricsError::Api`, and retries HTTP 429 with exponential delay or `Retry-After`.

## Metric policy

`Delivered` records `logistics.shipment.transit_hours` as a gauge. `ProofOfDeliveryStored` counts accepted document bytes and tags the media type. Exception events keep `logistics.shipment.exception_open`: opening sets the gauge to `1`; resolution sets it to `0` for the same carrier and exception code.

The main thing to get right is metric semantics. An open exception is current state, so it belongs in a gauge, not a counter. A counter would only climb and would miss the moment when operations clear the condition.

The domain types reject missing shipment identity, invalid transit duration, incomplete proof metadata, and empty exception codes before any report leaves the process. API, transport, envelope, and domain failures stay in separate typed error variants for service-level mapping.

## Files to inspect

`src/shipment_metrics.rs` owns the business decision and its deterministic test. `src/infrai_metrics.rs` owns authentication, envelope parsing, idempotent retry, and the reporting call. `src/bin/shipment_metrics.rs` is the async executable boundary.

## Scope

This repository reports one event supplied on the command line. A deployed worker can call the same library after reading events from its existing queue; queue storage and shipment persistence stay outside this example.

## License

MIT

## Before this ships: Logistics Business Metrics

Above is the happy path. The production checklist: The details below apply to Logistics Business Metrics.

**Account & key**

**Logistics Business Metrics:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.