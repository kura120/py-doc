"""Module variables, class attributes, dataclass fields and enum members."""

from dataclasses import dataclass, field
from enum import Enum, IntEnum
from typing import ClassVar, Union

__all__ = ["MAX_RETRIES", "TIMEOUT", "DEFAULT_NAME", "PALETTE", "Mode", "Point",
           "Level", "Number", "JsonValue", "pending", "Settings", "first", "second"]

MAX_RETRIES = 3
"""How often a request is retried."""

#: Seconds to wait for a reply.
#: Applies to every request.
TIMEOUT: float = 2.5

DEFAULT_NAME = "anonymous"  #: Name used when none is given.

PALETTE = {"red": "#ff0000", "green": "#00ff00", "blue": "#0000ff", "black": "#000000", "white": "#ffffff"}
"""A value too long to show in full."""

pending: int
"""Annotated, without a value."""

Number = Union[int, float]

type JsonValue = dict[str, "JsonValue"] | list["JsonValue"] | str | int | None

first, second = 1, 2

NOT_EXPORTED = "left out of __all__"


class Mode(Enum):
    """An enum."""

    FAST = "fast"
    """Go fast."""
    SLOW = "slow"
    _ignore_ = ["helper"]


class Level(IntEnum):
    LOW = 1
    HIGH = 2


@dataclass
class Point:
    """A dataclass."""

    x: float
    """Horizontal position."""
    y: float = 0.0
    tags: list[str] = field(default_factory=list)
    origin: ClassVar[str] = "top-left"
    _cache: dict = field(default_factory=dict)
    __slots__ = ("x", "y")

    def norm(self) -> float:
        """Length of the vector."""


class Settings:
    """A plain class with attributes."""

    debug = False
    #: Where logs go.
    log_path: str = "/var/log/app.log"
    __match_args__ = ("debug",)
    __hash__ = None
