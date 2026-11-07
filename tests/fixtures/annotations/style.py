"""Styles."""

from enum import Enum


class Style:
    """A style."""


class Color(Enum):
    RED = 1


def parse_style(text: str) -> "Style":
    """Parses a style."""
