# Insights Engine — Known Limitations

Honest edges of the statistical engine and its reports. Each entry states the
limitation, why it exists, and what would lift it.

## Multiple-comparison correction is per-measure only (Drivers)

`rank_segment_drivers` Bonferroni-corrects each slice's Welch p-value across
the slices tested **for the same measure** — not across measures, dimensions,
or reports. On tables with many measures the family-wise error rate is higher
than the per-measure p-values suggest. On very high-cardinality dimensions the
opposite bites: Bonferroni can over-suppress genuinely moving slices. If that
shows up in practice, the fallback is per-dimension correction (divide the
family by dimension before correcting).

## Null-model calibration is synthetic-only

The false-positive / power tests in `analysis/null_models.rs` run against
deterministic pseudo-noise, not against a corpus of real business tables.
Real data has autocorrelation and heavy tails that the calibration harness
does not model; treat the ~5% FP targets as design intent, not a measured
guarantee.

## Seasonality assumes regularly spaced samples

`detect_seasonality` (see the assumption note at the top of
`analysis/seasonality.rs`) computes autocorrelation at fixed lags over the
row sequence ordered by timestamp. Irregular sampling (bursty event data,
gaps) distorts the lag structure and can both mask real cycles and invent
false ones. Aggregating to a regular period grid before the ACF would fix it.

## Drivers depth is one dimension

The delta decomposition ranks single-dimension slices only. Depth-2 pairs
("region=EU AND channel=web drove it") are deferred: `pair_aggregates` in
`candidates.rs` deliberately skips measure sums at depth 2 to keep the row
pass cheap, so the moments Welch tests need don't exist there yet.

## Null segment values are excluded from slices

`DimensionIndex` drops empty-string dimension values when picking top values,
so rows with a null segment never form their own slice in the derived-series
pass or Drivers. They still count in whole-table totals — a delta driven
mostly by null-segment rows will show a total change that no listed driver
explains. (Segment attribution in Review labels them "(blank)".)

## Ambiguous slash dates parse day-first

`parse_date_string` tries `%d/%m/%Y` before `%m/%d/%Y`, so `03/04/2023` is
April 3rd, not March 4th. Unambiguous values (day > 12) land correctly in
either convention. US-format files with day ≤ 12 will be bucketed into wrong
periods; a table-level date-format setting would resolve it.

## Post-sync auto-runs compute Trends only

The after-sync auto-run recomputes the Trends report (broadest coverage per
unit of compute) — Review and Drivers stay manual. Auto-runs also never write
`insight_history`: "shown" means a human saw it, so findings nobody opened
stay novel and keep the badge honest.

## Measure polarity is display-only (v1)

`SetColumnPolarity` tags findings good/bad for the UI; scoring deliberately
ignores it. The hook point for a future polarity-aware boost is
`scoring::kpi_boost_for`.

## Legacy-detector fingerprints activated retroactively

Since the legacy sites (period comparison, seasonality, concentration, …)
gained fingerprints, they participate in novelty decay and curation for the
first time. On workspaces with existing `insight_history`, ranking will shift
as these findings start decaying — intended, but visible.
