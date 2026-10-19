#!/usr/bin/env bash
# Process-level launcher tests; no Flutter build, browser or physical GPU needed.
set -euo pipefail
here=$(cd -P "$(dirname "$0")" && pwd -P)
python3 - "$here/run-web-browser.sh" <<'PY'
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.request

LAUNCHER = Path(sys.argv.pop()).resolve()


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory(prefix='domowik-launcher-test-')
        self.addCleanup(self.work.cleanup)
        self.root = Path(self.work.name)
        scripts = self.root / 'scripts'
        scripts.mkdir()
        self.script = scripts / 'run-web-browser.sh'
        shutil.copy2(LAUNCHER, self.script)
        bundle = self.root / 'build/web'
        bundle.mkdir(parents=True)
        for name in ('index.html', 'domowik_bridge.wasm', 'domowik_renderer.js',
                     'domowik_renderer_bg.wasm'):
            (bundle / name).write_text('launcher test bundle')
        self.record = self.root / 'browser.json'
        self.browser = self.root / 'mock browser'
        self.browser.write_text('''#!'''+sys.executable+'''
import json, os, sys, time, urllib.request
from pathlib import Path
url = sys.argv[-1]
assert urllib.request.urlopen(url).read() == b'launcher test bundle'
Path(os.environ['MOCK_RECORD']).write_text(json.dumps({
    'args': sys.argv[1:], 'drivers': os.environ.get('VK_LOADER_DRIVERS_SELECT'),
    'url': url, 'pid': os.getpid(),
}))
if os.environ.get('MOCK_WAIT'):
    while True: time.sleep(.1)
sys.exit(int(os.environ.get('MOCK_EXIT', '0')))
''')
        self.browser.chmod(0o755)
        self.profile = self.root / 'persistent profile'
        self.env = dict(os.environ, DISPLAY=':test', DOMOWIK_WEB_PORT='0',
                        DOMOWIK_WEB_BROWSER=str(self.browser),
                        DOMOWIK_WEB_PROFILE=str(self.profile),
                        MOCK_RECORD=str(self.record))
        self.env.pop('VK_LOADER_DRIVERS_SELECT', None)
        self.env.pop('MOCK_WAIT', None)
        self.env.pop('MOCK_EXIT', None)

    def run_launcher(self):
        return subprocess.run(['bash', str(self.script)], env=self.env,
                              capture_output=True, text=True, timeout=15)

    def assert_server_stopped(self, url):
        with self.assertRaises(OSError):
            urllib.request.urlopen(url, timeout=1)

    def test_ready_flags_and_browser_exit_cleanup(self):
        result = self.run_launcher()
        self.assertEqual(result.returncode, 0, result.stderr)
        info = json.loads(self.record.read_text())
        self.assertEqual(info['drivers'], '*intel*')
        self.assertIn('--enable-features=Vulkan', info['args'])
        self.assertIn('--ozone-platform=x11', info['args'])
        self.assertIn('--user-data-dir='+str(self.profile), info['args'])
        self.assertFalse(any('unsafe' in a or a == '--no-sandbox' for a in info['args']))
        self.assert_server_stopped(info['url'])
        self.assertTrue(self.profile.is_dir(), 'profile persists on exit')

    def test_browser_failure_and_driver_override(self):
        self.env.update(MOCK_EXIT='27', VK_LOADER_DRIVERS_SELECT='*')
        result = self.run_launcher()
        self.assertEqual(result.returncode, 27, result.stderr)
        info = json.loads(self.record.read_text())
        self.assertEqual(info['drivers'], '*')
        self.assert_server_stopped(info['url'])

    def test_occupied_port_does_not_launch_browser_or_stop_other_server(self):
        with socket.socket() as occupied:
            occupied.bind(('127.0.0.1', 0))
            occupied.listen()
            self.env['DOMOWIK_WEB_PORT'] = str(occupied.getsockname()[1])
            result = self.run_launcher()
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('could not start localhost server', result.stderr)
            self.assertFalse(self.record.exists())
            with socket.create_connection(occupied.getsockname(), timeout=1):
                pass

    def test_interruption_cleans_up_browser_and_server(self):
        for signum, status in ((signal.SIGTERM, 143), (signal.SIGINT, 130)):
            with self.subTest(signal=signum):
                self.record.unlink(missing_ok=True)
                self.assert_interrupted(signum, status)

    def assert_interrupted(self, signum, status):
        self.env['MOCK_WAIT'] = '1'
        # Background verification may inherit SIGINT ignored. Simulate a
        # foreground launch, where Ctrl+C is delivered, and isolate cleanup.
        with subprocess.Popen(['bash', str(self.script)], env=self.env,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              text=True, start_new_session=True,
                              preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL)) as process:
            try:
                for _ in range(100):
                    if self.record.exists():
                        break
                    time.sleep(.05)
                self.assertTrue(self.record.exists(), 'browser must start')
                info = json.loads(self.record.read_text())
                process.send_signal(signum)
                out, err = process.communicate(timeout=10)
                self.assertEqual(process.returncode, status, out+err)
                self.assert_server_stopped(info['url'])
                with self.assertRaises(ProcessLookupError):
                    os.kill(info['pid'], 0)
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.communicate(timeout=5)

    def test_locked_profile_is_not_reused(self):
        self.profile.mkdir()
        (self.profile / 'SingletonLock').symlink_to('existing-browser')
        result = self.run_launcher()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('profile already locked', result.stderr)
        self.assertFalse(self.record.exists())

    def test_invalid_port(self):
        self.env['DOMOWIK_WEB_PORT'] = '65536'
        result = self.run_launcher()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('DOMOWIK_WEB_PORT must be', result.stderr)
        self.assertFalse(self.record.exists())


unittest.main()
PY
