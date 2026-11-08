"""Google-style docstrings."""


class Engine:
    """Runs jobs.

    Attributes:
        speed (int): How fast it runs.
        name: Its name.

    Args:
        name: The engine's name.
        speed: Jobs per second.
    """

    def __init__(self, name: str = "main", speed: int = 3) -> None:
        self.name = name
        self.speed = speed

    def run(self, job: str, *args, retries: int = 1, **kwargs) -> bool:
        """Runs one job.

        Some more text about running.

        Args:
            job: The job to run.
                #sd-doc-link: google.Engine
            *args: Passed to the job.
            retries (int, optional): How often to retry.
                #sd-note: Retries back off exponentially.
            **kwargs: Options for the job.
            ghost: Documented but not in the signature.

        Returns:
            bool: True if the job **succeeded**.

        Raises:
            ValueError: If the job is empty.
            RuntimeError: If the engine is stopped.

        Example:
            >>> Engine().run("build")
            True

        Note:
            Jobs run one at a time.

        Warning:
            Never run two engines on one database.
        """

    def stream(self):
        """Streams results.

        Yields:
            str: One line of output.
        """

    def copy(self) -> "Engine":
        """Copies the engine.

        Returns: a copy of self.
        """
