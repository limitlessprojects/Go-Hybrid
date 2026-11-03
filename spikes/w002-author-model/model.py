"""Bounded W-002 capture experiment; no .hy parser or production safety claim."""

from dataclasses import dataclass


class Rejected(ValueError):
    pass


@dataclass(frozen=True)
class Capability:
    kind: str
    resource: int = -1
    captures: tuple = ()


class Context:
    def __init__(self):
        self.bindings = {}
        self.next_resource = 0

    def vacant(self, name):
        # No shadowing in the admitted fragment, including consumed names.
        if name in self.bindings:
            raise Rejected("destination already bound")

    def get(self, name):
        value = self.bindings.get(name)
        if value is None:
            raise Rejected("unbound or consumed")
        return value

    def allocate(self, name):
        self.vacant(name)
        self.bindings[name] = Capability("own", self.next_resource)
        self.next_resource += 1

    def literal(self, name, immutable=False):
        self.vacant(name)
        self.bindings[name] = Capability("share" if immutable else "scalar")

    def copy(self, target, source):
        self.vacant(target)
        value = self.get(source)
        if value.kind not in ("scalar", "share"):
            raise Rejected("explicit move or read borrow required")
        self.bindings[target] = value

    @staticmethod
    def descendants(value):
        yield value
        for child in value.captures:
            yield from Context.descendants(child)

    def borrowed(self, resource):
        return any(
            node.kind == "borrow" and node.resource == resource
            for value in self.bindings.values() if value is not None
            for node in self.descendants(value)
        )

    def transferable(self, value):
        for node in self.descendants(value):
            if node.kind == "own" and self.borrowed(node.resource):
                raise Rejected("live read borrow prevents transfer")

    def move(self, target, source):
        self.vacant(target)
        value = self.get(source)
        self.transferable(value)
        self.bindings[source] = None
        self.bindings[target] = value

    def borrow(self, target, source):
        self.vacant(target)
        value = self.get(source)
        if value.kind not in ("own", "borrow"):
            raise Rejected("read borrow requires a resource")
        self.bindings[target] = Capability("borrow", value.resource)

    def write(self, name):
        value = self.get(name)
        if value.kind != "own" or self.borrowed(value.resource):
            raise Rejected("write requires an unborrowed owner")

    def freeze(self, target, source):
        self.vacant(target)
        value = self.get(source)
        if value.kind != "own":
            raise Rejected("freeze requires owner")
        self.transferable(value)
        self.bindings[source] = None
        self.bindings[target] = Capability("share", value.resource)

    def closure(self, target, captures):
        """captures is a sequence of (move|borrow|copy, source-name)."""
        self.vacant(target)
        names = [source for _, source in captures]
        if len(names) != len(set(names)):
            raise Rejected("duplicate capture")
        staged = []
        consumed = []
        for mode, source in captures:
            value = self.get(source)
            if mode == "move":
                self.transferable(value)
                consumed.append(source)
                staged.append(value)
            elif mode == "borrow" and value.kind in ("own", "borrow"):
                staged.append(Capability("borrow", value.resource))
            elif mode == "copy" and value.kind in ("share", "scalar"):
                staged.append(value)
            else:
                raise Rejected("unsupported capture")
        # Commit only after the entire capture list validates.
        for source in consumed:
            self.bindings[source] = None
        self.bindings[target] = Capability("closure", captures=tuple(staged))

    def spawn(self, name):
        value = self.get(name)
        self.transferable(value)
        if value.kind != "closure":
            raise Rejected("spawn requires a closure")
        if any(node.kind == "borrow" for node in self.descendants(value)):
            raise Rejected("borrowed capture cannot cross task boundary")
        self.bindings[name] = None

    def call(self, name):
        if self.get(name).kind != "closure":
            raise Rejected("call requires closure; unknown summaries rejected")
        self.bindings[name] = None  # affine closure, body outside experiment

    def drop(self, name):
        value = self.get(name)
        self.transferable(value)
        self.bindings[name] = None
