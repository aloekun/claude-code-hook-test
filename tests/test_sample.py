from src.sample import greet


def test_greet_returns_name():
    assert greet("world") == "world"


def test_greet_with_other_name():
    assert greet("Alice") == "Alice"
