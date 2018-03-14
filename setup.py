from setuptools import setup, find_packages
from setuptools_rust import Binding, RustExtension


setup(name='fpath',
      version='0.1',
      packages=find_packages(),
      rust_extensions=[
          RustExtension('_fpath', 'Cargo.toml', binding=Binding.PyO3)],
      # rust extensions are not zip safe, just like C-extensions.
      zip_safe=False)
