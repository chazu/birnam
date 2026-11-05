.PHONY: build test verify doctor clean
build:
	python3 tools/build.py build
test:
	python3 tools/build.py test
verify: test
doctor:
	python3 tools/build.py doctor
clean:
	python3 tools/build.py clean
