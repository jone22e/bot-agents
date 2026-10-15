import { describe, expect, it } from "vitest";
import { describeApproval, describeEvent, describeOrigin, describeSchedule, describeSubscription, describeToolCall, dotStateLine, splitPurpose } from "./humanize";

describe("humanize", () => {
  it("describes tool calls in plain language", () => {
    expect(describeToolCall("mcp__dots__dots_write_file", { path: "/a/b/hello.txt" })).toBe("Escreveu o arquivo hello.txt");
    expect(describeToolCall("dots/dots_run_command", { command: "npm test" })).toBe('Rodou o comando "npm test"');
    expect(describeToolCall("call_mcp_tool", { ToolName: "dots_list_dir", Arguments: { path: "/x/y/z" } })).toBe("Olhou a pasta …/y/z");
  });
  it("hides successful tool results and shows failures", () => {
    expect(describeEvent({ type: "tool_result", call_id: "1", name: "", ok: true, output: "" })).toBeNull();
    expect(describeEvent({ type: "failed", kind: "auth", message: "x" })?.tone).toBe("error");
  });
  it("describes approvals", () => {
    expect(describeApproval({ tool: "dots_write_file", resources: ["/w/f.txt"], input: {}, summary: "s" } as never)).toBe("quer escrever o arquivo f.txt");
  });
  it("computes dot state line", () => {
    expect(dotStateLine([]).text).toBe("Livre");
    expect(dotStateLine([{ status: "RUNNING", title: "X" } as never]).text).toBe("Trabalhando em: X");
    expect(dotStateLine([{ status: "WAITING_APPROVAL", title: "Y" } as never]).tone).toBe("attention");
  });
  it("describes origins, triggers and schedules", () => {
    expect(describeOrigin({ origin: "event:1", kind: "task", delegation_chain: [] } as never)).toBe("Um gatilho disparou");
    expect(describeOrigin({ origin: "delegation:1", kind: "task", delegation_chain: ["d1"] } as never, () => "Chefe")).toBe("Pedido de Chefe");
    expect(describeOrigin({ origin: "user", kind: "chat", delegation_chain: [] } as never)).toBe("Voce, na conversa");
    expect(describeSubscription({ event_type: "file.changed", filter: { path_glob: "*.md", source_dot_id: null }, prompt_template: "Revise" } as never)).toBe('Quando um arquivo mudar (*.md), pedir: "Revise"');
    expect(describeSchedule({ kind: "weekly", spec: "1 08:00" } as never)).toBe("Toda segunda as 08:00");
  });
  it("splits purpose", () => {
    const r = splitPurpose("Revisa codigo. Aponta bugs.");
    expect(r.objective).toBe("Revisa codigo.");
    expect(r.instructions).toBe("Revisa codigo. Aponta bugs.");
  });
});
