#!/usr/bin/env python3
"""Real external Web generation, locked relocation and recovery scenarios."""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('web_install', ROOT / 'scripts/web-install.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


def environment():
    return {k: v for k, v in os.environ.items()
            if not k.startswith(('AUTO_', 'AC_', 'NEXTEST_', 'AV798_', 'MN818_'))
            and not k.startswith(('CARGO_PROFILE_', 'CARGO_PKG_'))
            and k not in {'CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'RUSTC_WRAPPER',
                         'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
                         'CARGO_BUILD_RUSTFLAGS', 'CARGO_MANIFEST_DIR', 'CARGO_MANIFEST_PATH', 'T3_MILESTONE'}}


def source_env(group):
    return {**environment(), 'AUTO_WEB_ROOT': str(group / 'tool/sdk'),
            'AUTO_LANG_ROOT': str(group / 'tool/sdk'),
            'AUTO_RUST_WORKSPACE': str(group / 'rust-workspace')}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def request(port, method, path):
    with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{port}{path}', method=method), timeout=5) as response:
        return response.status, response.read()


def runtime(binary, expected, log, *, vm_source=None):
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
    config = json.dumps({'profile': 'development', 'listen': {'port': port}, 'limits': {'drain_timeout_ms': 100}})
    env = {**environment(), 'AUTO_HTTP_SERVICE_JSON': config, 'AUTO_HTTP_PORT': str(port),
           'AUTO_EXECUTION_ENGINE': 'autovm'}
    args = [str(binary)]
    if vm_source:
        config_file = log.with_suffix('.service.json')
        config_file.write_text(config)
        args += ['run', str(vm_source), '--config', str(config_file)]
    with log.open('wb') as output:
        child = subprocess.Popen(args, env=env, stdout=output, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 20
            while True:
                if child.poll() is not None:
                    raise RuntimeError('runtime exited before readiness: ' + str(log))
                try:
                    status, value = request(port, 'GET', '/value')
                    break
                except (OSError, TimeoutError):
                    if time.monotonic() > deadline:
                        raise RuntimeError('runtime readiness timed out: ' + str(log))
                    time.sleep(.05)
            if status != 200 or value != json.dumps(expected).encode():
                raise ValueError(f'exact response differs: {status} {value!r}, wanted {expected!r}')
            if request(port, 'POST', '/__auto/shutdown')[0] != 202:
                raise ValueError('shutdown status differs')
            if child.wait(timeout=10) != 0:
                raise ValueError('runtime shutdown exit differs')
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=10)
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', port))
    return {'exact_json': expected, 'shutdown': 202, 'exit': 0, 'port_released': True,
            'binary_sha256': sha(binary), 'log_sha256': sha(log)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    parser.add_argument('--allow-candidate', action='store_true')
    parser.add_argument('--legacy-receipt', type=Path)
    parser.add_argument('--independent-test-receipt', type=Path)
    parser.add_argument('--skip-independent-tests', action='store_true', help='diagnostic partial run only; never certifies distribution')
    args = parser.parse_args()
    source, base = args.source.resolve(), args.scratch.resolve()
    if base.exists() or base == source or source in base.parents or base in source.parents:
        raise ValueError('scratch must be a new independent directory')
    bundle = installer.verify_source(source)
    if bundle['candidate'] and not args.allow_candidate:
        raise ValueError('fixed distribution requires committed source cohort')
    base.mkdir(parents=True)
    logs = base / 'logs'
    logs.mkdir()
    receipt = {'schema': 'web-repository-distribution/1', 'outcome': 'fail',
               'sources': bundle['sources'], 'source_sha256': bundle['source_sha256'],
               'candidate': bundle['candidate'], 'checks': [], 'pending': []}

    def command(name, argv, cwd, env=None, expected=0):
        started = time.monotonic()
        log = logs / (name + '.log')
        with log.open('wb') as output:
            result = subprocess.run([str(v) for v in argv], cwd=cwd, env=env or environment(),
                                    stdout=output, stderr=subprocess.STDOUT, timeout=3600)
        row = {'id': name, 'command': [str(v) for v in argv], 'exit': result.returncode,
               'expected_exit': expected, 'seconds': time.monotonic() - started,
               'log': str(log), 'log_sha256': sha(log),
               'status': 'PASS' if result.returncode == expected else 'FAIL'}
        receipt['checks'].append(row)
        if row['status'] == 'FAIL':
            raise ValueError('command failed: ' + name + ', see ' + str(log))

    def record(name, detail):
        receipt['checks'].append({'id': name, 'status': 'PASS', 'detail': detail})

    def generated_manifest(group):
        return group / 'rust-workspace/external-web-back/Cargo.toml'

    def server(group):
        return group / 'generated-target/debug' / installer.OWNER.exe('external-web-back')

    def generate_and_build(group, prefix, *, fresh=False):
        generator = group / installer.OWNER.exe('web-standalone-consumer')
        command(prefix + '-public-generation', [generator, group / 'external-web'], group, source_env(group))
        manifest = generated_manifest(group)
        text = (group / 'rust-workspace/Cargo.toml').read_text()
        for original in (str(ROOT), str(ROOT.parent / 'auto-lang'), str(ROOT.parent / 'auto-ui'), str(ROOT.parent / 'auto-man')):
            if original.replace('\\', '/') in text.replace('\\', '/'):
                raise ValueError('generated Cargo workspace borrowed original checkout')
        argv = ['cargo', 'build', '--manifest-path', manifest, '--target-dir', group / 'generated-target',
                '--config', 'profile.dev.debug="line-tables-only"', '-j', '4']
        if not fresh: argv.append('--locked')
        command(prefix + '-generated-build', argv, group, source_env(group))
        command(prefix + '-generated-locked-replay', [*argv, *([] if '--locked' in argv else ['--locked'])], group, source_env(group))
        record(prefix + '-generated-runtime', runtime(server(group), 890, logs / (prefix + '-generated-runtime.log')))

    try:
        sdk_inputs = installer.OWNER.manifest_files(source / 'sdk')
        # Earlier execution can be reused only for identical actual SDK inputs
        # and the exact nextest configuration, not merely the same HEAD.
        test_identity = installer.fingerprint(sdk_inputs)
        config_hash = sha(source / 'sdk/.config/nextest.toml')
        if args.skip_independent_tests:
            receipt['pending'].append('independent-sdk-tests: explicitly not executed in diagnostic partial run')
        elif args.independent_test_receipt:
            tested = json.loads(args.independent_test_receipt.read_text())
            if (tested.get('schema') != 'web-independent-tests/1' or tested.get('outcome') != 'pass'
                    or tested.get('sdk_inputs_sha256') != test_identity or tested.get('config_sha256') != config_hash
                    or tested.get('passed') != 217 or tested.get('original_skipped') != 2
                    or sha(Path(tested['log'])) != tested['log_sha256']):
                raise ValueError('independent test input/config/output receipt differs')
            record('independent-sdk-tests', {'reused_receipt': str(args.independent_test_receipt),
                                            'sha256': sha(args.independent_test_receipt)})
        else:
            command('independent-sdk-tests', ['cargo', 'nextest', 'run', '--manifest-path', source / 'sdk/Cargo.toml',
                    '-p', 'auto-web-contract', '-p', 'auto-web', '--lib', '--features', 'auto-web/test-http-e2e',
                    '--config-file', source / 'sdk/.config/nextest.toml', '--target-dir', source / 'sdk/target',
                    '-j', '1', '--no-fail-fast'], base)
            output = (logs / 'independent-sdk-tests.log').read_text(encoding='utf-8', errors='replace')
            import re
            if not re.search(r'Summary\s+\[[^\]]+\]\s+217 tests run: 217 passed, 2 skipped', output):
                raise ValueError('independent tests did not execute all original applicable cases')
            installer.write_json(base / 'independent-tests.json', {'schema': 'web-independent-tests/1',
                'outcome': 'pass', 'sdk_inputs_sha256': test_identity, 'config_sha256': config_hash,
                'passed': 217, 'original_skipped': 2, 'log': str(logs / 'independent-sdk-tests.log'),
                'log_sha256': sha(logs / 'independent-sdk-tests.log')})
        a = base / 'A'
        a.mkdir()
        installer_path = source / 'scripts/web-install.py'
        command('fresh-install', [sys.executable, installer_path, 'install', '--source', source,
                                  '--install-root', a / 'tool', '--target-dir', base / 'tool-build'], base)
        installed = installer.verify_install(a / 'tool')
        metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1',
                            '--manifest-path', str(a / 'tool/sdk/Cargo.toml')], env=environment()))
        local = [p for p in metadata['packages'] if p['source'] is None]
        if {p['name'] for p in local} != {p['name'] for p in bundle['source_closure']['packages']}:
            raise ValueError('standalone local package closure differs')
        if any(not Path(p['manifest_path']).resolve().is_relative_to(a / 'tool/sdk') for p in local):
            raise ValueError('standalone package borrowed an external source')
        record('independent-installed-closure', {'local_packages': len(local), **installed})
        installer.OWNER.clean_tree(source / 'consumer', a / 'consumer')
        manifest = a / 'consumer/Cargo.toml'
        manifest.write_text(manifest.read_text().replace('../sdk/crates/auto-web', '../tool/sdk/crates/auto-web'))
        command('external-fresh-lock', ['cargo', 'generate-lockfile', '--manifest-path', manifest], a)
        command('external-locked-build', ['cargo', 'build', '--locked', '--manifest-path', manifest,
                                         '--target-dir', base / 'consumer-build', '-j', '4'], a)
        shutil.copy2(base / 'consumer-build/debug' / installer.OWNER.exe('web-standalone-consumer'), a)
        project = a / 'external-web'
        (project / 'src/back').mkdir(parents=True)
        (project / 'pac.at').write_text('{name:"external-web",scene:"workspace"}')
        (project / 'src/back/api.at').write_text('#[api(method="GET",path="/value")]\npub fn value() int { 890 }\n')
        sentinel = project / 'user.txt'
        sentinel.write_text('user-owned content must survive')
        user_hash = sha(sentinel)
        record('fresh-vm-runtime', runtime(a / 'tool' / installer.OWNER.exe('auto-web'), 890,
                                         logs / 'fresh-vm-runtime.log', vm_source=project / 'src/back/api.at'))
        generate_and_build(a, 'fresh', fresh=True)
        # The real workspace phase can be interrupted without changing the
        # public generator. Restore its explicitly captured prior workspace,
        # then consume again; no process-wide atomic-write claim is made.
        backup = base / 'workspace-before-interruption'
        installer.OWNER.clean_tree(a / 'rust-workspace', backup)
        with (logs / 'interrupted-generation.log').open('w') as log:
            child = subprocess.Popen([str(a / installer.OWNER.exe('web-standalone-consumer')),
                                      '--pause-after-workspace', str(project)], cwd=a,
                                     env=source_env(a), stdout=subprocess.PIPE, stderr=log, text=True)
            try:
                import threading
                lines = []
                reader = threading.Thread(target=lambda: lines.append(child.stdout.readline()), daemon=True)
                reader.start()
                reader.join(timeout=30)
                if reader.is_alive() or lines != ['WEB_CONSUMER_WORKSPACE_CHECKPOINT\n']:
                    raise ValueError('generation checkpoint not actually reached')
                child.kill()
                child.wait(timeout=10)
                if child.returncode == 0: raise ValueError('interrupted generator exited successfully')
            finally:
                if child.poll() is None:
                    child.kill(); child.wait(timeout=10)
        damaged = a / 'interrupted-workspace'
        os.replace(a / 'rust-workspace', damaged)
        installer.OWNER.clean_tree(backup, a / 'rust-workspace')
        generate_and_build(a, 'after-interruption')
        record('interrupted-generation-recovered', {'checkpoint': 'public Web workspace phase', 'user_sha256': sha(sentinel)})
        # All paths are fixed children of this newly admitted scratch. Move
        # only the owned group; old build caches remain outside it and are not used.
        b = base / 'B'
        if a.parent != base or b.parent != base or b.exists(): raise ValueError('unsafe relocation target')
        # Remove the generated target from the relocated source cohort by
        # preserving it separately. B must compile into a fresh empty target.
        os.replace(a / 'generated-target', base / 'A-preserved-build-cache')
        os.replace(a, b)
        if a.exists() or (b / 'generated-target').exists(): raise ValueError('relocation reused old target')
        generate_and_build(b, 'relocated')
        record('relocated-vm-runtime', runtime(b / 'tool' / installer.OWNER.exe('auto-web'), 890,
                                             logs / 'relocated-vm-runtime.log', vm_source=b / 'external-web/src/back/api.at'))
        before = installer.OWNER.manifest_files(b / 'tool')
        bad_version = base / 'wrong-producer-version'
        installer.OWNER.clean_tree(source, bad_version)
        package_workspace = bad_version / 'sdk/Cargo.toml'
        text = package_workspace.read_text()
        changed = text.replace('[workspace.package]\nversion = "0.1.0"', '[workspace.package]\nversion = "99.0.0"', 1)
        if changed == text: raise ValueError('producer version mutation did not change its intended input')
        package_workspace.write_text(changed)
        old_workspace = installer.OWNER.manifest_files(b / 'rust-workspace')
        command('wrong-producer-version-public-rejection', [b / installer.OWNER.exe('web-standalone-consumer'), b / 'external-web'],
                b, {**source_env(b), 'AUTO_WEB_ROOT': str(bad_version / 'sdk')}, expected=1)
        if ('Web producer version mismatch' not in (logs / 'wrong-producer-version-public-rejection.log').read_text()
                or installer.OWNER.manifest_files(b / 'rust-workspace') != old_workspace):
            raise ValueError('wrong producer version was not rejected before existing workspace writes')
        record('wrong-producer-version-old-runtime', runtime(server(b), 890, logs / 'wrong-producer-version-old-runtime.log'))
        for mutation in ('missing-producer-version', 'managed-source-hash', 'missing-managed-receipt'):
            bad = base / mutation
            if mutation == 'missing-producer-version':
                bad.mkdir()
                installer.OWNER.clean_tree(b / 'tool/sdk', bad / 'sdk')
                path = bad / 'sdk/crates/auto-web/Cargo.toml'
                text = path.read_text()
                changed = text.replace('version.workspace = true\n', '', 1)
                if text == changed: raise ValueError('missing version mutation did not change its intended input')
                path.write_text(changed)
            else:
                installer.OWNER.clean_tree(b / 'tool', bad)
                if mutation == 'managed-source-hash':
                    (bad / 'sdk/crates/auto-web/src/lib.rs').write_text('// changed selected producer')
                else:
                    (bad / 'install-manifest.json').unlink()
            command(mutation + '-public-rejection', [b / installer.OWNER.exe('web-standalone-consumer'), b / 'external-web'],
                    b, {**source_env(b), 'AUTO_WEB_ROOT': str(bad / 'sdk')}, expected=1)
            if installer.OWNER.manifest_files(b / 'rust-workspace') != old_workspace:
                raise ValueError('rejected producer changed old generated workspace: ' + mutation)
            record(mutation + '-old-runtime', runtime(server(b), 890, logs / (mutation + '-old-runtime.log')))
        for mutation in ('missing-resource', 'bad-hash', 'bad-version'):
            bad = base / mutation
            installer.OWNER.clean_tree(source, bad)
            if mutation == 'missing-resource': (bad / 'sdk/stdlib/auto/image.rs.at').unlink()
            elif mutation == 'bad-hash': (bad / 'sdk/crates/auto-web/src/lib.rs').write_text('corrupt selected producer')
            else:
                doc = json.loads((bad / installer.SOURCE_MANIFEST).read_text())
                doc['version'] = '99.0.0'
                installer.write_json(bad / installer.SOURCE_MANIFEST, doc)
            command(mutation + '-rejected', [sys.executable, installer_path, 'install', '--source', bad,
                     '--install-root', b / 'tool', '--target-dir', base / 'tool-build'], base, expected=2)
            if installer.OWNER.manifest_files(b / 'tool') != before or sha(b / 'external-web/user.txt') != user_hash:
                raise ValueError('rejected source changed old install or user files')
            record(mutation + '-old-runtime', runtime(server(b), 890, logs / (mutation + '-old-runtime.log')))
        command('failed-swap', [sys.executable, installer_path, 'install', '--source', source,
                 '--install-root', b / 'tool', '--target-dir', base / 'tool-build', '--fail-point', 'swap'], base, expected=2)
        if installer.OWNER.manifest_files(b / 'tool') != before:
            raise ValueError('failed swap did not restore exact prior install')
        generate_and_build(b, 'after-failed-swap')
        command('successful-upgrade', [sys.executable, installer_path, 'install', '--source', source,
                 '--install-root', b / 'tool', '--target-dir', base / 'tool-build'], base)
        generate_and_build(b, 'after-upgrade')
        if sha(b / 'external-web/user.txt') != user_hash: raise ValueError('user content changed')
        if args.legacy_receipt:
            legacy = json.loads(args.legacy_receipt.read_text())
            if legacy.get('schema') != 'web-legacy-upgrade/1' or legacy.get('outcome') != 'pass':
                raise ValueError('legal old three-owner upgrade/recovery receipt missing or failed')
            record('legacy-three-owner-recovery', {'receipt': str(args.legacy_receipt), 'sha256': sha(args.legacy_receipt)})
        else:
            receipt['pending'].append('legacy-three-owner-recovery: real old combination receipt required')
        if bundle['candidate']: receipt['pending'].append('fixed committed source cohort required')
        receipt['outcome'] = 'partial' if receipt['pending'] else 'pass'
    except Exception as error:
        receipt['error'] = str(error)
        print(str(error), file=sys.stderr)
    finally:
        receipt['summary'] = {'executed': len(receipt['checks']),
                              'passed': sum(r['status'] == 'PASS' for r in receipt['checks']),
                              'failed': sum(r['status'] == 'FAIL' for r in receipt['checks']),
                              'pending': len(receipt['pending'])}
        installer.write_json(base / 'distribution-receipt.json', receipt)
        print(json.dumps(receipt['summary']) + ' ' + receipt['outcome'])
    return 0 if receipt['outcome'] == 'pass' else 2


if __name__ == '__main__':
    sys.exit(main())
