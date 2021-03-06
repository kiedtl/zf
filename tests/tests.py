#!/usr/bin/env python3

import os
from subprocess import Popen, PIPE, STDOUT

zfpath = "target/debug/zf"
testdir = 'tests'

FAILED = 0
PASSED = 0

def do_test(test):
    test = testdir + "/" + test

    testcode = ""
    expect = ""
    with open(test) as f:
        testcode = f.read()
    for line in testcode.split("\n"):
        if not line.startswith("\\\\ "):
            break
        expect += line[3:] + "\n"

    cmd = [zfpath, test]
    proc = Popen(cmd, stdout=PIPE, stderr=STDOUT, stdin=PIPE)
    out, err = proc.communicate(testcode.encode("utf-8"))
    output = out.decode("utf-8")
    exit = proc.wait()

    if not exit == 0 or not output == expect:
        return (False, expect, output)
    return (True, None)

failed = []

tests = [s for s in os.listdir(testdir) if 'test_' in s]
tests = list(sorted(tests, key=lambda i: i, ))

for test in tests:
    test_name = test.replace("test_", "", 1).replace(".zf", "", 1)
    print(f"testing \033[97m{test_name:8}\033[m... ", end="")

    r = do_test(test)
    if r[0]:
        PASSED += 1
        print("\033[1;32mok\033[m")
    else:
        FAILED += 1
        failed.append([test, r[1], r[2]])
        print("\033[1;31mFAILED\033[m")

print("\n\n", end="")

for failure in failed:
    print(f"{failure[0]}:")
    print(f"    expected     {repr(failure[1])}")
    print(f"    got          {repr(failure[2])}")
    print(f"")

print(f"{len(tests)} tests, {PASSED} passed, {FAILED} failed")
