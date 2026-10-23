#!/usr/bin/env python3
"""Flexi falso para testes: login (com 2FA opcional), login pelo navegador, /api/me por cookie, foto, rotas de anuncios e Google Ads."""
import json, sys, time
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs, urlsplit

PORT = int(sys.argv[1]); MFA = "--mfa" in sys.argv
SESSIONS = {}  # cookie -> user
STATE = {"anuncios": {101: {"id": 101, "nome": "Furadeira X", "preco": 199.9, "preco_minimo": 150.0, "estoque": 5, "status": "active"}, 102: {"id": 102, "nome": "Serra Y", "preco": 89.0, "preco_minimo": 80.0, "estoque": 0, "status": "paused"}}, "logins": 0, "expire_next": False, "browser": None, "especialidades": None}
USER = {"id": 7, "usuario_id": 7, "nome": "Agente Teste", "email": "agente@teste.com", "connection": {"base": "c2s_teste"}, "administrador": 0, "token": "SEGREDO"}

PHOTO = bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010806000000")

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def _json(self, code, obj, cookie=None):
        body = json.dumps(obj).encode()
        self.send_response(code); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body)))
        if cookie: self.send_header("Set-Cookie", f"userdata={cookie}; Path=/; HttpOnly")
        self.end_headers(); self.wfile.write(body)
    def _body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(n) or b"{}") if n else {}
    def _user(self):
        c = self.headers.get("Cookie", "")
        for p in c.split(";"):
            p = p.strip()
            if p.startswith("userdata="):
                return SESSIONS.get(p[9:])
        return None
    def do_POST(self):
        path = self.path.split("?")[0]; b = self._body()
        if path == "/api/auth/login":
            STATE["logins"] += 1
            if b.get("email") != "agente@teste.com" or b.get("senha") != "segredo123": return self._json(401, {"status": 0, "error": "invalid_credentials"})
            if MFA: return self._json(202, {"status": 2, "mfa_required": True, "challenge_token": "chal-1", "expires_at": "2099-01-01T00:00:00Z"})
            cookie = f"sess-{STATE['logins']}"; SESSIONS[cookie] = USER
            return self._json(200, {"status": 1, "user": USER, "empresa": {"id": 1, "razao": "Empresa Teste"}}, cookie)
        if path == "/api/auth/mfa/totp/login/verify":
            if b.get("challenge_token") == "chal-1" and b.get("code") == "123456":
                cookie = f"sess-mfa-{STATE['logins']}"; SESSIONS[cookie] = USER
                return self._json(200, {"status": 1, "user": USER, "empresa": {"id": 1, "razao": "Empresa Teste"}}, cookie)
            return self._json(401, {"status": 0, "motivo": "codigo invalido"})
        if path == "/api/auth/logout": return self._json(200, {"status": "ok"})
        if path == "/api/auth/app-externo/desafios":
            if b.get("app") != "dots": return self._json(400, {"error": "invalid_app"})
            STATE["browser"] = "pending"
            return self._json(200, {"status": "pending", "token": "tok-1", "codigo": "ABC-123", "authorize_path": "/login-external-app?token=tok-1", "expires_at": "2099-01-01T00:00:00Z", "poll_interval_ms": 10})
        u = self._user()
        if not u: return self._json(401, {"status": 0, "motivo": "Nao autenticado."})
        if path == "/api/gestao-marketplaces/anuncios":
            ids = b.get("ids"); items = [a for a in STATE["anuncios"].values() if not ids or a["id"] in ids]
            return self._json(200, {"status": 1, "dados": items})
        if path == "/api/gestao-marketplaces/anuncios/atualizar-preco":
            a = STATE["anuncios"].get(int(b.get("id", 0)))
            if not a: return self._json(200, {"status": 0, "motivo": "Anuncio nao encontrado."})
            if float(b["preco"]) < a["preco_minimo"]: return self._json(200, {"status": 0, "motivo": "Preco abaixo do minimo."})
            a["preco"] = float(b["preco"]); return self._json(200, {"status": 1, "operation_id": "op-1", "preco": a["preco"]})
        if path == "/api/gestao-marketplaces/anuncios/pausar":
            for i in b.get("ids", []): STATE["anuncios"][int(i)]["status"] = "paused"
            return self._json(200, {"status": 1, "batch_id": "batch-1"})
        if path == "/api/gestao-marketplaces/anuncios/publicar":
            return self._json(200, {"status": 1, "publicado": b.get("id")})
        if path.startswith("/api/google/ads/dashboard/campanhas/") and path.endswith("/pausar"):
            if b.get("confirm") is not True: return self._json(200, {"status": 0, "error": "confirmation_required"})
            return self._json(200, {"status": 1, "campanha": path.split("/")[-2], "paused": True})
        return self._json(404, {"status": 0, "motivo": "rota desconhecida"})
    def do_GET(self):
        parsed = urlsplit(self.path)
        path = parsed.path
        query = parse_qs(parsed.query)
        if path == "/api/me":
            u = self._user()
            if STATE["expire_next"]:
                STATE["expire_next"] = False; SESSIONS.clear(); u = None
            return self._json(200, {"authenticated": True, "user": u} if u else {"authenticated": False})
        if path == "/__expire":  # utilitario de teste: invalida todas as sessoes
            SESSIONS.clear(); return self._json(200, {"ok": True})
        if path == "/__especialidades":  # utilitario de teste: liga a rota de especialidades (sem isso, Flexi antigo = 404)
            STATE["especialidades"] = [
                {"id": 2, "codigo": "ml_manager", "nome": "Gerente de Mercado Livre", "setor": "mercadolivre", "setor_nome": "Mercado Livre", "permissao": "read", "instrucoes": "Coordena o Mercado Livre.", "ordem": 2},
                {"id": 12, "codigo": "gerente_de_compras", "nome": "Gerente de Compras", "setor": "compras", "setor_nome": "Compras", "permissao": "write", "instrucoes": "Coordena as compras.", "ordem": 12},
                {"id": 1, "codigo": "mkt_manager", "nome": "Gerente de Marketplaces", "setor": "marketplaces", "setor_nome": "", "permissao": "read", "instrucoes": "Setor fora da lista.", "ordem": 1},
            ]
            return self._json(200, {"ok": True})
        if path == "/api/sub-aplicativos/agentes-dots/especialidades":
            if STATE["especialidades"] is None: return self._json(404, {"message": "Route GET:/api/sub-aplicativos/agentes-dots/especialidades not found", "error": "Not Found"})
            if not self._user(): return self._json(401, {"error": "unauthorized"})
            return self._json(200, {"status": 1, "items": STATE["especialidades"]})
        if path == "/__logins": return self._json(200, {"logins": STATE["logins"]})
        if path in ("/__approve", "/__deny"):  # utilitario de teste: o usuario responde no navegador
            STATE["browser"] = "approved" if path == "/__approve" else "denied"; return self._json(200, {"ok": True})
        if path == "/api/auth/app-externo/desafios/tok-1":
            if STATE["browser"] == "approved":
                STATE["browser"] = "consumed"; cookie = "sess-browser"; SESSIONS[cookie] = USER
                return self._json(200, {"status": "authenticated", "usuario_id": 7, "email": USER["email"]}, cookie)
            if STATE["browser"] is None: return self._json(404, {"error": "auth_external_app_challenge_not_found"})
            return self._json(200, {"status": STATE["browser"]})
        if path == "/__photo.png":
            self.send_response(200); self.send_header("Content-Type", "binary/octet-stream"); self.send_header("Content-Length", str(len(PHOTO))); self.end_headers(); self.wfile.write(PHOTO); return
        u = self._user()
        if not u: return self._json(401, {"status": 0, "motivo": "Nao autenticado."})
        if path == "/api/marketplaces/mercadolivre/contas": return self._json(200, {"status": 1, "dados": [{"id": 1, "nome": "Loja ML", "marketplace": 0, "ambiente": "producao"}]})
        if path in ("/api/gestao-marketplaces/anuncios/precos-calculados", "/api/gestao-marketplaces/anuncios/historico-preco"):
            anuncio_id = query.get("anuncio_id", [""])[0]
            if not anuncio_id.isdigit(): return self._json(200, {"status": 0, "motivo": "Anúncio não informado."})
            anuncio = STATE["anuncios"].get(int(anuncio_id))
            if not anuncio: return self._json(200, {"status": 0, "motivo": "Anúncio do marketplace não encontrado."})
            if path.endswith("/precos-calculados"):
                return self._json(200, {"status": 1, "dados": {"id": anuncio["id"], "preco_sugerido": anuncio["preco"], "preco_minimo": anuncio["preco_minimo"]}})
            return self._json(200, {"status": 1, "dados": []})
        if path == "/api/google/ads/contas": return self._json(200, {"status": 1, "dados": [{"id": 3, "customer_id": "123-456-7890"}]})
        if path == "/api/me/profile-photo": return self._json(200, {"url": f"http://127.0.0.1:{PORT}/__photo.png"})
        if path == "/api/perguntas-new/threads": return self._json(200, {"status": 1, "dados": [], "page": 1})
        return self._json(404, {"status": 0, "motivo": "rota desconhecida"})

HTTPServer(("127.0.0.1", PORT), H).serve_forever()
