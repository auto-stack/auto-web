#!/usr/bin/env python3
"""Build a standalone Web source payload and install it through Man's transaction."""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
SOURCE_MANIFEST = 'source-manifest.json'


def artifact_producer(vendor):
    provenance = tomllib.loads((vendor / 'man-install.provenance.toml').read_text())
    for name, key in [('man-install.py', 'sha256'), ('MAN-LICENSE', 'license_sha256')]:
        if hashlib.sha256((vendor / name).read_bytes()).hexdigest() != provenance[key]:
            raise ValueError('immutable Man installer artifact differs; regenerate from committed owner')
    if provenance['source_repo'] != 'auto-man' or not re.fullmatch('[0-9a-f]{40}', provenance['source_commit']):
        raise ValueError('invalid installer source owner')
    return provenance


def transaction_owner():
    vendor = Path(__file__).resolve().parent / 'vendor'
    provenance = artifact_producer(vendor)
    spec = importlib.util.spec_from_file_location('web_man_transaction', vendor / 'man-install.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module, provenance


OWNER, TOOL_PRODUCER = transaction_owner()


def file_map(root):
    files = OWNER.manifest_files(root)
    files.pop(SOURCE_MANIFEST, None)
    return files


def write_json(path, document):
    path.write_text(json.dumps(document, indent=2, ensure_ascii=False) + '\n', encoding='utf-8', newline='\n')


def fingerprint(files):
    return hashlib.sha256(json.dumps(files, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def validate_packages(packages):
    names = [p['name'] for p in packages]
    if len(names) != len(set(names)) or not {'auto-web', 'auto-web-contract', 'auto-lang-core', 'auto-generation-core', 'a2r-std', 'auto-media-core'}.issubset(names):
        raise ValueError('missing/duplicate Web source package')
    for package in packages:
        name = package['name']
        # PLAN-906: auto-web-contract 单源迁语言仓——receipt owner 随源树翻转。
        owner = 'auto-web' if name == 'auto-web' else 'auto-lang'
        if name in ('ace-semantics-contract', 'autodown-test-parser'):
            # Non-crates whitelist entries (fix-989-sdk-staging): same-shape
            # mirrored standalone workspaces from the language producer.
            allowed = ('experimental/ace/crates/semantics-contract'
                       if name == 'ace-semantics-contract'
                       else 'vendor/autodown-test-parser')
            if (package['source_repo'] != 'auto-lang'
                    or package['source_path'] != allowed
                    or package['installed_path'] != package['source_path']):
                raise ValueError('invalid standalone package owner/path: ' + name)
            continue
        if (package['source_repo'] != owner or package['source_path'] != 'crates/' + name
                or package['installed_path'] != 'crates/' + name
                or name.startswith(('auto-ui', 'auto-man')) or name in ('tauri', 'auto-lang', 'auto-api-gen')):
            raise ValueError('invalid standalone package owner/path: ' + name)


def verify_source(root):
    document = json.loads((root / SOURCE_MANIFEST).read_text(encoding='utf-8'))
    if document.get('schema') != 'web-source/1' or document.get('version') != '0.1.0':
        raise ValueError('unsupported Web source version')
    if file_map(root) != document['files'] or fingerprint(document['files']) != document['source_sha256']:
        raise ValueError('Web source content/hash differs')
    if document['tool_producer'] != artifact_producer(root / 'scripts/vendor'):
        raise ValueError('Web installer source identity differs')
    for name in ('auto-lang', 'auto-web'):
        if not re.fullmatch('[0-9a-f]{40}', document['sources'].get(name, '')):
            raise ValueError('missing fixed source revision: ' + name)
        if not (root / 'licenses' / name / 'LICENSE').is_file():
            raise ValueError('missing source license: ' + name)
    packages = document['source_closure']['packages']
    validate_packages(packages)
    workspace = tomllib.loads((root / 'sdk/Cargo.toml').read_text())
    if workspace['workspace']['package']['version'] != '0.1.0':
        raise ValueError('Web producer version mismatch')
    return document


def stage_source(output):
    if output.exists() and any(output.iterdir()):
        raise ValueError('source output must be fresh or empty; previous payload preserved')
    lang = Path(os.environ.get('AUTO_LANG_ROOT', ROOT.parent / 'auto-lang')).resolve()
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT))
    local = [p for p in metadata['packages'] if p['source'] is None]
    names = {p['name'] for p in local}
    if any(n.startswith(('auto-ui', 'auto-man')) or n in ('auto-lang', 'auto-api-gen', 'tauri') for n in names):
        raise ValueError('forbidden standalone Web closure')
    output.mkdir(parents=True, exist_ok=True)
    sdk = output / 'sdk'
    sdk.mkdir()
    packages = []
    for package in local:
        source = Path(package['manifest_path']).parent.resolve()
        owner = 'auto-web' if source.is_relative_to(ROOT) else 'auto-lang' if source.is_relative_to(lang) else None
        if owner is None:
            raise ValueError('foreign local source: ' + str(source))
        relative = source.relative_to(ROOT if owner == 'auto-web' else lang)
        if relative.parts != ('crates', package['name']):
            # PLAN-944 (2026-11-06) added ace-semantics-contract as a
            # non-optional dependency of auto-lang-core at a non-crates
            # layout; mirror it same-shape into the sdk (standalone
            # workspace — not a projected member). fix-989-sdk-staging.
            if relative.as_posix() in ('experimental/ace/crates/semantics-contract',
                                        'vendor/autodown-test-parser'):
                # Non-crates standalone workspaces: PLAN-944 semantics-contract
                # (non-optional dep of auto-lang-core) and the PLAN-901 vendored
                # parser (via auto-test-case). Not projected members; the
                # parser is already staged wholesale below, semantics-contract
                # mirrors here same-shape. fix-989-sdk-staging.
                if relative.as_posix() == 'experimental/ace/crates/semantics-contract':
                    destination = sdk / relative
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    OWNER.clean_tree(source, destination)
                packages.append({'name': package['name'], 'source_repo': owner,
                                 'source_path': relative.as_posix(), 'installed_path': relative.as_posix()})
                continue
            raise ValueError('unexpected local package layout: ' + str(source))
        destination = sdk / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        OWNER.clean_tree(source, destination)
        OWNER.rewrite_manifest(destination / 'Cargo.toml')
        OWNER.project_missing_optional(destination / 'Cargo.toml', names)
        packages.append({'name': package['name'], 'source_repo': owner,
                         'source_path': relative.as_posix(), 'installed_path': relative.as_posix()})
    workspace = (lang / 'Cargo.toml').read_text()
    # Non-crates standalone workspaces (semantics-contract, vendored parser)
    # are path dependencies, never projected members — exclude them like the
    # language root does. fix-989-sdk-staging.
    standalone = sorted(p['installed_path'] for p in packages
                        if p['installed_path'].startswith(('experimental/', 'vendor/')))
    standalone_set = set(standalone)
    members = ', '.join(json.dumps(p['installed_path']) for p in sorted(packages, key=lambda p: p['name'])
                        if p['installed_path'] not in standalone_set)
    workspace, count = re.subn(r'(?ms)^members = \[.*?^\]', 'members = [' + members + ']', workspace, count=1)
    if count != 1:
        raise ValueError('language workspace member projection failed')
    if standalone:
        # Inject into the existing [workspace] table (right after members):
        # a second [workspace] table would be invalid TOML.
        workspace, count = re.subn(r'(?m)^(members = \[.*\])$',
                                   r'\1\nexclude = [' + ', '.join(json.dumps(s) for s in standalone) + ']',
                                   workspace, count=1)
        if count != 1:
            raise ValueError('standalone exclude injection failed')
    workspace = workspace.split('[patch.crates-io]', 1)[0].rstrip()
    workspace += '\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2021"\nlicense = "MIT"\n'
    (sdk / 'Cargo.toml').write_text(workspace, encoding='utf-8', newline='\n')
    OWNER.clean_tree(ROOT / '.config', sdk / '.config')
    for relative in OWNER.ROOT_INPUT_DIRS:
        OWNER.clean_tree(lang / relative, sdk / relative)
    for relative in OWNER.ROOT_INPUTS:
        destination = sdk / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(lang / relative, destination)
    OWNER.clean_tree(lang / 'vendor/autodown-test-parser', sdk / 'vendor/autodown-test-parser')
    # UI example fixtures follow the auto-ui PLAN-007 relocation (2026-11-02):
    # the managed examples live in the sibling auto-ui checkout, byte-identical
    # to their pre-relocation auto-lang content (per-file SHA manifest).
    ui_repo = lang.parent / 'auto-ui'
    for example in ('015-notes', '017-chat', '023-realworld'):
        base = Path('examples/ui') / example
        OWNER.clean_tree(ui_repo / base / 'src/back', sdk / base / 'src/back')
        shutil.copy2(ui_repo / base / 'pac.at', sdk / base / 'pac.at')
    for name, source in [('auto-lang', lang), ('auto-web', ROOT)]:
        destination = output / 'licenses' / name
        destination.mkdir(parents=True)
        shutil.copy2(source / 'LICENSE', destination / 'LICENSE')
    OWNER.clean_tree(ROOT / 'scripts', output / 'scripts')
    OWNER.clean_tree(ROOT / 'test/fixtures/standalone-consumer', output / 'consumer')
    consumer_manifest = output / 'consumer/Cargo.toml'
    consumer_manifest.write_text(consumer_manifest.read_text().replace('../../../crates/auto-web', '../sdk/crates/auto-web'),
                                 encoding='utf-8', newline='\n')
    shutil.copy2(ROOT / 'README.md', output / 'README.md')
    shutil.copy2(ROOT / 'Cargo.lock', sdk / 'Cargo.lock')
    OWNER.run(['cargo', 'generate-lockfile'], sdk)
    sources = {name: OWNER.head(path) for name, path in [('auto-lang', lang), ('auto-web', ROOT)]}
    candidate = any(subprocess.check_output(['git', '-C', str(path), 'status', '--porcelain', '--',
                    'crates', 'scripts', 'stdlib', 'test', 'vendor', '.config', '.cargo', 'Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md'])
                    for path in (lang, ROOT))
    files = file_map(output)
    document = {'schema': 'web-source/1', 'version': '0.1.0', 'sources': sources,
                'candidate': candidate, 'tool_producer': TOOL_PRODUCER,
                'source_closure': {'sdk_layout': 'combined-crates', 'packages': packages,
                                   'root_inputs': OWNER.ROOT_INPUTS + OWNER.ROOT_INPUT_DIRS},
                'third_party': [{'name': p['name'], 'version': p['version'], 'source': p['source'], 'license': p['license']}
                                for p in metadata['packages'] if p['source'] is not None],
                'files': files, 'source_sha256': fingerprint(files)}
    write_json(output / SOURCE_MANIFEST, document)
    verify_source(output)
    return {'source_root': str(output), 'packages': len(packages), 'candidate': candidate,
            'source_sha256': document['source_sha256']}


def verify_install(root):
    document = json.loads((root / OWNER.MANIFEST).read_text(encoding='utf-8'))
    if document.get('schema') != 'web-install/1' or document.get('profile') != 'web' or document.get('version') != '0.1.0':
        raise ValueError('unsupported Web install schema/profile/version')
    actual = OWNER.manifest_files(root)
    if document['files'] != actual:
        raise ValueError('Web install content/hash differs')
    producer = document.get('tool_producer', {})
    if (set(document['sources']) != {'auto-lang', 'auto-web'} or producer.get('source_repo') != 'auto-man'
            or producer.get('source_path') != 'scripts/man-install.py'
            or not re.fullmatch('[0-9a-f]{40}', producer.get('source_commit', ''))
            or any(not re.fullmatch('[0-9a-f]{64}', producer.get(k, '')) for k in ('sha256', 'license_sha256'))):
        raise ValueError('Web install source/tool owners differ')
    validate_packages(document['source_closure']['packages'])
    for name in document['sources']:
        if not re.fullmatch('[0-9a-f]{40}', document['sources'][name]):
            raise ValueError('invalid fixed Web source revision')
    required = {OWNER.exe('auto-web'), 'sdk/crates/auto-web/Cargo.toml',
                'sdk/crates/auto-web-contract/Cargo.toml', 'sdk/Cargo.lock',
                'licenses/auto-web/LICENSE', 'licenses/auto-lang/LICENSE'}
    if not required.issubset(actual):
        raise ValueError('required Web tool/source/resource/license missing')
    if any(p.startswith(('sdk/crates/auto-ui', 'sdk/crates/auto-man', 'sdk/crates/auto-lang/')) for p in actual):
        raise ValueError('forbidden UI/Man/facade source in Web install')
    producer = document['runtime_producer']
    if producer['binary_sha256'] != actual[OWNER.exe('auto-web')]['sha256'] or producer['sources'] != document['sources']:
        raise ValueError('runtime producer identity differs')
    return {'version': document['version'], 'files': len(actual), 'sources': document['sources']}


def install(source, root, target, failure):
    def prepare(stage):
        bundle = verify_source(source)
        OWNER.run(['cargo', 'build', '--locked', '--manifest-path', str(source / 'sdk/Cargo.toml'),
                   '-p', 'auto-web', '--bin', 'auto-web', '--target-dir', str(target), '-j', '4'], source)
        verify_source(source)
        OWNER.clean_tree(source / 'sdk', stage / 'sdk')
        OWNER.clean_tree(source / 'licenses', stage / 'licenses')
        binary = target / 'debug' / OWNER.exe('auto-web')
        shutil.copy2(binary, stage / OWNER.exe('auto-web'))
        files = OWNER.manifest_files(stage)
        document = {'schema': 'web-install/1', 'profile': 'web', 'version': '0.1.0',
                    'sources': bundle['sources'], 'source_closure': bundle['source_closure'],
                    'tool_producer': TOOL_PRODUCER, 'files': files,
                    'runtime_producer': {'sources': bundle['sources'], 'source_sha256': bundle['source_sha256'],
                                         'lock_sha256': OWNER.sha256(source / 'sdk/Cargo.lock'),
                                         'binary_sha256': OWNER.sha256(binary)}}
        write_json(stage / OWNER.MANIFEST, document)
    return OWNER.install_payload(root, prepare, verify_install, failure)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    source = sub.add_parser('source')
    source.add_argument('--output', type=Path, required=True)
    verify = sub.add_parser('verify-source')
    verify.add_argument('--source', type=Path, required=True)
    installer = sub.add_parser('install')
    installer.add_argument('--source', type=Path, required=True)
    installer.add_argument('--install-root', type=Path, required=True)
    installer.add_argument('--target-dir', type=Path, required=True)
    installer.add_argument('--fail-point', choices=['prepare', 'verify', 'swap'])
    installed = sub.add_parser('verify')
    installed.add_argument('--install-root', type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == 'source': result = stage_source(args.output.resolve())
        elif args.command == 'verify-source': result = verify_source(args.source.resolve())
        elif args.command == 'verify': result = verify_install(args.install_root.resolve())
        else: result = install(args.source.resolve(), args.install_root.resolve(), args.target_dir.resolve(), args.fail_point)
        print(json.dumps(result, ensure_ascii=False))
        return 0
    except Exception as error:
        print('web-install: ' + str(error), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
