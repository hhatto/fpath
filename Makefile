.PHONY: test install benchmark clean all

test:
	cd tests && python test_posixpath.py PosixPathTest $(VERBOSE)
	cd tests && python test_posixpath.py PathLikeTests $(VERBOSE)
	cd tests && python test_posixpath.py PosixCommonTest $(VERBOSE)
	cd tests && python test_genericpath.py $(VERBOSE)

test-verbose: VERBOSE = "-v"
test-verbose: test

build:
	maturin build

install:
	maturin develop

benchmark:
	cd benchmarks && zsh ./bench.sh

clean:
	rm -rf target *.egg-info dist build */__pycache__

all: install test benchmark
