/**
 * Screen 8 — Metrics. Each measure plotted against the target it was given.
 *
 * The data model for this has existed since Phase 0 and nothing ever showed it,
 * which made it the same kind of dead weight as the event log before the task
 * page: recorded faithfully, readable by nobody.
 *
 * A number on its own does not say whether things are going well. "270 tests"
 * needs the target and the direction before it means anything, and a metric
 * whose target is to go *down* — orientation cost, error rate — reads exactly
 * backwards without them. So every metric here carries its target line and its
 * direction, and "on track" is computed rather than eyeballed.
 */

import { useMemo } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Card, Empty, ErrorBox, Spinner, cx } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

interface Metric {
  id: string;
  name: string;
  unit?: string;
  target_value?: number;
  direction?: "up" | "down";
}

interface Point {
  at: number;
  value: number;
  note?: string;
}

/** Whether the latest reading has reached its target, given the direction. */
function onTrack(latest: number, target: number | undefined, direction: string): boolean | null {
  if (target === undefined) return null;
  return direction === "down" ? latest <= target : latest >= target;
}

/**
 * A line chart, hand-drawn as SVG.
 *
 * No chart library, for the reason the components are hand-written (B-14): what
 * this needs is a polyline, a dashed target and a few labels, and pulling in a
 * charting dependency for that is more surface than it saves.
 *
 * The y-axis always includes the target, even when every reading sits far from
 * it. A chart scaled to the data alone can put the target off-screen, which
 * hides the one comparison the chart exists to make.
 */
function Chart({ points, target }: { points: Point[]; target?: number }) {
  const w = 560;
  const h = 140;
  const pad = { top: 10, right: 10, bottom: 4, left: 10 };

  const values = points.map((p) => p.value);
  const candidates = target === undefined ? values : [...values, target];
  let lo = Math.min(...candidates);
  let hi = Math.max(...candidates);
  if (lo === hi) {
    // A flat series has no range to scale to. Give it one so the line lands in
    // the middle rather than on an edge, where it reads as a boundary.
    lo -= 1;
    hi += 1;
  }
  const span = hi - lo;

  const x = (i: number) =>
    points.length === 1
      ? (w - pad.left - pad.right) / 2 + pad.left
      : pad.left + (i / (points.length - 1)) * (w - pad.left - pad.right);
  const y = (v: number) => pad.top + (1 - (v - lo) / span) * (h - pad.top - pad.bottom);

  const line = points.map((p, i) => `${x(i)},${y(p.value)}`).join(" ");
  const last = points[points.length - 1];

  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="h-36 w-full" role="img" aria-label="Observations over time">
      {target !== undefined && (
        <>
          <line
            x1={pad.left}
            x2={w - pad.right}
            y1={y(target)}
            y2={y(target)}
            className="stroke-warn/60"
            strokeDasharray="4 4"
            strokeWidth={1}
          />
          <text x={w - pad.right} y={y(target) - 4} textAnchor="end" className="fill-warn/80 text-[10px]">
            target {target}
          </text>
        </>
      )}

      <polyline points={line} fill="none" className="stroke-accent" strokeWidth={2} />
      {points.map((p, i) => (
        <circle key={i} cx={x(i)} cy={y(p.value)} r={2.5} className="fill-accent">
          <title>
            {p.value}
            {p.note ? ` — ${p.note}` : ""}
          </title>
        </circle>
      ))}
      {last && (
        <circle cx={x(points.length - 1)} cy={y(last.value)} r={4} className="fill-accent" />
      )}
    </svg>
  );
}

export function MetricsScreen({ route, generation }: ScreenProps) {
  const project = route.project ?? "";

  const metrics = useAsync(() => api.metrics(project), [project, generation]);
  const observations = useAsync(() => api.observations(project), [project, generation]);

  const byMetric = useMemo(() => {
    const out = new Map<string, Point[]>();
    for (const row of observations.data?.items ?? []) {
      const o = row as unknown as {
        metric_id: string;
        value: number;
        observed_at: string;
        note?: string;
      };
      const at = Date.parse(o.observed_at);
      if (Number.isNaN(at)) continue;
      const list = out.get(o.metric_id) ?? [];
      list.push({ at, value: o.value, note: o.note ?? undefined });
      out.set(o.metric_id, list);
    }
    // Oldest first: a line that reads right-to-left is a line nobody trusts.
    for (const list of out.values()) list.sort((a, b) => a.at - b.at);
    return out;
  }, [observations.data]);

  if ((metrics.loading && !metrics.data) || (observations.loading && !observations.data)) {
    return <Spinner label="Reading the measurements…" />;
  }

  const error = metrics.error ?? observations.error;
  if (error) {
    return (
      <Page title="Metrics" crumbs={projectCrumbs(route, "Metrics")}>
        <ErrorBox error={error} retry={metrics.error ? metrics.reload : observations.reload} />
      </Page>
    );
  }

  const rows = (metrics.data?.items ?? []) as unknown as Metric[];

  return (
    <Page
      title="Metrics"
      crumbs={projectCrumbs(route, "Metrics")}
      meta={
        <span className="text-small text-ink-faint">
          {rows.length} metric{rows.length === 1 ? "" : "s"}
        </span>
      }
    >
      {rows.length === 0 ? (
        <Empty
          message="Nothing is being measured."
          hint="Ask Claude to track a metric and record observations against it."
        />
      ) : (
        <div className="grid gap-5 lg:grid-cols-2">
          {rows.map((m) => {
            const points = byMetric.get(m.id) ?? [];
            const latest = points[points.length - 1];
            const track = latest ? onTrack(latest.value, m.target_value, m.direction ?? "up") : null;

            return (
              <Card
                key={m.id}
                title={m.name}
                actions={
                  <span className="text-micro text-ink-faint">
                    {m.direction === "down" ? "lower is better" : "higher is better"}
                  </span>
                }
              >
                <div className="mb-3 flex items-baseline gap-3">
                  <span
                    className={cx(
                      "text-title font-semibold tabular-nums",
                      track === null ? "text-ink" : track ? "text-good" : "text-warn",
                    )}
                  >
                    {latest ? latest.value : "—"}
                  </span>
                  {m.unit && <span className="text-small text-ink-muted">{m.unit}</span>}
                  {m.target_value !== undefined && (
                    <span className="ml-auto text-small text-ink-faint">
                      target {m.target_value}
                      {m.unit ? ` ${m.unit}` : ""}
                    </span>
                  )}
                </div>

                {points.length === 0 ? (
                  <Empty message="No observations recorded yet." />
                ) : (
                  <>
                    <Chart points={points} target={m.target_value} />
                    <p className="mt-2 text-micro text-ink-faint">
                      {points.length} observation{points.length === 1 ? "" : "s"}
                      {latest && ` · latest ${new Date(latest.at).toLocaleDateString()}`}
                    </p>
                  </>
                )}
              </Card>
            );
          })}
        </div>
      )}
    </Page>
  );
}
