"""Draws the homepage's two IDE explorer illustrations of the same TypeScript service.

One keeps every file directly in `src`, and the other groups the files into feature directories.

Run it with `just sprawl-guard-public-directory-trees`.
"""

import sys
from collections import defaultdict
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from xml.sax.saxutils import escape

PANEL_WIDTH = 340
ROW_HEIGHT = 22
HEADER_HEIGHT = 44
"""Height of the "EXPLORER" header above the first row."""
BOTTOM_PADDING = 12
INDENT = 16
SIDE_PADDING = 14


@dataclass(frozen=True)
class File:
    """A TypeScript source file."""

    name: str


@dataclass(frozen=True)
class Folder:
    """A directory, drawn expanded or collapsed."""

    name: str
    children: tuple["File | Folder", ...]
    expanded: bool = True


@dataclass(frozen=True)
class SourceFile:
    """One file of the service, named as each tree shows it."""

    flat_name: str
    organized_path: str
    """The path under `src` in the organized tree: `folder/name`, or a bare name for `src` itself."""


SOURCE_FILES = (
    SourceFile("api-router.ts", "api/router.ts"),
    SourceFile("auth-middleware.ts", "auth/middleware.ts"),
    SourceFile("auth-session.ts", "auth/session.ts"),
    SourceFile("auth-tokens.ts", "auth/tokens.ts"),
    SourceFile("billing-calculator.ts", "billing/calculator.ts"),
    SourceFile("billing-discounts.ts", "billing/discounts.ts"),
    SourceFile("billing-invoice-pdf.ts", "billing/invoice-pdf.ts"),
    SourceFile("billing-invoices.test.ts", "billing/invoices.test.ts"),
    SourceFile("billing-invoices.ts", "billing/invoices.ts"),
    SourceFile("billing-tax.test.ts", "billing/tax.test.ts"),
    SourceFile("billing-tax.ts", "billing/tax.ts"),
    SourceFile("config.ts", "config.ts"),
    SourceFile("customer-import.ts", "customers/import.ts"),
    SourceFile("customer-merge.ts", "customers/merge.ts"),
    SourceFile("customer-search.ts", "customers/search.ts"),
    SourceFile("customers.ts", "customers/customers.ts"),
    SourceFile("db-migrations.ts", "db/migrations.ts"),
    SourceFile("db-pool.ts", "db/pool.ts"),
    SourceFile("email-templates.ts", "notifications/email-templates.ts"),
    SourceFile("email.ts", "notifications/email.ts"),
    SourceFile("errors.ts", "api/errors.ts"),
    SourceFile("format.ts", "reports/format.ts"),
    SourceFile("helpers.ts", "notifications/scheduling.ts"),
    SourceFile("index.ts", "index.ts"),
    SourceFile("logger.ts", "logger.ts"),
    SourceFile("notifications.ts", "notifications/index.ts"),
    SourceFile("payments-refunds.ts", "payments/refunds.ts"),
    SourceFile("payments-webhooks.ts", "payments/webhooks.ts"),
    SourceFile("payments.test.ts", "payments/payments.test.ts"),
    SourceFile("payments.ts", "payments/payments.ts"),
    SourceFile("rate-limit.ts", "api/rate-limit.ts"),
    SourceFile("reports-export.ts", "reports/export.ts"),
    SourceFile("reports.ts", "reports/reports.ts"),
    SourceFile("retry.ts", "payments/retry.ts"),
    SourceFile("sms.ts", "notifications/sms.ts"),
    SourceFile("types.ts", "db/types.ts"),
    SourceFile("utils.ts", "payments/currency.ts"),
    SourceFile("utils.test.ts", "payments/currency.test.ts"),
    SourceFile("validation.ts", "api/validation.ts"),
    SourceFile("webhooks.ts", "notifications/webhooks.ts"),
)
"""Both trees draw these files, so the illustration shows the same files reorganized."""

EXPANDED_FOLDERS = frozenset({"api", "auth", "billing"})
"""The organized tree's folders drawn open; the rest are collapsed to keep it short."""


def flat_tree() -> Folder:
    """Every file directly inside `src`."""
    return Folder("src", tuple(File(file.flat_name) for file in SOURCE_FILES))


def organized_tree() -> Folder:
    """The files grouped into feature folders, with folders listed before files."""
    grouped: dict[str, list[File]] = defaultdict(list)
    for file in SOURCE_FILES:
        folder, _, name = file.organized_path.rpartition("/")
        grouped[folder].append(File(name))
    folders = tuple(
        Folder(name, sort_files(grouped[name]), expanded=name in EXPANDED_FOLDERS)
        for name in sorted(grouped.keys() - {""})
    )
    return Folder("src", folders + sort_files(grouped[""]))


