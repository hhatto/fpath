
test:
	cd tests && python test_posixpath.py PosixPathTest
	cd tests && python test_posixpath.py PathLikeTests
	cd tests && python test_posixpath.py PosixCommonTest
	cd tests && python test_genericpath.py

install:
	python setup.py install

benchmark:
	cd benchmarks && zsh ./bench.sh

clean:
	python setup.py clean
	rm -rf *.egg-info dist build */__pycache__

all: install test benchmark
