"""A module without `__all__`: leading underscores decide."""


def visible() -> None:
    """Public."""


def _secret() -> None:
    """Private."""


class Widget:
    """Public class."""

    def __call__(self) -> None:
        """Dunders stay visible."""

    def _paint(self) -> None:
        """Private method."""


class _Base:
    """Private class."""
