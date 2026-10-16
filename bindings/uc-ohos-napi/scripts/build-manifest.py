#!/usr/bin/env python3
"""记录鸿蒙开发产物与实际源码、SDK 和声明的对应关系。"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import re
import tarfile


def validate_native(data, label):
    """Validate that a packaged native library is a non-empty ARM64 ELF."""
    if not data:
        raise ValueError(f'{label} native library is empty')
    if len(data) < 20 or data[:4] != b'\x7fELF':
        raise ValueError(f'{label} native library is not an ELF file')
    if data[4] != 2 or data[5] != 1:
        raise ValueError(f'{label} native library is not a little-endian ELF64 file')
    machine = int.from_bytes(data[18:20], 'little')
    if machine != 183:
        raise ValueError(f'{label} native library is not an AArch64 ELF')


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args])


def fingerprint(root):
    digest = hashlib.sha256(git(root, 'rev-parse', 'HEAD'))
    digest.update(git(root, 'diff', '--binary', 'HEAD'))
    for name in sorted(git(root, 'ls-files', '--others', '--exclude-standard', '-z').split(b'\0')):
        if name:
            digest.update(name + b'\0')
            digest.update((root / name.decode()).read_bytes())
    return digest.hexdigest()


def archive_bytes(archive, name):
    member = archive.getmember(name)
    if not member.isfile():
        raise ValueError('HAR member must be a regular file')
    return archive.extractfile(member).read()


def methods(data, name):
    match = re.search(r'export interface ' + name + r'\s*\{([\s\S]*?)\n\s*\}', data.decode())
    if match is None:
        raise ValueError(f'HAR declaration is missing interface {name}')
    return set(re.findall(r'^\s+(\w+)\(', match.group(1), re.M))


def manifest(har, declaration, native, version, commit, state, source_fingerprint, sdk_api):
    with tarfile.open(har) as archive:
        package = json.loads(archive_bytes(archive, 'package/oh-package.json5'))
        archive_bytes(archive, 'package/Index.d.ets')
        if package['name'] != '@uniclipboard/engine' or package['version'] != version:
            raise ValueError('HAR package identity does not match the source')
        minimum_api = package['compatibleSdkVersion']
        if not isinstance(minimum_api, int) or minimum_api < 24 or sdk_api < minimum_api:
            raise ValueError('HAR minimum API or build SDK mismatch')
        declarations = archive_bytes(archive, 'package/src/main/cpp/types/libuc_ohos_napi/index.d.ts')
        library = archive_bytes(archive, 'package/libs/arm64-v8a/libuc_ohos_napi.so')
    source_declarations = declaration.read_bytes()
    source_library = native.read_bytes()
    for name in ['OhEngine', 'OhStartupLifecycle']:
        if methods(declarations, name) != methods(source_declarations, name):
            raise ValueError('HAR contains stale method declarations')
    validate_native(source_library, 'source')
    validate_native(library, 'HAR')
    data = har.read_bytes()
    return {
        'schemaVersion': 1, 'development': True,
        'release': 'v' + version, 'commit': commit, 'sourceState': state,
        'sourceFingerprint': source_fingerprint, 'sdkApi': sdk_api,
        'minimumApi': minimum_api, 'artifact': 'UniClipboardEngine.har',
        'sha256': hashlib.sha256(data).hexdigest(), 'size': len(data),
        'declarationsSha256': hashlib.sha256(declarations).hexdigest(),
        'sourceDeclarationsSha256': hashlib.sha256(source_declarations).hexdigest(),
        'nativeSha256': hashlib.sha256(library).hexdigest(),
        'sourceNativeSha256': hashlib.sha256(source_library).hexdigest(),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--fingerprint', action='store_true')
    parser.add_argument('--expected-fingerprint')
    parser.add_argument('--har', type=Path)
    parser.add_argument('--declaration', type=Path)
    parser.add_argument('--native', type=Path)
    parser.add_argument('--sdk', type=Path)
    parser.add_argument('--version')
    args = parser.parse_args()
    current = fingerprint(args.root)
    if args.fingerprint:
        print(current)
        return
    if current != args.expected_fingerprint:
        raise ValueError('Source changed during build; rebuild before importing the HAR')
    sdk_api = int(json.loads((args.sdk / 'default/sdk-pkg.json').read_text())['data']['apiVersion'])
    result = manifest(args.har, args.declaration, args.native, args.version,
                      git(args.root, 'rev-parse', 'HEAD').decode().strip(),
                      'modified' if git(args.root, 'status', '--porcelain').strip() else 'clean',
                      current, sdk_api)
    output = args.har.with_suffix('.build.json')
    output.write_text(json.dumps(result, indent=2) + '\n')
    print(f'OK: {output}')


if __name__ == '__main__':
    main()
