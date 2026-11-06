.PHONY: build test verify doctor clean selfhost verify-selfhost
build:
	python3 tools/build.py build
test:
	python3 tools/build.py test
verify: test
doctor:
	python3 tools/build.py doctor
clean:
	python3 tools/build.py clean

selfhost: build
	python3 tools/selfhost.py
verify-selfhost: build
	python3 tests/selfhost_test.py
