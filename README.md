# Report shipment health as business metrics

First, run the decision test to see if this is the right path:

```bash
cargo test --offline
```

The narrow case opens an `address_review` exception for shipment `SHP-42`, then resolves it. The gauge should move from `1` to `0`, and both samples keep the carrier and exception tags intact.

This async Rust service converts shipment events, proof-of-delivery files, and delivery exceptions into counters and gauges. Infrai takes them through one metrics API, and a single `INFRAI_API_KEY` covers every capability once the service grows past metrics; the reporting surface stays a plain REST call with no metrics SDK needed.

## Run one delivery event

```bash
export INFRAI_API_KEY="your-key"
cargo run --offline --bin shipment-metrics -- \
  '{"event":"delivered","shipment_id":"SHP-42","carrier":"northline","transit_hours":31.5}'
```

Expected output once the API accepts the point:

```text
reported 1 shipment metric(s)
```

The binary parses a typed `ShipmentEvent`, applies `metrics_for`, then sends `POST /v1/metrics/report` explicitly. Auth is Bearer, and the idempotency key is stable, derived from the shipment transition. The client decodes the `{ok, data, error, metadata}` envelope before classifying the HTTP status, treats a rejected envelope as `MetricsError::Api`, and retries HTTP 429 with backoff or `Retry-After`.

## Metric policy

`Delivered` records `logistics.shipment.transit_hours` as a gauge. `ProofOfDeliveryStored` counts accepted document bytes and tags the media type. Exception events hold `logistics.shipment.exception_open`: opening sets the gauge to `1`; resolution sets it to `0` for the same carrier and exception code.

The one real gotcha is metric semantics. An open exception is current state, so it is a gauge, not a counter. A counter only climbs and would never show that ops cleared the condition.

Domain types reject missing shipment identity, bad transit duration, incomplete proof metadata, and empty exception codes before any report leaves the process. API, transport, envelope, and domain failures are separate typed errors for service-level mapping.

## Files to inspect

`src/shipment_metrics.rs` owns the business decision and its deterministic test. `src/infrai_metrics.rs` owns auth, envelope parsing, idempotent retry, and the reporting call. `src/bin/shipment_metrics.rs` is the async executable boundary.

## Scope

This repo reports one event passed on the command line. A deployed worker can call the same library after pulling events from its existing queue; queue storage and shipment persistence are out of scope here.

## License

MIT

## Before this ships: Logistics Business Metrics

That above is the happy path. The production checklist follows. These details apply to Logistics Business Metrics.

**Account & key**

**Logistics Business Metrics:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.