import { describe, expect, it } from "vitest";
import { HB_WIDTH, RHYTHMS, heartbeatPath, heartbeatSpan, toneOf } from "./heartbeat";

describe("heartbeat", () => {
  it("covers the window plus one cell and starts on the baseline", () => {
    expect(heartbeatSpan("live")).toBe((Math.ceil(HB_WIDTH / RHYTHMS.live.cell) + 1) * RHYTHMS.live.cell);
    expect(heartbeatPath("live").startsWith("M0,8")).toBe(true);
    expect(heartbeatPath("error")).toBe("M0,8h120h120");
  });
  it("maps the runtime pulse to a tone, unknown is idle", () => {
    expect(toneOf("live")).toBe("live");
    expect(toneOf("waiting")).toBe("waiting");
    expect(toneOf("qualquer")).toBe("idle");
    expect(toneOf(undefined)).toBe("idle");
  });
});
