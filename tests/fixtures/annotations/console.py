"""The console: annotations that link, and ones that must not."""

from __future__ import annotations

import collections
import json

import annotations.widgets.button
from typing import TYPE_CHECKING, Callable, Literal, Optional

from . import style as style_module
from .style import Style
from annotations.style import Color

if TYPE_CHECKING:
    from .widgets.button import Button


class ConsoleOptions:
    """Options."""


class Themed(style_module.Style):
    """A base class through an aliased module import."""


class Registry(collections.UserDict):
    """An external base class: no link."""


DEFAULT_STYLE: Style = Style()

COLORS: dict[str, Color] = {}


class Console:
    """Prints things."""

    def copy(self) -> ConsoleOptions:
        """A copy of the options."""

    def render(
        self,
        style: Optional[Style] = None,
        renderables: dict[str, list["Button"]] = {},
        on_done: Callable[[int], Optional[Style]] | None = None,
        mode: Literal["Style", "plain"] = "plain",
    ) -> Style | None:
        """Renders.

        Args:
            style: The style to use.
            renderables: What to render.
        """

    def find(self, name: str) -> annotations.widgets.button.Button:
        """An absolute import of the package itself."""

    def dump(self, decoder: json.JSONDecoder, data: collections.OrderedDict) -> None:
        """Types from other packages stay plain text, without a warning."""
