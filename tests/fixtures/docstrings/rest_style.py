"""reST-style docstrings."""


def connect(host: str, port: int = 5432, *, timeout: float = 1.0):
    """Open a connection.

    :param host: Server to connect to.
    :param int port: Port number.
    :keyword timeout: Seconds before giving up.
    :raises ConnectionError: If the server does not answer.
    :returns: An open connection.
    :rtype: Connection

    .. note:: Connections are not thread-safe.

    .. warning::
       Close connections you open.
    """
