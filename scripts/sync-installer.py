#!/usr/bin/env python3
"""Regenerate the immutable Man-owned installer artifact from committed source."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--man-root', type=Path, default=ROOT.parent / 'auto-man')
    args = parser.parse_args()
    owner = args.man_root.resolve()
    source = owner / 'scripts/man-install.py'
    revision = subprocess.check_output(['git', '-C', str(owner), 'rev-parse', 'HEAD'], text=True).strip()
    subprocess.run(['git', '-C', str(owner), 'diff', '--quiet', 'HEAD', '--', 'scripts/man-install.py', 'LICENSE'], check=True)
    destination = ROOT / 'scripts/vendor'
    destination.mkdir(parents=True, exist_ok=True)
    artifacts = {}
    for name, relative in [('man-install.py', 'scripts/man-install.py'), ('MAN-LICENSE', 'LICENSE')]:
        raw = subprocess.check_output(['git', '-C', str(owner), 'show', revision + ':' + relative])
        (destination / name).write_bytes(raw)
        artifacts[name] = raw
    provenance = (f'source_repo = "auto-man"\nsource_commit = "{revision}"\n'
                  f'source_path = "scripts/man-install.py"\n'
                  f'sha256 = "{hashlib.sha256(artifacts["man-install.py"]).hexdigest()}"\n'
                  f'license_sha256 = "{hashlib.sha256(artifacts["MAN-LICENSE"]).hexdigest()}"\n')
    (destination / 'man-install.provenance.toml').write_text(provenance, encoding='utf-8', newline='\n')
    print(provenance)


if __name__ == '__main__':
    main()
