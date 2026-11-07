"""Mixed, malformed and look-alike docstrings."""


def mixed(a: int, b: int) -> int:
    """Google sections with a stray reST field.

    Returns:
        int: The sum.

    Args:
        a: First.
        b: Second.

    :raises ValueError: If a is negative.
    :meta private:
    """


def malformed(x):
    """A half-written section is kept as text.

    Args:
        this line is not a parameter
        x: Fine on its own.

    Returns:
    """


def lookalike():
    """Prose that mentions Args: in passing is left alone.

    Returns are discussed in the guide.

    ```
    Args:
        x: inside a code block
    ```
    """


def plain():
    """Just Markdown, with **bold** text."""
