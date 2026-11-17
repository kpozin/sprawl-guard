"""Copies the site's full logo into the social card SVG.

An SVG viewed as an image can't load other files, so the card carries its own copy of
`sprawl-guard-shield-clamp.svg`, the logo with braces, inside the nested `<svg id="mark">` element.
`write` refreshes that copy; `check` exits with an error when it no longer matches the logo file.
"""

from __future__ import annotations

import argparse
import copy
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

SVG_NAMESPACE = "http://www.w3.org/2000/svg"
MARK_ID = "mark"
"""The `id` of the element in the card that holds the copied mark."""

MARK_CHROME_TAGS = {f"{{{SVG_NAMESPACE}}}title", f"{{{SVG_NAMESPACE}}}style"}
"""Mark children left out of the copy: the card has its own title, and sets the mark's color."""

INDENT = "  "

ET.register_namespace("", SVG_NAMESPACE)


def parse(path: Path) -> ET.ElementTree:
    """Parses an SVG file, keeping its comments."""
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    return ET.parse(path, parser)


def render_card(card_path: Path, mark_path: Path) -> str:
    """Returns the card's text with a fresh copy of the mark in its `<svg id="mark">`."""
    card = parse(card_path)
    mark = parse(mark_path).getroot()
    slot = card.getroot().find(f".//*[@id='{MARK_ID}']")
    if slot is None:
        raise SystemExit(f'{card_path} has no element with id="{MARK_ID}"')

    slot.attrib["viewBox"] = mark.attrib["viewBox"]
    slot[:] = [copy.deepcopy(child) for child in mark if child.tag not in MARK_CHROME_TAGS]
    ET.indent(card, space=INDENT)
    return ET.tostring(card.getroot(), encoding="unicode") + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["write", "check"])
    parser.add_argument("card", type=Path)
    parser.add_argument("mark", type=Path)
    args = parser.parse_args()

    expected = render_card(args.card, args.mark)
    match args.mode:
        case "write":
            args.card.write_text(expected)
        case "check":
            if args.card.read_text() != expected:
                sys.exit(
                    f"{args.card} is out of date with {args.mark}; "
                    "run just sprawl-guard-public-site-social-card"
                )


if __name__ == "__main__":
    main()
