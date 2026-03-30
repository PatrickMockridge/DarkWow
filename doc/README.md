The DarkFi book
===============

This directory contains the sources for the book that can be read on
https://dark.fi/book/

When adding or removing a section of the book, make sure to update the
[SUMMARY.md](src/SUMMARY.md) file to actually list the contents.

## Reference Materials

**Uncensorable ZK and DarkFi Reference Material (Arweave)**

DarkFi Reference Material stored permanently on Arweave:
- [DarkFi Reference Material](https://app.ardrive.io/#/drives/f79597cd-8a4e-426e-840e-25c1453e418d=name=DarkFi+Reference+Material) - Textbooks, papers, and materials on ZK circuits, cryptography, and DarkFi

Use a python virtual environment to install its requirements:
```shell
% python -m venv venv
% source venv/bin/activate
```

> **Conda Users**: If using conda, consider running `conda deactivate` before creating venvs, or use `conda create -n mdbook python=3.x && conda activate mdbook` to avoid environment conflicts. The venv approach is recommended over conda for mdbook builds.

Then install the requirements:

```shell
% pip install -r requirements.txt
```

Using the Makefile to build the sources requires the Rust `mdbook`
utility which may be installed via:

```shell
cargo install mdbook
```

For the plugin mdbook backends run:

```
cargo install --git "https://github.com/lzanini/mdbook-katex"
cargo install --git "https://github.com/badboy/mdbook-toc"
cargo install --git "https://github.com/badboy/mdbook-mermaid" mdbook-mermaid
cargo install --git "https://github.com/rustforweb/mdbook-plugins" mdbook-tabs
```
