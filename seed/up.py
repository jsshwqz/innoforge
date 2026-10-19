import http.server as h, os
class S(h.BaseHTTPRequestHandler):
    def _c(self):
        self.send_header('Access-Control-Allow-Origin','*')
        self.send_header('Access-Control-Allow-Methods','GET,PUT,OPTIONS')
        self.send_header('Access-Control-Allow-Headers','*')
    def do_OPTIONS(self):
        self.send_response(204); self._c(); self.end_headers()
    def do_GET(self):
        self.send_response(200); self._c(); self.end_headers(); self.wfile.write(b'up')
    def do_PUT(self):
        n=int(self.headers.get('Content-Length',0)); d=self.rfile.read(n)
        p=os.path.join('/root/sync/inbox', self.path.strip('/'))
        os.makedirs(os.path.dirname(p), exist_ok=True)
        open(p,'wb').write(d)
        self.send_response(200); self._c(); self.end_headers(); self.wfile.write(str(len(d)).encode())
    def log_message(self,*a): pass
h.HTTPServer(('0.0.0.0',8001),S).serve_forever()
