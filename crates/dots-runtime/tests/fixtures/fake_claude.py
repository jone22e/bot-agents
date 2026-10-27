#!/usr/bin/env python3
"""CLI falso que imita `claude -p --output-format stream-json` e fala MCP com o gateway do DOTS.
Comportamento controlado pelo prompt (stdin):
  WRITE:<path>:<content>  -> chama dots_write_file e responde FEITO
  DENY:<path>             -> tenta escrever fora do workspace (espera negacao) e responde NEGADO/ESCREVEU
  SLEEP:<segundos>        -> dorme (para cancelamento/concorrencia) e responde ACORDEI
  RECALL:<query>          -> chama dots_recall e responde o texto
  BROWSE:<url>            -> chama dots_browser_open e responde PAGINA <texto> ou FALHOU <motivo>
  default                 -> responde OK
"""
import json, os, subprocess, sys, time

args = sys.argv[1:]
if args[:1] == ["--version"]:
    print("2.1.300 (Claude Code)"); sys.exit(0)
if args[:2] == ["auth", "status"]:
    print(json.dumps({"loggedIn": True, "authMethod": "fake"})); sys.exit(0)

def arg(flag):
    return args[args.index(flag) + 1] if flag in args else None

session = arg("--session-id") or arg("--resume") or "fake-session"
prompt = sys.stdin.read().strip()
sysprompt = arg("--append-system-prompt") or ""
# O runtime anexa ao pedido um bloco <contexto_da_tarefa> (memorias, checkpoint, retomada). Separa para os
# comandos de teste (DELEGATE:, RUNCMD:...) nao casarem com texto de memorias de tarefas anteriores.
taskctx = ""
if "<contexto_da_tarefa>" in prompt:
    prompt, taskctx = prompt.split("<contexto_da_tarefa>", 1)
    prompt = prompt.strip()

