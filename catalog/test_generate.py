"""Original recipes, inventory, deterministic bytes and real Rust decode gates."""

import contextlib
import ctypes as C
import hashlib
import importlib.util
import io
import json
import pathlib
import struct
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "catalog_generator", pathlib.Path(__file__).with_name("generate.py")
)
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


class CatalogGenerationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temp.cleanup)
        cls.first = pathlib.Path(cls.temp.name) / "first"
        cls.second = pathlib.Path(cls.temp.name) / "second"
        cls.bridge = generator.ROOT / (
            "frontend/target/release/libdomowik_bridge."
            + ("dylib" if sys.platform == "darwin" else "so")
        )
        with contextlib.redirect_stdout(io.StringIO()):
            generator.generate(cls.first, cls.bridge)
            generator.generate(cls.second, cls.bridge)
        cls.index = json.loads((cls.first / "index.json").read_text())

    def test_exactly_100_distinct_inventory_entries_and_small_files(self):
        entries = self.index["models"]
        self.assertEqual(len(entries), 100)
        self.assertEqual(len({m["id"] for m in entries}), 100)
        self.assertEqual(len({m["model"]["sha256"] for m in entries}), 100)
        self.assertLess((self.first / "index.json").stat().st_size, 64 * 1024)
        for entry in entries:
            self.assertEqual(entry["license"], "Apache-2.0")
            for field, limit in [("model", 256 * 1024), ("thumbnail", 64 * 1024)]:
                item = entry[field]
                raw = (self.first / item["path"]).read_bytes()
                self.assertEqual(hashlib.sha256(raw).hexdigest(), item["sha256"])
                self.assertEqual(len(raw), item["bytes"])
                self.assertLessEqual(len(raw), limit)
            png = (self.first / entry["thumbnail"]["path"]).read_bytes()
            self.assertEqual(struct.unpack(">II", png[16:24]), (256, 256))
            glb = (self.first / entry["model"]["path"]).read_bytes()
            self.assertEqual(struct.unpack_from("<I", glb, 8)[0], len(glb))
            doc = json.loads(glb[20 : 20 + struct.unpack_from("<I", glb, 12)[0]])
            self.assertEqual(doc["asset"]["extras"]["license"], "Apache-2.0")
            self.assertNotIn("images", doc)
            self.assertFalse(any("uri" in b for b in doc["buffers"]))

    def test_all_published_bytes_are_reproducible(self):
        for path in self.first.rglob("*"):
            if path.is_file():
                self.assertEqual(
                    path.read_bytes(),
                    (self.second / path.relative_to(self.first)).read_bytes(),
                )

    def test_every_model_imports_and_has_the_declared_physical_size(self):
        lib = C.CDLL(str(self.bridge))
        lib.domowik_plan_create_default.argtypes = [C.POINTER(C.c_uint64)]
        lib.domowik_plan_import_model.argtypes = [
            C.c_uint64,
            C.c_char_p,
            C.c_uint64,
            C.c_char_p,
            C.c_uint64,
            C.POINTER(generator.Buf),
        ]
        lib.domowik_plan_assets.argtypes = [
            C.c_uint64,
            C.c_char_p,
            C.c_uint64,
            C.POINTER(generator.Buf),
        ]
        lib.domowik_plan_close.argtypes = [C.c_uint64]
        lib.domowik_buffer_free.argtypes = [C.c_uint64]
        h = C.c_uint64()
        self.assertEqual(lib.domowik_plan_create_default(C.byref(h)), 0)

        def reply(fn, *args):
            out = generator.Buf()
            self.assertEqual(fn(h, *args, C.byref(out)), 0)
            try:
                return json.loads(C.string_at(out.data, out.len))
            finally:
                lib.domowik_buffer_free(out.token)

        try:
            for entry in self.index["models"]:
                raw = (self.first / entry["model"]["path"]).read_bytes()
                name = (entry["name"] + ".glb").encode()
                result = reply(
                    lib.domowik_plan_import_model, name, len(name), raw, len(raw)
                )
                self.assertEqual(result["digest"], "sha256:" + entry["model"]["sha256"])
                request = json.dumps(
                    {"kind": "size", "digest": result["digest"]}
                ).encode()
                size = reply(lib.domowik_plan_assets, request, len(request))
                self.assertTrue(size["physicalUnits"])
                for actual, declared in zip(
                    size["dimensionsMm"], entry["dimensionsMm"]
                ):
                    self.assertAlmostEqual(actual, declared, delta=0.51)
        finally:
            lib.domowik_plan_close(h)

    def test_refuses_overwriting_existing_distribution(self):
        with self.assertRaises(FileExistsError):
            generator.generate(self.first, self.bridge)


if __name__ == "__main__":
    unittest.main()
