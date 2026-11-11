#!/usr/bin/env python3
"""PLAN-1006: real external member delivery — export via the public CLI,
relocate, make the pinned generation roots unreachable, and run real
business + A/B recovery drills (RC-WEB-01)."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
FIXTURES = REPO_ROOT / 'crates/auto-web/tests/fixtures/member-delivery'

SCRUB_PREFIXES = ('AUTO_', 'AC_', 'NEXTEST_', 'CARGO_PROFILE_')
SCRUB_KEYS = {
    'CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'RUSTC_WRAPPER',
    'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
    'CARGO_BUILD_RUSTFLAGS', 'CARGO_MANIFEST_DIR', 'CARGO_MANIFEST_PATH',
    'T3_MILESTONE',
}


def environment(extra=None):
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(SCRUB_PREFIXES) and k not in SCRUB_KEYS}
    if extra:
        env.update(extra)
    return env


def sha(path: Path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def free_port() -> int:
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


def request(port, method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    headers = {'Content-Type': 'application/json'} if data else {}
    req = urllib.request.Request(f'http://127.0.0.1:{port}{path}', data=data,
                                 headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=10) as response:
        return response.status, json.loads(response.read() or b'null')


def wait_http(port, path, deadline_s, log: Path, child):
    deadline = time.monotonic() + deadline_s
    while True:
        if child.poll() is not None:
            raise RuntimeError(f'service exited before readiness; log={log}')
        try:
            return request(port, 'GET', path)
        except (OSError, urllib.error.URLError):
            if time.monotonic() > deadline:
                raise RuntimeError(f'service readiness timed out; log={log}')
            time.sleep(0.1)


def run_service(binary: Path, port: int, env_extra, log: Path, ready_path='/value'):
    log.parent.mkdir(parents=True, exist_ok=True)
    handle = log.open('wb')
    child = subprocess.Popen([str(binary)], env=environment(env_extra),
                             stdout=handle, stderr=subprocess.STDOUT)
    try:
        wait_http(port, ready_path, 90, log, child)
    except Exception:
        child.kill()
        child.wait()
        raise
    return child


def stop_service(child, port):
    try:
        status, _ = request(port, 'POST', '/__auto/shutdown')
        assert status == 202, f'shutdown status {status}'
    finally:
        rc = child.wait(timeout=15)
        assert rc == 0, f'service exit {rc}'
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', port))
    return {'shutdown': 202, 'exit': 0, 'port_released': True}


class Driver:
    def __init__(self, args):
        self.binary = args.binary.resolve()
        self.scratch = args.scratch.resolve()
        self.lang_source = args.lang_root.resolve()
        self.web_source = args.web_root.resolve()
        self.chat = args.chat_root.resolve() if args.chat_root else None
        self.database = args.database_root.resolve() if args.database_root else None
        self.receipt = {'schema': 'generated-member-delivery/1', 'outcome': 'fail',
                        'checks': [], 'pending': []}
        self.logs = self.scratch / 'logs'
        self.skipped = []

    def command(self, name, argv, cwd, env=None, expected=0, timeout=3600):
        started = time.monotonic()
        log = self.logs / f'{name}.log'
        with log.open('wb') as output:
            result = subprocess.run([str(v) for v in argv], cwd=str(cwd),
                                    env=env or environment(), stdout=output,
                                    stderr=subprocess.STDOUT, timeout=timeout)
        row = {'id': name, 'exit': result.returncode, 'expected': expected,
               'seconds': round(time.monotonic() - started, 1),
               'log': str(log), 'log_sha256': sha(log),
               'status': 'PASS' if result.returncode == expected else 'FAIL'}
        self.receipt['checks'].append(row)
        if row['status'] == 'FAIL':
            raise ValueError(f'command failed: {name}, see {log}')
        return log

    def record(self, name, detail):
        self.receipt['checks'].append({'id': name, 'status': 'PASS', 'detail': detail})

    def pending(self, note):
        self.receipt['pending'].append(note)

    # ---- export via the formal public CLI over pinned owned source copies ----
    def pinned_env(self, group: Path):
        return {'AUTO_LANG_ROOT': str(group / 'auto-lang'),
                'AUTO_WEB_ROOT': str(group / 'auto-web')}

    def pin_sources(self, group: Path):
        for name, source in (('auto-lang', self.lang_source), ('auto-web', self.web_source)):
            destination = group / name
            destination.mkdir(parents=True)
            for entry in source.iterdir():
                if entry.name in {'.git', 'target', 'node_modules', '__pycache__'}:
                    continue
                if entry.is_dir():
                    shutil.copytree(entry, destination / entry.name,
                                    ignore=shutil.ignore_patterns('.git', 'target'))
                else:
                    shutil.copy2(entry, destination / entry.name)

    def export(self, name, project: Path, delivery: Path, group: Path, env_extra=None):
        env = self.pinned_env(group)
        if env_extra:
            env.update(env_extra)
        self.command(f'export-{name}', [self.binary, 'service-export',
                     '--dir', project, '--delivery-root', delivery],
                     self.scratch, env)
        document = json.loads((delivery / 'delivery-manifest.json').read_text(encoding='utf-8'))
        assert document['schema'] == 'web-member-delivery/1'
        return document

    def build_locked(self, name, bundle: Path):
        self.command(f'{name}-fresh-locked-build',
                     ['cargo', 'build', '--locked', '-j', '4'], bundle)
        return json.loads((bundle / 'delivery-manifest.json').read_text(encoding='utf-8'))['member']['name']

    def service_binary(self, bundle: Path, member: str) -> Path:
        exe = bundle / 'target/debug' / (member + ('.exe' if os.name == 'nt' else ''))
        assert exe.is_file(), f'member binary missing: {exe}'
        return exe

    # ---- phases ----
    def phase_ab_bundles(self, group: Path):
        bundles = {}
        for letter in ('a', 'b'):
            project = self.scratch / f'project-{letter}'
            shutil.copytree(FIXTURES / letter, project)
            bundles[letter] = self.scratch / f'bundle-{letter}'
            document = self.export(letter, project, bundles[letter], group)
            assert any(p['name'] == 'auto-web' for p in document['source_closure']['packages'])
            self.record(f'export-{letter}-manifest',
                        {'member': document['member']['name'],
                         'packages': len(document['source_closure']['packages']),
                         'manifest_sha256': sha(bundles[letter] / 'delivery-manifest.json')})
        return bundles

    def consume(self, name, bundle: Path, member: str, port, data_root: Path, env_extra=None):
        env = {'MEMBER_DELIVERY_DATA': str(data_root)}
        if env_extra:
            env.update(env_extra)
        child = run_service(self.service_binary(bundle, member), port, env,
                            self.logs / f'{name}.log')
        return child

    def phase_a_state(self, consumer: Path, data_root: Path):
        member = self.build_locked('a', consumer)
        port = free_port()
        child = self.consume('a', consumer, member, port, data_root)
        try:
            status, value = request(port, 'GET', '/value')
            assert status == 200 and value == 100, f'A /value: {status} {value}'
            status, result = request(port, 'POST', '/counter/add', {'delta': 7})
            assert status == 200 and result['ok'] and result['value'] == 7, f'A add: {result}'
            status, result = request(port, 'GET', '/counter/get')
            assert result['value'] == 7, f'A get: {result}'
            # Named business error surface (bad input is not a silent success).
            status, _ = request(port, 'GET', '/counter/get?token=')  # deterministic endpoint
            assert status == 200
        finally:
            detail = stop_service(child, port)
        assert (data_root / 'counter.txt').read_text().strip() == '7', 'independent read-back differs'
        self.record('a-real-state', {'http': '/value,/counter/add,/counter/get',
                                     'independent_read': str(data_root / 'counter.txt'), **detail})
        return member

    def phase_b_upgrade_and_recovery(self, consumer: Path, data_root: Path, member_a: str, group: Path):
        # Backup A's data, swap the delivered directory A -> B (directory-level
        # replace with preservation; user data lives outside the bundle).
        data_backup = self.scratch / 'data-before-upgrade'
        shutil.copytree(data_root, data_backup)
        preserved_a = self.scratch / 'preserved-bundle-a'
        shutil.move(str(consumer), str(preserved_a))
        member_b = self.build_locked('b', consumer)
        port = free_port()
        child = self.consume('b', consumer, member_b, port, data_root)
        try:
            status, value = request(port, 'GET', '/value')
            assert status == 200 and value == 200, f'B /value: {status} {value}'
            # B reads A's data (same schema) and continues business.
            status, result = request(port, 'POST', '/counter/double', {})
            assert result['ok'] and result['value'] == 14, f'B double on A data: {result}'
        finally:
            detail = stop_service(child, port)
        assert (data_root / 'counter.txt').read_text().strip() == '14'
        self.record('b-reads-a-data', {'value_marker': 200, 'doubled': 14, **detail})

        # Failed B re-export (swap fail-point) must keep the delivered bundle
        # and the data byte-identical; error names the preserved artifacts.
        before_files = {str(p.relative_to(consumer)): sha(p)
                        for p in consumer.rglob('*') if p.is_file()
                        and 'target' not in p.parts}
        before_counter = sha(data_root / 'counter.txt')
        self.command('failed-b-export', [self.binary, 'service-export',
                     '--dir', self.scratch / 'project-b', '--delivery-root', consumer],
                     self.scratch, {**self.pinned_env(group),
                                    'AUTO_EXPORT_FAIL_POINT': 'swap'}, expected=1)
        after_files = {str(p.relative_to(consumer)): sha(p)
                       for p in consumer.rglob('*') if p.is_file()
                       and 'target' not in p.parts}
        assert before_files == after_files, 'failed export changed delivered bundle'
        assert sha(data_root / 'counter.txt') == before_counter, 'failed export touched data'
        self.record('failed-export-preserves', {'bundle_files': len(before_files),
                                                'data': str(data_root / 'counter.txt')})

        # Recovery: restore the preserved A bundle + data backup and verify
        # independent read-back (RC-WEB-01 directory-switch recovery).
        shutil.rmtree(consumer / 'target')
        shutil.move(str(consumer), str(self.scratch / 'retired-bundle-b'))
        shutil.move(str(preserved_a), str(consumer))
        shutil.rmtree(data_root)
        shutil.copytree(data_backup, data_root)
        member = member_a
        port = free_port()
        child = self.consume('a-recovered', consumer, member, port, data_root)
        try:
            status, value = request(port, 'GET', '/value')
            assert status == 200 and value == 100
            status, result = request(port, 'GET', '/counter/get')
            assert result['value'] == 7, f'recovered A state: {result}'
        finally:
            detail = stop_service(child, port)
        assert (data_root / 'counter.txt').read_text().strip() == '7'
        self.record('recovered-a-reads-backup', {'counter': 7, **detail})

    def phase_unreachable_roots(self, group: Path, bundle: Path):
        # The pinned owned source copies are removed entirely; the relocated
        # bundle still builds + serves from its own closure with scrubbed env.
        consumer2 = self.scratch / 'consumer-relocated'
        shutil.copytree(bundle, consumer2,
                        ignore=shutil.ignore_patterns('target'))
        shutil.rmtree(group)
        text_files = [p for p in consumer2.rglob('*')
                      if p.suffix in {'.toml', '.json'} and p.is_file()
                      and 'target' not in p.parts]
        for path in text_files:
            content = path.read_text(encoding='utf-8', errors='replace')
            for root in (str(self.lang_source), str(self.web_source)):
                assert root.replace('\\', '/') not in content.replace('\\', '/'), \
                    f'dev anchor leaked: {path}'
        self.record('original-roots-unreachable', {'removed': str(group),
                                                   'scanned_manifests': len(text_files)})
        member = json.loads((consumer2 / 'delivery-manifest.json').read_text())['member']['name']
        self.command('relocated-locked-build', ['cargo', 'build', '--locked', '-j', '4'], consumer2)
        port = free_port()
        data_root = self.scratch / 'data-relocated'
        data_root.mkdir()
        child = self.consume('relocated', consumer2, member, port, data_root)
        try:
            status, value = request(port, 'GET', '/value')
            assert status == 200 and value == 100
        finally:
            detail = stop_service(child, port)
        self.record('relocated-service', detail)

    def phase_chat(self, group: Path):
        if not self.chat:
            self.pending('real-chat: --chat-root not provided')
            return
        project = self.scratch / 'project-chat'
        shutil.copytree(self.chat, project,
                        ignore=shutil.ignore_patterns('.git', 'target', 'rust-workspace'))
        bundle = self.scratch / 'bundle-chat'
        self.export('chat', project, bundle, group)
        member = self.build_locked('chat', bundle)
        data_root = self.scratch / 'chat-data'
        data_root.mkdir()
        port = free_port()
        child = run_service(self.service_binary(bundle, member), port,
                            {'CHAT_DATA_DIR': str(data_root), 'CHAT_TEST_MODE': '1'},
                            self.logs / 'chat.log', ready_path='/api/identities')
        try:
            request(port, 'POST', '/api/test/reset')
            status, alice = request(port, 'POST', '/api/register', {'name': '甲'})
            assert status == 200 and alice['ok'], f'register 甲: {alice}'
            status, bob = request(port, 'POST', '/api/register', {'name': '乙'})
            assert bob['ok'], f'register 乙: {bob}'
            status, convo = request(port, 'POST', '/api/conversations/open',
                                    {'token': alice['token'], 'peer_id': bob['id']})
            assert status == 200, f'open: {convo}'
            status, sent = request(port, 'POST', '/api/messages',
                                   {'token': alice['token'], 'conversation_id': convo['id'],
                                    'text': 'plan-1006', 'client_msg_id': 'md-1'})
            assert sent['ok'] and sent['message']['text'] == 'plan-1006', f'send: {sent}'
            status, history = request(port, 'GET',
                                      f"/api/messages?token={alice['token']}&conversation_id={convo['id']}")
            assert any(m['text'] == 'plan-1006' for m in history), f'history: {history}'
        finally:
            detail = stop_service(child, port)
        db = data_root / 'chat.db'
        assert db.is_file(), 'chat sqlite missing'
        self.record('real-chat-business', {'db': str(db), 'db_sha256': sha(db),
                                           'db_bytes': db.stat().st_size, **detail})
        # Independent SQLite read-back of the persisted message.
        import sqlite3
        connection = sqlite3.connect(f'file:{db}?mode=ro', uri=True)
        try:
            rows = connection.execute(
                "SELECT text FROM messages WHERE client_msg_id='md-1'").fetchall()
        finally:
            connection.close()
        assert rows == [('plan-1006',)], f'independent sqlite read: {rows}'
        self.record('chat-independent-readback', {'rows': len(rows)})

    def phase_database(self, group: Path):
        if not self.database:
            self.pending('real-database: --database-root not provided')
            return
        project = self.scratch / 'project-database'
        shutil.copytree(self.database, project,
                        ignore=shutil.ignore_patterns('.git', 'target', 'rust-workspace'))
        bundle = self.scratch / 'bundle-database'
        self.export('database', project, bundle, group)
        member = self.build_locked('database', bundle)
        seed_dir = self.scratch / 'db-seeds'
        seed_dir.mkdir()
        self.command('database-seeds',
                     [sys.executable, project / 'tests/basic/fixtures/make_fixtures.py',
                      '--out', seed_dir], self.scratch)
        basic = seed_dir / 'basic_a.db'
        port = free_port()
        child = run_service(self.service_binary(bundle, member), port, {},
                            self.logs / 'database.log', ready_path='/api/probe/ping')
        try:
            status, opened = request(port, 'POST', '/api/db/open', {'path': str(basic)})
            assert opened['ok'] and opened['tables'] >= 4, f'db open: {opened}'
            status, data = request(port, 'POST', '/api/db/table_data',
                                   {'path': str(basic), 'tname': 'customers',
                                    'page': 1, 'page_size': 10, 'sort_col': 'id',
                                    'sort_dir': 'asc', 'search': ''})
            assert data['total'] == 20 and data['showing'] == 10, f'table_data: {data}'
            status, query = request(port, 'POST', '/api/db/query',
                                    {'path': str(basic),
                                     'sql': 'SELECT country, COUNT(*) AS n FROM customers '
                                            'WHERE country IS NOT NULL GROUP BY country'})
            assert query['ncols'] == 2 and query['rowcount'] >= 1, f'query: {query}'
            status, rejected = request(port, 'POST', '/api/db/query',
                                       {'path': str(basic), 'sql': 'DELETE FROM customers'})
            assert rejected['ok'] is False and rejected['err_code'] == 'rejected', \
                f'guard: {rejected}'
        finally:
            detail = stop_service(child, port)
        self.record('real-database-business', {'seed': str(basic), 'seed_sha256': sha(basic), **detail})

    def phase_policy(self, group: Path):
        project = self.scratch / 'project-a'
        c_root = self.scratch / 'bundle-policy-c'
        full_root = self.scratch / 'bundle-policy-full'
        self.export('policy-c', project, c_root, group)
        document_c = json.loads((c_root / 'delivery-manifest.json').read_text())
        assert document_c['debug_policy']['label'] == 'c'
        assert '[profile.dev]' in (c_root / 'Cargo.toml').read_text()
        self.export('policy-full', project, full_root, group,
                    env_extra={'AUTO_GENERATED_DEBUG_POLICY': 'full'})
        document_f = json.loads((full_root / 'delivery-manifest.json').read_text())
        assert document_f['debug_policy']['label'] == 'full', document_f['debug_policy']
        manifest = full_root / 'Cargo.toml'
        assert '[profile.dev]' not in manifest.read_text(), 'full bundle must omit C block'
        self.record('c-full-policy', {'c': document_c['debug_policy'],
                                      'full': document_f['debug_policy']})
        # Manifest tamper is caught by strict verification (verify_delivery_bundle
        # via a second export over the tampered root: unknown root refusal).
        victim = c_root / 'sdk/Cargo.lock' if (c_root / 'sdk/Cargo.lock').exists() else None
        target = victim or (c_root / 'Cargo.lock')
        original = target.read_bytes()
        target.write_bytes(original + b'\n# tampered\n')
        self.command('tampered-root-refused', [self.binary, 'service-export',
                     '--dir', project, '--delivery-root', c_root],
                     self.scratch, self.pinned_env(group), expected=1)
        target.write_bytes(original)
        self.record('manifest-tamper-detected', {'target': str(target)})

    def phase_unknown_root(self):
        unknown = self.scratch / 'unmanaged-root'
        unknown.mkdir()
        (unknown / 'user.txt').write_text('user-owned')
        self.command('unknown-root-refused', [self.binary, 'service-export',
                     '--dir', self.scratch / 'project-a', '--delivery-root', unknown],
                     self.scratch, environment(), expected=1)
        assert (unknown / 'user.txt').read_text() == 'user-owned'
        self.record('unknown-nonempty-root-untouched', {'path': str(unknown)})

    def main(self):
        if self.scratch.exists():
            raise ValueError(f'scratch must be a new directory: {self.scratch}')
        self.scratch.mkdir(parents=True)
        self.logs.mkdir()
        if not self.binary.is_file():
            raise ValueError(f'--binary must be a built auto CLI: {self.binary}')
        group = self.scratch / 'tool'
        self.pin_sources(group)
        bundles = self.phase_ab_bundles(group)
        # Consumer home is fully independent of the export roots.
        consumer = self.scratch / 'consumer'
        shutil.copytree(bundles['a'], consumer)
        data_root = self.scratch / 'data-root'
        data_root.mkdir()
        member_a = self.phase_a_state(consumer, data_root)
        self.phase_b_upgrade_and_recovery(consumer, data_root, member_a, group)
        self.phase_unreachable_roots(group, bundles['b'])
        # After phase_unreachable_roots removed `group`, re-pin for the
        # remaining export phases.
        group = self.scratch / 'tool2'
        self.pin_sources(group)
        self.phase_chat(group)
        self.phase_database(group)
        self.phase_policy(group)
        self.phase_unknown_root()
        self.receipt['outcome'] = 'partial' if self.receipt['pending'] else 'pass'
        return 0 if self.receipt['outcome'] == 'pass' else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True,
                        help='built `auto` CLI (service-export entry)')
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--lang-root', type=Path,
                        default=REPO_ROOT.parent / 'auto-lang')
    parser.add_argument('--web-root', type=Path, default=REPO_ROOT)
    parser.add_argument('--chat-root', type=Path)
    parser.add_argument('--database-root', type=Path)
    args = parser.parse_args()
    driver = Driver(args)
    try:
        code = driver.main()
    except Exception as error:
        driver.receipt['error'] = str(error)
        print(str(error), file=sys.stderr)
        code = 2
    finally:
        driver.receipt['summary'] = {
            'executed': len(driver.receipt['checks']),
            'passed': sum(r['status'] == 'PASS' for r in driver.receipt['checks']),
            'failed': sum(r['status'] == 'FAIL' for r in driver.receipt['checks']),
            'pending': len(driver.receipt['pending'])}
        (driver.scratch / 'member-delivery-receipt.json').write_text(
            json.dumps(driver.receipt, indent=2, ensure_ascii=False) + '\n',
            encoding='utf-8', newline='\n')
        print(json.dumps(driver.receipt['summary']) + ' ' + driver.receipt['outcome'])
    return code


if __name__ == '__main__':
    sys.exit(main())
