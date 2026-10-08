"""Engine module.

Runs things.
"""
import os
from typing import Callable, Optional


class Base:
    pass


class Engine(Base):
    """The main engine.

    Example:
        >>> Engine().speed
        3
    """

    speed: int = 3

    def __init__(self, name: str = "x", *args, retries: int = 3, **kwargs) -> None:
        """Create an engine."""

    @property
    def status(self) -> dict[str, int]:
        """Status map."""
        return {}

    @staticmethod
    def helper(
        a: Optional[int],
        b: "Engine",
        c: int | None = None,
        d: os.PathLike = ".",
        e: Callable[[int], str] = str,
    ) -> list[str]:
        return []

    async def arun(self, /, x, *, y: int = 1) -> tuple[int, str]:
        """Run asynchronously."""
