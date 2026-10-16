import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('build_manifest', ROOT / 'scripts/build-manifest.py')
build_manifest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build_manifest)

SOURCE = b'export interface OhEngine {\n  start(): void\n}\nexport interface OhStartupLifecycle {\n  suspend(): void\n}\n'
PACKAGED = SOURCE.replace(b'  ', b'    ').replace(b'void\n', b'void;\n')


def elf(machine=183, payload=b'fixture'):
    header = bytearray(64)
    header[:4] = b'\x7fELF'
    header[4] = 2  # ELF64
    header[5] = 1  # little endian
    header[6] = 1  # current ELF version
    header[16:18] = (3).to_bytes(2, 'little')  # ET_DYN
    header[18:20] = machine.to_bytes(2, 'little')
    header[20:24] = (1).to_bytes(4, 'little')
    return bytes(header) + payload


class ManifestTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.har = self.root / 'UniClipboardEngine.har'
        self.declaration = self.root / 'index.d.ts'
        self.declaration.write_bytes(SOURCE)
        self.native = self.root / 'libuc_ohos_napi.so'
        self.native.write_bytes(elf())
        self.write_har()

    def write_har(self, declarations=PACKAGED, native=None, api=24, version='1.1.0-test'):
        if native is None:
            native = elf()
        members = {
            'package/oh-package.json5': json.dumps({'name': '@uniclipboard/engine', 'version': version, 'compatibleSdkVersion': api}).encode(),
            'package/Index.d.ets': b'export default engine',
            'package/src/main/cpp/types/libuc_ohos_napi/index.d.ts': declarations,
            'package/libs/arm64-v8a/libuc_ohos_napi.so': native,
        }
        with tarfile.open(self.har, 'w:gz') as archive:
            for name, data in members.items():
                entry = tarfile.TarInfo(name)
                entry.size = len(data)
                archive.addfile(entry, io.BytesIO(data))

    def manifest(self, sdk=24):
        return build_manifest.manifest(self.har, self.declaration, self.native, '1.1.0-test',
                                       'a' * 40, 'modified', 'b' * 64, sdk)

    def test_manifest_records_source_and_packaged_declarations_separately(self):
        result = self.manifest()
        self.assertTrue(result['development'])
        self.assertEqual(result['sha256'], hashlib.sha256(self.har.read_bytes()).hexdigest())
        self.assertEqual(result['sourceDeclarationsSha256'], hashlib.sha256(SOURCE).hexdigest())
        self.assertEqual(result['declarationsSha256'], hashlib.sha256(PACKAGED).hexdigest())
        self.assertEqual(result['nativeSha256'], hashlib.sha256(self.native.read_bytes()).hexdigest())
        self.assertEqual(result['sourceNativeSha256'], hashlib.sha256(self.native.read_bytes()).hexdigest())

    def test_stripped_native_is_accepted_and_recorded(self):
        packaged = elf(payload=b'stripped')
        self.write_har(native=packaged)
        result = self.manifest()
        self.assertEqual(result['nativeSha256'], hashlib.sha256(packaged).hexdigest())
        self.assertEqual(result['sourceNativeSha256'], hashlib.sha256(self.native.read_bytes()).hexdigest())

    def test_invalid_native_is_rejected(self):
        self.write_har(native=b'old native')
        with self.assertRaisesRegex(ValueError, 'HAR native library is not an ELF'):
            self.manifest()

    def test_wrong_architecture_is_rejected(self):
        self.write_har(native=elf(machine=62))
        with self.assertRaisesRegex(ValueError, 'HAR native library is not an AArch64'):
            self.manifest()

    def test_empty_native_is_rejected(self):
        self.write_har(native=b'')
        with self.assertRaisesRegex(ValueError, 'HAR native library is empty'):
            self.manifest()

    def test_missing_method_is_rejected(self):
        self.write_har(declarations=PACKAGED.replace(b'    start(): void;\n', b''))
        with self.assertRaisesRegex(ValueError, 'stale method'):
            self.manifest()

    def test_missing_interface_has_explicit_error(self):
        self.write_har(declarations=b'export interface Different {}')
        with self.assertRaisesRegex(ValueError, 'missing interface OhEngine'):
            self.manifest()

    def test_wrong_version_and_sdk_are_rejected(self):
        for options in [{'version': '0.0.1'}, {'api': 22}, {'api': 25}]:
            with self.subTest(options=options):
                self.write_har(**options)
                with self.assertRaises(ValueError):
                    self.manifest()
        self.write_har()
        with self.assertRaises(ValueError):
            self.manifest(sdk=22)


class SourceFingerprintTests(unittest.TestCase):
    def test_tracks_staged_unstaged_and_untracked_content_but_ignores_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(['git', '-C', directory, *args], stderr=subprocess.DEVNULL)
            git('init')
            (root / '.gitignore').write_text('target/\n')
            source = root / 'source.rs'
            source.write_text('initial')
            git('add', '.')
            git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-m', 'fixture')
            baseline = build_manifest.fingerprint(root)
            (root / 'target').mkdir()
            (root / 'target/built.so').write_bytes(b'output')
            self.assertEqual(baseline, build_manifest.fingerprint(root))
            source.write_text('changed')
            changed = build_manifest.fingerprint(root)
            self.assertNotEqual(baseline, changed)
            git('add', 'source.rs')
            self.assertEqual(changed, build_manifest.fingerprint(root))
            extra = root / 'new.rs'
            extra.write_text('new source')
            untracked = build_manifest.fingerprint(root)
            self.assertNotEqual(changed, untracked)
            extra.write_text('different source')
            self.assertNotEqual(untracked, build_manifest.fingerprint(root))
