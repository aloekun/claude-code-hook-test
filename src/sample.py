# Sample Python file with intentional lint errors
import os
import json

unused_var = 42
another_unused = "test"


def greet(name):
    x = 10
    if name == "world":
        print("Hello, " + name)
    return name




result = greet("world")
print(result)
dead_code = 999
another_dead = "hook test v4"
