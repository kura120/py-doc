"""Definitions inside if / try / with / TYPE_CHECKING blocks.

#pd-doc-link: compat.Outer.Inner
#pd-doc-link: compat.Outer.Inner.Deep.ping
#pd-doc-link: Outer.Inner.LIMIT
#pd-doc-link: compat.Outer.Meta
#pd-doc-link: compat.Outer._Private
"""

import sys
from typing import TYPE_CHECKING, overload

if TYPE_CHECKING:
    from collections.abc import Callable

    Handler = Callable[[str], None]
    """Only for type checkers."""

    def runtime_twin() -> int:
        """Type-checking stub, replaced by the runtime definition below."""

if sys.version_info >= (3, 11):

    def read_config(path: str) -> dict:
        """Uses tomllib."""

    MODE = "fast"
elif sys.version_info >= (3, 9):

    def read_config(path: str, strict: bool) -> dict:
        """Second branch: dropped."""

else:

    def read_config(path: str, legacy: int) -> dict:
        """Third branch: dropped."""

    MODE = "slow"

try:
    from ujson import dumps
except ImportError:

    def dumps(obj: object) -> str:
        """Fallback when ujson is missing."""

else:
    BACKEND = "ujson"
finally:
    CLEANED_UP = True

with open(__file__) as _f:
    SOURCE_LENGTH = 0


def runtime_twin() -> int:
    """The runtime definition."""


@overload
def scale(x: int) -> int: ...
@overload
def scale(x: float) -> float: ...
def scale(x):
    """Overloads in one suite are all kept."""


def factory():
    def inner_helper():
        """Inside a function body: never documented."""

    class InnerClass:
        """Inside a function body: never documented."""


class Outer:
    """Has nested classes."""

    class Inner:
        """A nested class."""

        LIMIT = 3

        class Deep:
            """Two levels down."""

            def ping(self) -> None:
                """Pong."""

    class _Private:
        """Hidden by default."""

    class Meta:
        ordering = ["name"]

    if sys.platform == "win32":

        def path(self) -> str:
            """Windows."""

    else:

        def path(self) -> str:
            """POSIX: dropped."""


if __name__ == "__main__":

    def main() -> None:
        """Script code, never documented."""

    SCRIPT_ONLY = 1
