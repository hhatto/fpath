from setuptools import setup, find_packages
from setuptools_rust import Binding, RustExtension


with open('README.md', 'r') as fh:
    long_description = fh.read()


print(long_description)

setup(
    name='fpath',
    version='0.1.2',
    description='fast path manipulation module written in Rust',
    long_description=long_description,
    long_description_content_type='text/markdown',
    author='Hideo Hattori',
    author_email='hhatto.jp@gmail.com',
    url='https://github.com/hhatto/fpath',
    license='MIT',
    classifiers=[
        'Development Status :: 3 - Alpha',
        'License :: OSI Approved :: MIT License',
        'Programming Language :: Python :: 2.7',
        'Programming Language :: Python :: 3.7',
    ],
    keywords='path, rust',
    packages=find_packages(),
    rust_extensions=[
        RustExtension('_fpath', 'Cargo.toml', binding=Binding.PyO3)],
    # rust extensions are not zip safe, just like C-extensions.
    zip_safe=False)
