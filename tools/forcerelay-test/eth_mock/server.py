from http.server import BaseHTTPRequestHandler, HTTPServer
import re
import os.path

count = 0
dir = os.path.dirname(__file__)

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        match_obj = re.match(r'/eth/v1/beacon/headers/([\d]+)', self.path)
        if match_obj:
            file_path = dir + '/mock_data/headers/{}.json'.format(match_obj.group(1))
            if os.path.exists(file_path):
                self.send_response(200)
                self.send_header('Content-type', 'application/json')
                self.end_headers()
                content = open(file_path, 'rb').read()
                self.wfile.write(content)
            else:
                self.send_response(404)
        elif self.path == '/eth/v1/beacon/light_client/bootstrap/0xe06056afdb9a0a9fd7fbaf89bb0e96eced24de0104bc5b7e3960c115d6990f90':
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            content = open(dir + '/mock_data/bootstrap.json', 'rb').read()
            self.wfile.write(content)
        elif self.path == '/eth/v1/beacon/light_client/updates?start_period=706&count=128':
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            content = open(dir + '/mock_data/updates.json', 'rb').read()
            self.wfile.write(content)
        elif self.path == '/eth/v1/beacon/light_client/updates?start_period=706&count=1':
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            content = open(dir + '/mock_data/updates.json', 'rb').read()
            self.wfile.write(content)
        elif self.path == '/eth/v1/beacon/light_client/finality_update':
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            global count
            count += 1
            if count <= 5:
                content = open(dir + '/mock_data/finality_update1.json', 'rb').read()
                self.wfile.write(content)
            else:
                content = open(dir + '/mock_data/finality_update2.json', 'rb').read()
                self.wfile.write(content)
        else:
            print(self.path)
            self.send_response(404)


if __name__ == '__main__':
    server = HTTPServer(('', 8545), Handler)
    print('start the server')
    server.serve_forever()
