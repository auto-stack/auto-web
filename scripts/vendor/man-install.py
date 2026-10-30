#!/usr/bin/env python3
"""Build, install, and verify PLAN-832 Man Core/full profiles.

Layout: auto(.exe), optional auto-ui-tool(.exe), sdk/, install-manifest.json.
Preparation happens in a unique marked staging directory. Existing managed
installations are moved to a unique preserved backup before swap; unknown
non-empty directories are never claimed or deleted.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import tomllib

MAN = Path(__file__).resolve().parent.parent
GROUP = MAN.parent
LANG = GROUP / "auto-lang"
UI = GROUP / "auto-ui"
DOWN = GROUP / "auto-down"
WEB = GROUP / "auto-web"
MANIFEST = "install-manifest.json"
ROOT_INPUTS = [
    "stdlib/assembly-providers.json",
    "stdlib/auto/image.at",
    "stdlib/auto/image.rs.at",
    "stdlib/auto/image.vm.at",
    "docs/schema/aura.at",
]
ROOT_INPUT_DIRS = ['stdlib/auto']
SCRUB = {
    "AUTO_LANG_ROOT", "AUTO_MAN_ROOT", "AUTO_UI_ROOT", "AUTO_DOWN_ROOT",
    "AUTO_UI_TOOL_BIN", "AUTO_RUST_WORKSPACE", "AUTO_LANG_CRATE",
    "AUTO_DELIVERY_SOURCE_ROOT", "AUTO_UI_SOURCE_ROOT", "CARGO_TARGET_DIR",
    "RUSTC_WRAPPER", "AUTO_WEB_ROOT",
}


def exe(name: str) -> str:
    return name + (".exe" if os.name == "nt" else "")


def run(cmd: list[str], cwd: Path, timeout: int = 7200) -> None:
    env = {k: v for k, v in os.environ.items() if k not in SCRUB}
    proc = subprocess.run(cmd, cwd=str(cwd), env=env, timeout=timeout)
    if proc.returncode:
        raise RuntimeError(f"command failed ({proc.returncode}): {' '.join(cmd)}")


def head(root: Path) -> str:
    return subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def clean_tree(src: Path, dst: Path) -> None:
    def ignore(_dir, names):
        skipped = {name for name in names if name in {".git", "target", "__pycache__", "node_modules"}}
        for name in set(names) - skipped:
            path = Path(_dir) / name
            if path.is_symlink() or path.is_junction():
                raise RuntimeError(f'linked source input is forbidden: {path}')
        return skipped
    shutil.copytree(src, dst, ignore=ignore)


def metadata_package_dirs(profile: str) -> set[Path]:
    manifest = MAN / "core/Cargo.toml" if profile == "core" else MAN / "Cargo.toml"
    cmd = ["cargo", "metadata", "--locked", "--manifest-path", str(manifest), "--format-version", "1"]
    if profile == "core":
        cmd += ["--no-default-features", "--features", "core"]
    metadata = json.loads(subprocess.check_output(cmd, cwd=str(MAN), text=True))
    root_id = next(p["id"] for p in metadata["packages"] if p["name"] == "auto")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    packages = {p["id"]: p for p in metadata["packages"]}
    seen = set()

    def visit(package_id: str) -> None:
        if package_id in seen:
            return
        seen.add(package_id)
        for dependency in nodes[package_id]["deps"]:
            visit(dependency["pkg"])

    visit(root_id)
    if profile == "full":
        for name in ("auto", "auto-lang", "auto-man"):
            visit(next(p["id"] for p in packages.values() if p["name"] == name))
    result = set()
    for package_id in seen:
        package = packages[package_id]
        if package.get("source") is None:
            result.add(Path(package["manifest_path"]).parent.resolve())
    return result


def repo_for(path: Path) -> tuple[str, Path]:
    for name, root in (("auto-man", MAN), ("auto-lang", LANG), ("auto-ui", UI), ("auto-down", DOWN), ("auto-web", WEB)):
        try:
            path.relative_to(root)
            return name, root
        except ValueError:
            pass
    raise RuntimeError(f"unregistered path source: {path}")


def rewrite_manifest(path: Path) -> None:
    text = path.read_text(encoding="utf-8")
    text = text.replace("../../../auto-lang/crates/", "../")
    text = text.replace("../../../auto-ui/crates/", "../")
    text = text.replace("../../../auto-web/crates/", "../")
    path.write_text(text, encoding="utf-8", newline="\n")


def project_missing_optional(manifest: Path, installed: set[str]) -> list[str]:
    import re
    doc = tomllib.load(manifest.open("rb"))
    missing = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        for name, spec in doc.get(section, {}).items():
            if isinstance(spec, dict) and spec.get("optional") and "path" in spec and name not in installed:
                missing.add(name)
    if not missing:
        return []
    text = manifest.read_text(encoding="utf-8")
    edits = []
    for name in sorted(missing):
        pattern = re.compile(rf"^\s*{re.escape(name)}\s*=.*(?:\n|$)", re.MULTILINE)
        text, count = pattern.subn("", text)
        edits.append(f"{manifest.parent.name}:{name}:dep={count}")
        text, count = re.subn(rf'^\s*"dep:{re.escape(name)}"\s*,?\s*(?:\n|$)', "", text, flags=re.MULTILINE)
        edits.append(f"{manifest.parent.name}:{name}:feature_lines={count}")
        for prefix in (f'"dep:{name}", ', f', "dep:{name}"', f'"dep:{name}"'):
            text = text.replace(prefix, "")
        text, count = re.subn(rf',?\s*"{re.escape(name)}/[^"]*",?\s*', "", text)
        edits.append(f"{manifest.parent.name}:{name}:feature={count}")
    manifest.write_text(text, encoding="utf-8", newline="\n")
    return edits


def stage_sources(stage: Path, profile: str) -> dict:
    sdk = stage / "sdk"
    sdk.mkdir(parents=True)
    package_dirs = metadata_package_dirs(profile)
    packages = []
    installed = {source.name for source in package_dirs}
    projection_edits = []
    for source in sorted(package_dirs, key=lambda p: p.as_posix()):
        repo, root = repo_for(source)
        rel = source.relative_to(root)
        if repo == "auto-down":
            packages.append({"name": source.name, "source_repo": repo,
                             "source_path": rel.as_posix(), "installed_path": f"auto-down/{rel.as_posix()}"})
            continue
        if rel.parts[0] != "crates":
            raise RuntimeError(f"non-crate package input requires explicit mapping: {source}")
        target = sdk / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        clean_tree(source, target)
        manifest = target / "Cargo.toml"
        if manifest.is_file():
            rewrite_manifest(manifest)
            if profile == "core" and source.name == "auto-core":
                text = manifest.read_text(encoding="utf-8")
                manifest.write_text(text.replace('workspace = "../../core"\n', ""),
                                    encoding="utf-8", newline="\n")
        packages.append({"name": source.name, "source_repo": repo,
                         "source_path": rel.as_posix(), "installed_path": rel.as_posix()})
    shared_targets = []
    if profile == "core":
        shared = MAN / "crates/auto"
        clean_tree(shared / "src", sdk / "crates/auto/src")
        shutil.copy2(shared / "build.rs", sdk / "crates/auto/build.rs")
        test_target = sdk / "crates/auto/tests"
        test_target.mkdir(parents=True)
        shared_targets = ["crates/auto/src", "crates/auto/build.rs"]
        core_manifest = tomllib.load((sdk / 'crates/auto-core/Cargo.toml').open('rb'))
        for test in core_manifest.get('test', []):
            target = (sdk / 'crates/auto-core' / test['path']).resolve()
            if not target.is_relative_to(sdk.resolve()):
                raise RuntimeError('Core shared test target escapes SDK')
            relative = target.relative_to(sdk.resolve())
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(MAN / relative, target)
            shared_targets.append(relative.as_posix())
    for manifest in sdk.glob("crates/*/Cargo.toml"):
        projection_edits.extend(project_missing_optional(manifest, installed))

    for rel in ROOT_INPUT_DIRS:
        clean_tree(LANG / rel, sdk / rel)
    for rel in ROOT_INPUTS:
        target = sdk / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(LANG / rel, target)
    clean_tree(LANG / "vendor/autodown-test-parser", sdk / "vendor/autodown-test-parser")
    clean_tree(MAN / "crates/autoui-skill", sdk / "crates/autoui-skill")
    lock = MAN / "core/Cargo.lock" if profile == "core" else MAN / "Cargo.lock"
    shutil.copy2(lock, sdk / "Cargo.lock")
    workspace = (MAN / "Cargo.toml").read_text(encoding="utf-8")
    # Web packages inherit these fixed package fields; Man's workspace did not
    # previously need them. They describe the Web producer, not the CLI version.
    members_start = workspace.index("members = [")
    members_end = workspace.index("]\ndefault-members", members_start) + 1
    member_list = ", ".join(f'"{path}"' for path in sorted({p["installed_path"] for p in packages if p["source_repo"] != "auto-down"}))
    workspace = workspace[:members_start] + f"members = [{member_list}]" + workspace[members_end:]
    default_member = "crates/auto-core" if profile == "core" else "crates/auto"
    workspace = workspace.replace('default-members = ["crates/auto"]',
                                  f'default-members = ["{default_member}"]')
    workspace = workspace.replace("../auto-lang/crates/patches/", "crates/patches/")
    if profile == "core":
        workspace = workspace.replace('exclude = ["crates/auto-core"]\n', "")
        workspace = workspace.split("[patch.crates-io]", 1)[0].rstrip() + "\n"
    workspace += '\n[workspace.package]\nversion = "0.1.0"\nedition = "2021"\nlicense = "MIT"\n'
    (sdk / "Cargo.toml").write_text(workspace, encoding="utf-8", newline="\n")
    run(["cargo", "generate-lockfile"], sdk)
    if profile == "full":
        clean_tree(DOWN, sdk / "auto-down")
        for rel in ("packages/blueprints", "packages/widgets"):
            clean_tree(LANG / rel, sdk / rel)
    license_repos = ["auto-lang", "auto-man"] if profile == "core" else ["auto-lang", "auto-man", "auto-ui"]
    if any(p['source_repo'] == 'auto-web' for p in packages):
        license_repos.append('auto-web')
    for repo in license_repos:
        license_root = {"auto-lang": LANG, "auto-man": MAN, "auto-ui": UI, "auto-web": WEB}[repo]
        target = stage / "licenses" / repo
        target.mkdir(parents=True, exist_ok=True)
        shutil.copy2(license_root / "LICENSE", target / "LICENSE")
    return {"sdk_layout": "combined-crates", "packages": packages,
            "projection_edits": projection_edits, "shared_targets": shared_targets,
            "root_inputs": ROOT_INPUTS + ROOT_INPUT_DIRS + ["vendor/autodown-test-parser", "crates/autoui-skill"]}


def manifest_files(root: Path) -> dict[str, dict]:
    files = {}
    for directory, names, filenames in os.walk(root, followlinks=False):
        names[:] = sorted(name for name in names if name not in {'target', '.git', 'node_modules', '__pycache__'})
        for name in (*names, *filenames):
            path = Path(directory) / name
            if path.is_symlink() or path.is_junction():
                raise RuntimeError(f'linked install input is forbidden: {path}')
        for name in sorted(filenames):
            path = Path(directory) / name
            if path == root / MANIFEST:
                continue
            files[path.relative_to(root).as_posix()] = {
                "bytes": path.stat().st_size,
                "sha256": sha256(path),
            }
    return files


def verify(root: Path, expected_profile: str | None = None) -> dict:
    path = root / MANIFEST
    if not path.is_file():
        raise RuntimeError(f"missing {MANIFEST}: {root}")
    doc = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(doc, dict) or doc.get("schema") != "man-install/1":
        raise RuntimeError(f"unsupported install manifest schema: {doc.get('schema') if isinstance(doc, dict) else None!r}")
    profile = doc.get("profile")
    if profile not in ("core", "full"):
        raise RuntimeError(f"unsupported install profile: {profile!r}")
    if expected_profile and profile != expected_profile:
        raise RuntimeError(f"profile mismatch: {profile} != {expected_profile}")
    actual = manifest_files(root)
    expected = doc["files"]
    if actual != expected:
        missing = sorted(set(expected) - set(actual))
        extra = sorted(set(actual) - set(expected))
        changed = sorted(k for k in set(actual) & set(expected) if actual[k] != expected[k])
        raise RuntimeError(f"install content mismatch: missing={missing}, extra={extra}, changed={changed}")
    if profile == "core":
        forbidden = [p for p in actual if p.startswith("sdk/crates/auto-ui")
                     or p.startswith("sdk/crates/auto-lang/") or p.startswith("sdk/crates/shell-pack")
                     or p.startswith("licenses/auto-ui/") or p == "sdk/crates/auto/Cargo.toml"
                     or p == exe("auto-ui-tool")]
        if forbidden:
            raise RuntimeError(f"UI source/tool in Core install: {forbidden}")
        for required in (exe("auto"), "sdk/crates/auto-core/Cargo.toml",
                         "sdk/crates/auto/src/main.rs", "sdk/crates/auto/build.rs"):
            if required not in actual:
                raise RuntimeError(f"Core required input missing: {required}")
        owners = {"auto-man", "auto-lang"}
        if any(p.startswith('sdk/crates/auto-web/') for p in actual):
            owners.add('auto-web')
            if 'licenses/auto-web/LICENSE' not in actual:
                raise RuntimeError('Web source license missing')
        if set(doc["sources"]) != owners:
            raise RuntimeError(f"Core source identity differs from actual owners {owners}: {doc['sources']}")
    else:
        for required in (exe("auto"), exe("auto-ui-tool"), "sdk/crates/auto-ui-tools/Cargo.toml"):
            if required not in actual:
                raise RuntimeError(f"full profile required input missing: {required}")
        owners = {'auto-man', 'auto-lang', 'auto-ui', 'auto-down'}
        if 'sdk/crates/auto-web/Cargo.toml' in actual:
            owners.add('auto-web')
            if 'licenses/auto-web/LICENSE' not in actual:
                raise RuntimeError('Web source license missing')
        if set(doc['sources']) != owners:
            raise RuntimeError(f'full source identity differs from actual owners {owners}')
    return {"profile": profile, "files": len(actual), "sources": doc["sources"]}


def unique_sibling(root: Path, kind: str) -> Path:
    return root.with_name(f".{root.name}.man-{kind}-{os.getpid()}-{time.time_ns()}")


def process_alive(pid: int) -> bool | None:
    if os.name == "nt":
        import ctypes
        ctypes.set_last_error(0)
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        handle = kernel.OpenProcess(0x1000, False, pid)
        if handle:
            kernel.CloseHandle(handle)
            return True
        error = ctypes.get_last_error()
        if error == 87:
            return False
        return None
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError:
        return None


def recover_stale_staging(root: Path) -> None:
    for candidate in root.parent.glob(f".{root.name}.man-staging-*"):
        marker = candidate / ".man-staging.marker"
        if not marker.is_file():
            continue
        try:
            owner = json.loads(marker.read_text(encoding="utf-8"))
            pid = int(owner["pid"])
            if owner.get("schema") != "man-staging/1":
                continue
        except (ValueError, TypeError, KeyError, json.JSONDecodeError):
            continue
        alive = process_alive(pid)
        if alive is False:
            shutil.rmtree(candidate)
        elif alive is None:
            print(f"man-install: owner liveness unknown for pid {pid}; preserving staging {candidate}",
                  file=sys.stderr)


def install(profile: str, install_root: Path, fail_point: str | None, skip_build: bool) -> dict:
    def prepare(stage):
        if not skip_build:
            if profile == "core":
                run(["cargo", "build", "--locked", "--manifest-path", "core/Cargo.toml",
                     "-p", "auto", "--no-default-features", "--features", "core"], MAN)
            else:
                run(["cargo", "build", "--locked", "-p", "auto"], MAN)
                run(["cargo", "build", "--locked", "-p", "auto-ui-tools"], UI)
        sources = {"auto-man": head(MAN), "auto-lang": head(LANG)}
        if profile == "full":
            sources.update({"auto-ui": head(UI), "auto-down": head(DOWN)})
        source_receipt = stage_sources(stage, profile)
        if any(p['source_repo'] == 'auto-web' for p in source_receipt['packages']):
            sources['auto-web'] = head(WEB)
        binary_root = MAN / "core/target/debug" if profile == "core" else MAN / "target/debug"
        shutil.copy2(binary_root / exe("auto"), stage / exe("auto"))
        if profile == "full":
            shutil.copy2(UI / "target/debug" / exe("auto-ui-tool"), stage / exe("auto-ui-tool"))
        doc = {
            "schema": "man-install/1", "profile": profile, "sources": sources,
            "source_closure": source_receipt,
            "files": manifest_files(stage),
        }
        (stage / MANIFEST).write_text(json.dumps(doc, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return install_payload(install_root, prepare, lambda path: verify(path, profile), fail_point,
                           verify_existing=verify)


def install_payload(install_root: Path, prepare, verify_payload, fail_point=None, *, verify_existing=None) -> dict:
    """Man-owned admission/staging/swap/recovery, also consumed by Web's tool artifact.

    Callbacks supply the domain payload and its strict receipt validation; the
    transaction and live/unknown staging-owner policy have exactly one source.
    """
    root = install_root.resolve()
    root.parent.mkdir(parents=True, exist_ok=True)
    if root.exists() and any(root.iterdir()):
        if not (root / MANIFEST).is_file():
            raise RuntimeError(f"refusing non-empty unmanaged install root: {root}")
        (verify_existing or verify_payload)(root)
    recover_stale_staging(root)
    stage = unique_sibling(root, "staging")
    backup = None
    swapped = False
    try:
        stage.mkdir()
        (stage / ".man-staging.marker").write_text(
            json.dumps({"schema": "man-staging/1", "pid": os.getpid()}) + "\n", encoding="utf-8")
        if fail_point == "prepare":
            raise RuntimeError("injected prepare failure")
        prepare(stage)
        verify_payload(stage)
        if fail_point == "verify":
            raise RuntimeError("injected verify failure")
        if root.exists():
            backup = unique_sibling(root, "backup")
            os.replace(root, backup)
            (backup / ".man-backup.marker").write_text("PLAN-832 preserved previous install\n", encoding="utf-8")
        if fail_point == "swap":
            raise RuntimeError("injected swap failure")
        os.replace(stage, root)
        swapped = True
        result = verify_payload(root)
        result.update({"install_root": str(root), "backup_root": str(backup) if backup else None,
                       "manifest": str(root / MANIFEST)})
        return result
    except Exception:
        # A post-swap failure must also restore the old usable installation.
        # Preserve the failed new payload for diagnosis, rather than deleting it.
        if swapped and root.exists():
            failed = unique_sibling(root, "failed")
            os.replace(root, failed)
        if backup and backup.exists() and not root.exists():
            (backup / ".man-backup.marker").unlink(missing_ok=True)
            os.replace(backup, root)
        if stage.exists() and (stage / ".man-staging.marker").is_file():
            shutil.rmtree(stage)
        raise


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    install_parser = sub.add_parser("install")
    install_parser.add_argument("--profile", choices=("core", "full"), required=True)
    install_parser.add_argument("--install-root", type=Path, required=True)
    install_parser.add_argument("--fail-point", choices=("prepare", "verify", "swap"))
    install_parser.add_argument("--skip-build", action="store_true")
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--install-root", type=Path, required=True)
    verify_parser.add_argument("--profile", choices=("core", "full"))
    args = parser.parse_args(argv)
    try:
        if args.command == "install":
            result = install(args.profile, args.install_root, args.fail_point, args.skip_build)
        else:
            result = verify(args.install_root.resolve(), args.profile)
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
        return 0
    except Exception as exc:
        print(f"man-install: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
