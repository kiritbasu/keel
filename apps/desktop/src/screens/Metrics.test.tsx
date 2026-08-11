/**
 * The metrics screen.
 *
 * The thing worth testing here is the direction. A metric whose target is to go
 * *down* — orientation cost, error rate — reads exactly backwards if you assume
 * higher is better, and a dashboard that says a project is on track when it is
 * not is worse than no dashboard.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { Route } from "../lib/router";

const METRICS = [
  {
    id: "mtr_up",
    type: "metric",
    name: "Tests passing",
    unit: "count",
    target_value: 270,
    direction: "up",
  },
  {
    id: "mtr_down",
    type: "metric",
    name: "Agent orientation cost",
    unit: "tokens",
    target_value: 4000,
    direction: "down",
  },
  { id: "mtr_bare", type: "metric", name: "Never measured", direction: "up" },
];

const OBSERVATIONS = [
  // Above a rising target: on track.
  { id: "obs_1", type: "metric_observation", metric_id: "mtr_up", value: 264, observed_at: "2026-08-09T12:00:00Z" },
  { id: "obs_2", type: "metric_observation", metric_id: "mtr_up", value: 425, observed_at: "2026-08-10T12:00:00Z" },
  // Above a falling target: NOT on track, and the number is bigger.
  { id: "obs_3", type: "metric_observation", metric_id: "mtr_down", value: 9000, observed_at: "2026-08-10T12:00:00Z" },
];

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    metrics: async () => ({ items: METRICS, total: METRICS.length }),
    observations: async () => ({ items: OBSERVATIONS, total: OBSERVATIONS.length }),
  },
}));

const { MetricsScreen } = await import("./Metrics");

const route: Route = { screen: "metrics", project: "keel", query: {} };
const show = () => render(<MetricsScreen route={route} generation={0} />);

afterEach(cleanup);

describe("metrics", () => {
  it("plots every metric with its target", async () => {
    show();
    expect(await screen.findByText("Tests passing")).toBeTruthy();
    expect(screen.getByText("Agent orientation cost")).toBeTruthy();
    expect(screen.getAllByText(/target 270/).length).toBeGreaterThan(0);
  });

  it("reads a falling target the right way round", async () => {
    show();
    await screen.findByText("Agent orientation cost");

    // The headline reading, not the SVG point's tooltip — both carry the same
    // number, and the tooltip has no colour to be wrong about.
    const headline = (value: string) =>
      screen.getAllByText(value).find((n) => n.tagName === "SPAN")!;

    // 425 against a target of 270, higher-is-better: good.
    expect(headline("425").className).toContain("text-good");

    // 9000 against a target of 4000, lower-is-better: not good. Asserting the
    // *absence* of good matters more than the presence of warn — the bug this
    // guards against is a red number rendering green.
    expect(headline("9000").className).not.toContain("text-good");
    expect(headline("9000").className).toContain("text-warn");
  });

  it("says which way is better, so a lone number is never ambiguous", async () => {
    show();
    await screen.findByText("Tests passing");
    expect(screen.getAllByText("lower is better").length).toBe(1);
    expect(screen.getAllByText("higher is better").length).toBe(2);
  });

  it("a metric with no observations says so rather than drawing an empty chart", async () => {
    show();
    await screen.findByText("Never measured");
    expect(screen.getByText("No observations recorded yet.")).toBeTruthy();
  });
});
