"""
Reference iXML parser using tree-sitter.

Intentionally simple over fast — the point is to have an obviously correct
second implementation to QA against earleybird output, similar to how the
Excel team maintained a slow but comprehensible spreadsheet engine to validate
their production one.

Usage:
    # First build the tree-sitter grammar (one-time):
    #   tree-sitter generate && tree-sitter build
    #
    # Then compare against earleybird:
    #   eb parse -g grammar.ixml -i input.txt > eb_output.xml
    #   python parse.py grammar.ixml input.txt > ts_output.xml
    #   diff eb_output.xml ts_output.xml
"""

import sys
import argparse
from pathlib import Path
from tree_sitter import Language, Parser


def load_language(build_dir: Path) -> Language:
    so_path = build_dir / "ixml.so"
    if not so_path.exists():
        sys.exit(
            f"Grammar not built. Run: tree-sitter generate && tree-sitter build -o {so_path}"
        )
    return Language(str(so_path), "ixml")


def parse_input(grammar_path: Path, input_path: Path) -> None:
    build_dir = Path(__file__).parent / "build"
    language = load_language(build_dir)

    parser = Parser()
    parser.set_language(language)

    source = input_path.read_bytes()
    tree = parser.parse(source)

    # TODO: walk tree and emit XML matching earleybird's output format
    print(tree.root_node.sexp())


def main() -> None:
    ap = argparse.ArgumentParser(description="Reference iXML parser (tree-sitter)")
    ap.add_argument("grammar", type=Path, help="iXML grammar file")
    ap.add_argument("input", type=Path, help="Input file to parse")
    args = ap.parse_args()

    parse_input(args.grammar, args.input)


if __name__ == "__main__":
    main()
