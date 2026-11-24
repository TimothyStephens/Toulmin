"""The one exception the reference checker raises for a catalog error."""


class CheckError(Exception):
    """A catalog error (errors.md): `code`, `field` (JSON Pointer or None),
    and a human message. `field` may be filled in later by the caller that
    knows which member holds the offending text (11.12: an error inside an
    expression points at the member holding the expression, no finer)."""

    def __init__(self, code, message, field=None):
        super().__init__(f"{code} {field or ''}: {message}")
        self.code = code
        self.message = message
        self.field = field

    def at(self, field):
        """Return self with `field` set when it is not set yet."""
        if self.field is None:
            self.field = field
        return self
