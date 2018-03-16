
test:
	cd tests && python test_posixpath.py PosixPathTest
	cd tests && python test_genericpath.py

install:
	python setup.py install

benchmark:
	cd benchmarks && zsh ./bench.sh;  cd -