def emit(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()

emit({"type": "system", "subtype": "init", "session_id": session, "tools": [], "model": "fake"})

class Gateway:
    def __init__(self):
        cfg = json.loads(arg("--mcp-config"))["mcpServers"]["dots"]
        env = dict(os.environ); env.update(cfg.get("env", {}))
        self.p = subprocess.Popen([cfg["command"]] + cfg["args"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env, cwd=os.getcwd())
        self.n = 0
        self.req("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "fake", "version": "0"}})
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n"); self.p.stdin.flush()
    def req(self, method, params):
        self.n += 1; i = self.n
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": i, "method": method, "params": params}) + "\n"); self.p.stdin.flush()
        while True:
            line = self.p.stdout.readline()
            if not line: raise RuntimeError("gateway fechou")
            o = json.loads(line)
            if o.get("id") == i: return o
    def call(self, name, arguments):
        permission = self.req("tools/call", {"name": "permission_prompt", "arguments": {"tool_name": f"mcp__dots__{name}", "input": arguments, "tool_use_id": "t1"}})
        r = self.req("tools/call", {"name": name, "arguments": arguments})["result"]
        text = "\n".join(c.get("text", "") for c in r.get("content", []))
        return (not r.get("isError", False)), text

def tool(name, inp, ok, out):
    emit({"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "toolu_1", "name": f"mcp__dots__{name}", "input": inp}]}})
    emit({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_1", "is_error": not ok, "content": [{"type": "text", "text": out}]}]}})

final = "OK"
if 'dots_answer_question(question_id="' in prompt:
    # gerente respondendo a pergunta de um funcionario (pedido delegado)
    qid = prompt.split('dots_answer_question(question_id="', 1)[1].split('"', 1)[0]
    ok, out = Gateway().call("dots_answer_question", {"question_id": qid, "answer": "SQLite"})
    tool("dots_answer_question", {"question_id": qid}, ok, out)
    final = "RESPONDI" if ok else "FALHOU: " + out
elif 'dots_decide_approval(approval_id="' in prompt:
    # gerente decidindo o pedido de permissao de um funcionario
    aid = prompt.split('dots_decide_approval(approval_id="', 1)[1].split('"', 1)[0]
    ok, out = Gateway().call("dots_decide_approval", {"approval_id": aid, "approved": True, "reason": "dentro do pedido"})
    tool("dots_decide_approval", {"approval_id": aid}, ok, out)
    final = "DECIDI" if ok else "FALHOU: " + out
elif prompt.startswith("Continuacao:"):
    final = "CONSOLIDADO: " + str(prompt.count("Resultado:")) + " resultado(s)" + (" | resposta=" + prompt.split("Resposta: ",1)[1].split("\n")[0] if "Resposta: " in prompt else "")
elif prompt.startswith("DELEGATE:") or prompt.startswith("DELEGATE_SERIAL:"):
    serial = prompt.startswith("DELEGATE_SERIAL:")
    _, names, request = prompt.split(":", 2)
    g = Gateway()
    ok, team = g.call("dots_list_team", {})
    tool("dots_list_team", {}, ok, team)
    outs = []
    for n in names.split(","):
        ok, out = g.call("dots_delegate", {"to": n.strip(), "request": request})
        tool("dots_delegate", {"to": n.strip(), "request": request}, ok, out)
        outs.append(("ok" if ok else "ERRO") + ":" + out[:80])
        if serial and ok:
            delegation_id = out.split("Delegacao ", 1)[1].split(" ", 1)[0]
            for _ in range(100):
                _, state = g.call("dots_delegation_status", {"id": delegation_id})
                if ": completed" in state:
                    break
                time.sleep(0.05)
            else:
                raise RuntimeError(f"delegacao {delegation_id} nao terminou")
    final = "AGUARDANDO " + " | ".join(outs)
elif prompt.startswith("ASK:"):
    ok, out = Gateway().call("dots_ask_user", {"question": prompt.split(":", 1)[1], "options": ["Postgres", "SQLite"]})
    tool("dots_ask_user", {}, ok, out)
    final = "PERGUNTEI" if ok else "FALHOU: " + out
elif prompt.startswith("RUNCMD:"):
    cmd = prompt.split(":", 1)[1]
    ok, out = Gateway().call("dots_run_command", {"command": cmd})
    tool("dots_run_command", {"command": cmd}, ok, out)
    if out.startswith("[ja executado"):
        final = "REPLAYED"
    elif "Retomada (tentativa" in taskctx:
        final = "RETOMADO_SEM_REPLAY"
    else:
        time.sleep(30); final = "ACORDEI"
elif prompt.startswith("BROWSE:"):
    url = prompt.split(":", 1)[1]
    ok, out = Gateway().call("dots_browser_open", {"url": url})
    tool("dots_browser_open", {"url": url}, ok, out)
    final = ("PAGINA " if ok else "FALHOU ") + out[:1500]
elif prompt.startswith("CLICK:"):
    # CLICK:<url>|<rotulo>: abre a pagina, acha o [ref=N] da linha com o rotulo e clica nele
    url, label = prompt.split(":", 1)[1].split("|", 1)
    g = Gateway()
    ok, page = g.call("dots_browser_open", {"url": url})
    tool("dots_browser_open", {"url": url}, ok, page)
    line = next((l for l in page.splitlines() if label in l), "")
    ref = int(line.split("[ref=")[1].split("]")[0]) if "[ref=" in line else 0
    ok, out = g.call("dots_browser_click", {"ref": ref})
    tool("dots_browser_click", {"ref": ref}, ok, out)
    final = ("CLICOU " if ok else "FALHOU ") + out[:1500]
elif prompt.startswith("FLEXI:"):
    _, name, raw = prompt.split(":", 2)
    targs = json.loads(raw or "{}")
    ok, out = Gateway().call(name, targs)
    tool(name, targs, ok, out)
    final = ("OK " if ok else "ERRO ") + out[:600]
elif prompt.startswith("TOOLS"):
    g = Gateway()
    r = g.req("tools/list", {})["result"]
    final = "TOOLS " + ",".join(t["name"] for t in r["tools"])
elif prompt.startswith("ECHO_PROMPT"):
    final = prompt[:600]
elif prompt.startswith("WRITE:"):
    _, path, content = prompt.split(":", 2)
    ok, out = Gateway().call("dots_write_file", {"path": path, "content": content})
    tool("dots_write_file", {"path": path, "content": content}, ok, out)
    final = "FEITO" if ok else f"FALHOU: {out}"
elif prompt.startswith("DENY:"):
    path = prompt.split(":", 1)[1]
    ok, out = Gateway().call("dots_write_file", {"path": path, "content": "x"})
    tool("dots_write_file", {"path": path, "content": "x"}, ok, out)
    final = "ESCREVEU" if ok else "NEGADO"
elif prompt.startswith("RESUME:"):
    # gerente retomando a tarefa parada de um funcionario
    ok, out = Gateway().call("dots_resume_delegation", {"id": prompt.split(":", 1)[1]})
    tool("dots_resume_delegation", {}, ok, out)
    final = "RETOMEI" if ok else "FALHOU: " + out
elif prompt.startswith("SLEEP:"):
    time.sleep(float(prompt.split(":", 1)[1])); final = "ACORDEI"
elif prompt.startswith("RECALL:"):
    ok, out = Gateway().call("dots_recall", {"query": prompt.split(":", 1)[1]})
    tool("dots_recall", {}, ok, out)
    final = out
elif prompt.startswith("ECHO_SYSTEM"):
    final = sysprompt[:300]
elif prompt.startswith("ECHO_CONTEXT"):
    final = taskctx[:600]

emit({"type": "assistant", "message": {"content": [{"type": "text", "text": final}]}})
emit({"type": "result", "subtype": "success", "is_error": False, "result": final, "session_id": session, "total_cost_usd": 0.001, "usage": {"input_tokens": 1, "output_tokens": 1}, "permission_denials": []})
