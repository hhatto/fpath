.PHONY: test install benchmark clean all

test:
	cargo test
	cd tests && python test_posixpath.py PosixPathTest $(VERBOSE)
	cd tests && python test_posixpath.py PathLikeTests $(VERBOSE)
	cd tests && python test_posixpath.py PosixCommonTest $(VERBOSE)
	cd tests && python test_genericpath.py $(VERBOSE)

test-verbose: VERBOSE = "-v"
test-verbose: test

install:
	python setup.py install

install-pip:
	pip install --upgrade . $(VERBOSE)

install-pip-verbose: VERBOSE = "-v"
install-pip-verbose: install-pip

benchmark:
	cd benchmarks && zsh ./bench.sh

clean:
	python setup.py clean
	rm -rf *.egg-info dist build */__pycache__

all: install test benchmark