def sort_files(files: list[File]) -> tuple[File, ...]:
    """Orders files by name."""
    return tuple(sorted(files, key=lambda file: file.name))


STYLE = """
    :root {
      --panel: #f6f8f7; --border: #d5ddd8; --text: #18242d; --muted: #5b6b74;
      --ts: #2f6fb3; --folder: #7d8f99; --guide: #e1e7e3;
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --panel: #141c21; --border: #2a363d; --text: #eef4f0; --muted: #9aa9b1;
        --ts: #6ea8e6; --folder: #8597a1; --guide: #222d33;
      }
    }
    text { font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; }
    .name { font-size: 13px; fill: var(--text); }
    .label { font-size: 11px; font-weight: 600; letter-spacing: 0.08em; fill: var(--muted); }
    .ts { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 8.5px;
          font-weight: 700; fill: var(--ts); }
"""


@dataclass(frozen=True)
class Row:
    """One visible line of the explorer, with the number of visible rows nested under it."""

    depth: int
    entry: File | Folder
    descendants: int


def rows(folder: Folder, depth: int = 0) -> Iterator[Row]:
    """Lists the visible rows of a folder and its expanded descendants, in display order."""
    nested = [
        row
        for child in (folder.children if folder.expanded else ())
        for row in (
            rows(child, depth + 1)
            if isinstance(child, Folder)
            else [Row(depth + 1, child, 0)]
        )
    ]
    yield Row(depth, folder, len(nested))
    yield from nested


def row_center(index: int) -> float:
    """The vertical center of a row."""
    return HEADER_HEIGHT + index * ROW_HEIGHT + ROW_HEIGHT / 2


def chevron(x: float, y: float, expanded: bool) -> str:
    """Draws the arrow that shows whether a folder is expanded."""
    d = f"M{x - 4} {y - 2}l4 4 4-4" if expanded else f"M{x - 2} {y - 4}l4 4-4 4"
    return (
        f'<path d="{d}" fill="none" stroke="var(--folder)" stroke-width="1.6" '
        'stroke-linecap="round" stroke-linejoin="round"/>'
    )


def folder_icon(x: float, y: float) -> str:
    """Draws an outlined folder."""
    return (
        f'<path d="M{x} {y - 5}h5l2 2h7v9h-14z" fill="none" stroke="var(--folder)" '
        'stroke-width="1.4" stroke-linejoin="round"/>'
    )


def draw_row(index: int, row: Row) -> str:
    """Draws one row, and the indent guide under it if it is an expanded folder."""
    y = row_center(index)
    x = SIDE_PADDING + row.depth * INDENT
    name = escape(row.entry.name)
    match row.entry:
        case File():
            return (
                f'<text class="ts" x="{x + 16}" y="{y + 3}">TS</text>'
                f'<text class="name" x="{x + 32}" y="{y + 4.5}">{name}</text>'
            )
        case Folder(expanded=expanded):
            guide = ""
            if row.descendants > 0:
                top = HEADER_HEIGHT + (index + 1) * ROW_HEIGHT
                bottom = top + row.descendants * ROW_HEIGHT
                guide = f'<path d="M{x + 4} {top}V{bottom}" stroke="var(--guide)"/>'
            return (
                guide
                + chevron(x + 4, y, expanded)
                + folder_icon(x + 14, y)
                + f'<text class="name" x="{x + 34}" y="{y + 4.5}">{name}</text>'
            )


def svg(title: str, tree: Folder) -> str:
    """Draws an explorer panel just tall enough to show `tree`."""
    visible_rows = list(rows(tree))
    height = HEADER_HEIGHT + len(visible_rows) * ROW_HEIGHT + BOTTOM_PADDING
    body = "".join(draw_row(index, row) for index, row in enumerate(visible_rows))
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {PANEL_WIDTH} {height}" role="img">
  <title>{escape(title)}</title>
  <style>{STYLE}  </style>
  <rect x="0.5" y="0.5" width="{PANEL_WIDTH - 1}" height="{height - 1}" rx="10" fill="var(--panel)" stroke="var(--border)"/>
  <text class="label" x="{SIDE_PADDING}" y="26">EXPLORER</text>
  <path d="M0 {HEADER_HEIGHT - 6}H{PANEL_WIDTH}" stroke="var(--border)"/>
  {body}
</svg>
"""


def main() -> None:
    """Writes both illustrations into the directory given as the argument."""
    out = Path(sys.argv[1])
    flat = flat_tree()
    (out / "directory-tree-flat.svg").write_text(
        svg(f"A src directory with {len(flat.children)} files directly inside it", flat)
    )
    (out / "directory-tree-organized.svg").write_text(
        svg("The same files organized into feature directories", organized_tree())
    )


if __name__ == "__main__":
    main()
