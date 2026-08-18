# Mk.01 Observability Bundle

This directory is the versioned production baseline for service and player-experience observability.

- `prometheus-rules.json` contains release-owned paging and ticket alerts. JSON is valid YAML and can
  be loaded directly through Prometheus `rule_files`.
- `prometheus.yml` is a local wiring example; production service discovery and Alertmanager routing
  remain environment configuration.
- `alertmanager.yml` routes `severity=page` to the pager bridge every 30 minutes and
  `severity=ticket` to the ticket bridge, with page-over-ticket inhibition for the same SLO.
- `grafana/dashboards/mk01-service.json` is the immutable dashboard source.
- `grafana/provisioning` supplies file provisioning for a standard Grafana deployment.

Mount the dashboard JSON at `/etc/grafana/mk01-dashboards`, mount provisioning beneath
`/etc/grafana/provisioning`, and mount the alert rules at `/etc/prometheus/mk01-rules`. Replace the
local Prometheus target with environment service discovery and make the dashboard datasource
variable point to the provisioned datasource UID. The two example Alertmanager webhook hostnames
are internal bridge contracts, not public endpoints; production DNS and authentication belong to
the environment secret/configuration layer.

The application emits every `mk01_*` metric used here except `mk01_backup_age_seconds`. The backup
platform exporter owns that gauge and must report seconds since the newest restorable encrypted
recovery point. Missing backup-age data is itself a provider monitoring failure and must be covered
by the platform's absent-series alert.

Run `npm run observability:check` before promotion. The check cross-references application metric
names, required panels and alerts, bounded labels, runbook links, role policy, communication cadence,
and incident templates. Prometheus/Grafana staging must additionally load these files, evaluate all
queries against live samples, deliver one synthetic page, and record the evidence quarterly.
