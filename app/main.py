"""Minimal library/CLI skeleton. Replace with the real code.

The data path comes from the environment (DATA_DIR in .env/.env.dev), never hardcoded: in a session you
see .env.dev (DATA_DIR=./dev/data, fake data in the repo); the real path lives only in the masked .env and
is used outside the sandbox. There is no ./data in the repo tree.
"""
import os


def data_dir() -> str:
    """Where this app reads and writes its data. Set by DATA_DIR; falls back to the dev corpus."""
    return os.environ.get("DATA_DIR", "./dev/data")


def greet(name: str) -> str:
    return f"hello, {name}"


if __name__ == "__main__":
    print(greet("world"))
    print(f"data dir: {data_dir()}")
