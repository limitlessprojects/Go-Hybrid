"""W-4664 (#4664): Static Dual Session Types for Task Rendezvous.

Implements compile-time binary session types, affine protocol choreography,
duality verification (HYB-E5002), linearity enforcement (HYB-E5003),
and zero-cost native Go channel erasure.
"""

from __future__ import annotations
from dataclasses import dataclass
from typing import Optional, Tuple, Union


# --- Session Type AST ---

@dataclass(frozen=True)
class End:
    def __str__(self) -> str:
        return "End"


@dataclass(frozen=True)
class Send:
    val_type: str
    continuation: SessionType

    def __str__(self) -> str:
        return f"Send[{self.val_type}, {self.continuation}]"


@dataclass(frozen=True)
class Recv:
    val_type: str
    continuation: SessionType

    def __str__(self) -> str:
        return f"Recv[{self.val_type}, {self.continuation}]"


SessionType = Union[End, Send, Recv]


def compute_dual(st: SessionType) -> SessionType:
    """Computes the dual session type:
    Dual(Send[T, S]) = Recv[T, Dual(S)]
    Dual(Recv[T, S]) = Send[T, Dual(S)]
    Dual(End) = End
    """
    if isinstance(st, End):
        return End()
    elif isinstance(st, Send):
        return Recv(val_type=st.val_type, continuation=compute_dual(st.continuation))
    elif isinstance(st, Recv):
        return Send(val_type=st.val_type, continuation=compute_dual(st.continuation))
    else:
        raise TypeError(f"Unknown session type: {st}")


# --- Linear Endpoint & Choreography Verifier ---

@dataclass
class ChannelEndpoint:
    channel_id: str
    session_type: SessionType
    is_consumed: bool = False


class SessionChoreographyVerifier:
    """Enforces affine protocol transitions and rendezvous duality at compile time."""

    def __init__(self):
        self.diagnostics: list[str] = []

    def create_channel(self, channel_id: str, proto: SessionType) -> Tuple[ChannelEndpoint, ChannelEndpoint]:
        """Creates an affine dual session channel pair (c_client, c_server)."""
        ep_a = ChannelEndpoint(f"{channel_id}_a", proto)
        ep_b = ChannelEndpoint(f"{channel_id}_b", compute_dual(proto))
        return ep_a, ep_b

    def verify_rendezvous(self, expected_type: SessionType, actual_type: SessionType) -> Tuple[bool, Optional[str]]:
        """Verifies that an incoming task argument matches the expected dual endpoint."""
        if expected_type != actual_type:
            err = (
                f"HYB-E5002: Session endpoint duality mismatch: "
                f"expected {expected_type}, found {actual_type}"
            )
            self.diagnostics.append(err)
            return False, err
        return True, None

    def step_send(self, ep: ChannelEndpoint, val_type: str) -> Tuple[Optional[ChannelEndpoint], Optional[str]]:
        """Advances endpoint through a Send operation, invalidating the previous linear handle."""
        if ep.is_consumed:
            err = f"HYB-E5003: Linear session endpoint '{ep.channel_id}' consumed and invalidated: affine reuse forbidden"
            self.diagnostics.append(err)
            return None, err

        if not isinstance(ep.session_type, Send):
            err = f"HYB-E5004: Protocol violation: expected {ep.session_type}, attempted Send"
            self.diagnostics.append(err)
            return None, err

        if ep.session_type.val_type != val_type:
            err = f"HYB-E5005: Payload type mismatch: expected {ep.session_type.val_type}, got {val_type}"
            self.diagnostics.append(err)
            return None, err

        # Invalidate current linear handle
        ep.is_consumed = True
        # Return advanced endpoint
        next_ep = ChannelEndpoint(ep.channel_id, ep.session_type.continuation)
        return next_ep, None

    def step_recv(self, ep: ChannelEndpoint) -> Tuple[Optional[ChannelEndpoint], Optional[str]]:
        """Advances endpoint through a Recv operation, invalidating the previous linear handle."""
        if ep.is_consumed:
            err = f"HYB-E5003: Linear session endpoint '{ep.channel_id}' consumed and invalidated: affine reuse forbidden"
            self.diagnostics.append(err)
            return None, err

        if not isinstance(ep.session_type, Recv):
            err = f"HYB-E5004: Protocol violation: expected {ep.session_type}, attempted Recv"
            self.diagnostics.append(err)
            return None, err

        # Invalidate current linear handle
        ep.is_consumed = True
        # Return advanced endpoint
        next_ep = ChannelEndpoint(ep.channel_id, ep.session_type.continuation)
        return next_ep, None

    def erase_to_native_go_channel(self, st: SessionType) -> str:
        """Erases session type wrappers into primitive native Go channel signatures."""
        if isinstance(st, (Send, Recv)):
            return f"chan {st.val_type}"
        return "chan struct{}"
