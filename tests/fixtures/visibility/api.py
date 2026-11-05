"""A module whose public API is fixed by `__all__`.

#pd-doc-link: api.connect
#pd-doc-link: api._make_pool
#pd-doc-link: plain._secret
#pd-doc-link: _helpers.normalize
"""

__all__ = ["connect", "Client"]
__all__ += ["_exported_private"]
__all__.extend(("Pool",))


def connect(url: str) -> "Client":
    """Opens a client."""


def not_exported() -> None:
    """Public by name, but missing from `__all__`."""


def _exported_private() -> None:
    """Private by name, but listed in `__all__`."""


def _make_pool() -> None:
    """Private and not exported."""


class Client:
    """A client."""

    def __init__(self, url: str) -> None:
        """Creates the client."""

    def __eq__(self, other: object) -> bool:
        """Compares two clients."""

    def send(self, data: bytes) -> None:
        """Sends data."""

    def _retry(self) -> None:
        """Private helper."""


class Pool:
    """A pool."""


class Hidden:
    """Not exported."""
