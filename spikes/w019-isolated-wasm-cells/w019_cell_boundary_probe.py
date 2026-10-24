"""W-019 intake probe; not a Hybrid compiler or a Go/WASM backend.

Requires Python 3.10+ and wasmtime==49.0.0. Run this file directly.
Uses real Wasm instances. No WASI, filesystem, network, or host callbacks.
"""
import json
import importlib.metadata
import wasmtime

assert importlib.metadata.version("wasmtime") == "49.0.0"
config = wasmtime.Config()
config.wasm_threads = False
config.consume_fuel = True
engine = wasmtime.Engine(config)
store = wasmtime.Store(engine)
store.set_fuel(100_000)
store.set_limits(memory_size=65_536, memories=8, instances=8)

OWN = '(module (memory (export "memory") 1 1))'
IMPORTED = '(module (import "host" "memory" (memory 1 1)) (export "memory" (memory 0)))'


def admit(wat):
    module = wasmtime.Module(engine, wat)
    if module.imports:
        raise ValueError("imports are outside this probe's admitted subset")
    return wasmtime.Instance(store, module, []).exports(store)["memory"]


def wire(value, depth=0):
    if depth > 8:
        raise ValueError("depth bound")
    if value is None or type(value) in (bool, str):
        return
    if type(value) is int and -(2**31) <= value < 2**31:
        return
    if type(value) is list and len(value) <= 32:
        for child in value:
            wire(child, depth + 1)
        return
    if type(value) is dict and len(value) <= 32 and all(type(k) is str for k in value):
        for child in value.values():
            wire(child, depth + 1)
        return
    raise ValueError("not an admitted scalar/tree value")


sender, receiver = admit(OWN), admit(OWN)
value = {"items": [{"n": 1}]}
wire(value)
encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
assert len(encoded) <= 4096
sender.write(store, encoded, 0)
snapshot = bytes(sender.read(store, 0, len(encoded)))
receiver.write(store, snapshot, 0)
value["items"][0]["n"] = 9
sender.write(store, json.dumps(value, sort_keys=True, separators=(",", ":")).encode(), 0)
assert json.loads(bytes(receiver.read(store, 0, len(encoded))))["items"][0]["n"] == 1
sender_before = bytes(sender.read(store, 0, len(encoded)))
receiver.write(store, b"X", 0)
assert bytes(sender.read(store, 0, len(encoded))) == sender_before
print("PASS: snapshot survives sender mutation; receiver mutation stays local")

for bad in (object(), lambda: None, b"pointer-like unsupported bytes"):
    try:
        wire(bad)
    except ValueError:
        pass
    else:
        raise AssertionError("unsupported value admitted")
try:
    admit(IMPORTED)
except ValueError:
    pass
else:
    raise AssertionError("host-memory import admitted")
print("PASS: unsupported host values and memory imports rejected")

# Deliberately bypass admission: an ordinary non-shared Memory can still be
# imported by TWO instances. Distinct Instance objects alone are insufficient.
foreign = wasmtime.Memory(store, wasmtime.MemoryType(wasmtime.Limits(1, 1)))
module = wasmtime.Module(engine, IMPORTED)
a = wasmtime.Instance(store, module, [foreign]).exports(store)["memory"]
b = wasmtime.Instance(store, module, [foreign]).exports(store)["memory"]
a.write(store, b"A", 0)
assert bytes(b.read(store, 0, 1)) == b"A"
print("NEGATIVE CONTROL FIRED: host import reintroduces cross-instance aliasing")
print("NOT TESTED: Go/WASM persistence, Hybrid lowering, complete admission, performance")
