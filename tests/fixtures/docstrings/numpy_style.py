"""NumPy-style docstrings."""


def mean(values: list[float], axis: int = 0, *, weights=None) -> float:
    """Compute the arithmetic mean.

    Parameters
    ----------
    values : array_like
        The values to average.
    axis : int, optional
        Axis along which the means are computed.
    weights
        Optional weights, one per value.

    Returns
    -------
    float
        The mean of `values`.

    Raises
    ------
    ZeroDivisionError
        If `values` is empty.

    See Also
    --------
    median : The middle value.

    Notes
    -----
    The mean is the sum divided by the count.

    Examples
    --------
    >>> mean([1, 2, 3])
    2.0
    """


def split(text: str) -> tuple[str, str]:
    """Split text in two.

    Returns
    -------
    head : str
        The first half.
    tail : str
        The second half.
    """
