"""Application-specific exceptions."""


class AutoCutError(Exception):
    """Base exception for errors that should be shown to CLI users."""


class MediaError(AutoCutError):
    """Raised when an input media file cannot provide usable audio."""


class ModelError(AutoCutError):
    """Raised when model configuration or inference fails."""
