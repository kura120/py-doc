"""Public API, limited by __all__."""

__all__ = ["Public", "listed"]


class Public:
    """Listed in __all__ and documented."""

    def __init__(self):
        pass

    def run(self):
        pass

    def _hidden(self):
        pass


def listed():
    pass


def unlisted():
    pass
