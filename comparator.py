#!/usr/bin/python

import sys
import json

print('Number of arguments:', len(sys.argv), 'arguments.')
print('Argument List:', str(sys.argv))

with open(sys.argv[1]) as base_file, open(sys.argv[2]) as new_file:
    base_data = json.load(base_file)
    new_data = json.load(new_file)

    for (base_suite, new_suite) in zip(base_data["results"]["suites"], new_data["results"]["suites"]):
        print(base_suite)
        print(new_suite)
