"""A page one folder down, importing two levels up."""

from ..console import Console
from ..style import Style as BaseStyle


class Button(BaseStyle):
    """A base class through an aliased relative import."""


def press(console: "Console", times: int = 1) -> "Button":
    """Presses the button."""
