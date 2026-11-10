import os

from app.main import data_dir, greet


def test_greet():
    assert greet("world") == "hello, world"


def test_data_dir_comes_from_env():
    os.environ["DATA_DIR"] = "/tmp/example-data"
    try:
        assert data_dir() == "/tmp/example-data"
    finally:
        del os.environ["DATA_DIR"]
