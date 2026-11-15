#!/usr/bin/env node

/**
 * Checks that a `sprawl-guard languages --detected --format json` report found source files for
 * a language, which shows that a release build really scanned the checkout.
 *
 * Release jobs run this on Node.js 22 without installing workspace dependencies, so it uses only
 * Node.js 22 built-ins and reads the report without Zod.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const USAGE = "usage: check-languages-report.ts --report FILE --language CONFIG_KEY";

/** The result of checking a report. */
export type CheckLanguagesReportResult =
  | { readonly kind: "ok" }
  | { readonly kind: "invalid_report"; readonly detail: string }
  | { readonly kind: "unexpected_mode"; readonly mode: unknown }
  | { readonly kind: "language_missing"; readonly language: string };

/** Checks that the detected-languages report counts source files for the language. */
export function checkLanguagesReport(
  reportJson: string,
  language: string,
): CheckLanguagesReportResult {
  let report: unknown;
  try {
    report = JSON.parse(reportJson);
  } catch (error) {
    return { kind: "invalid_report", detail: String(error) };
  }
  if (
    typeof report !== "object" ||
    report === null ||
    !("mode" in report) ||
    !("languages" in report) ||
    !Array.isArray(report.languages)
  ) {
    return { kind: "invalid_report", detail: "expected mode and languages" };
  }
  if (report.mode !== "detected") {
    return { kind: "unexpected_mode", mode: report.mode };
  }
  const found = report.languages.some(
    (entry: unknown) =>
      typeof entry === "object" &&
      entry !== null &&
      "config_key" in entry &&
      entry.config_key === language &&
      "source_files" in entry &&
      typeof entry.source_files === "number" &&
      entry.source_files > 0,
  );
  return found ? { kind: "ok" } : { kind: "language_missing", language };
}

/** Explains a failed check in one line. */
function describeFailure(
  failure: Exclude<CheckLanguagesReportResult, { readonly kind: "ok" }>,
): string {
  switch (failure.kind) {
    case "invalid_report":
      return `invalid languages report: ${failure.detail}`;
    case "unexpected_mode":
      return `languages report has mode ${JSON.stringify(failure.mode)}, expected "detected"`;
    case "language_missing":
      return `languages report found no ${failure.language} source files`;
  }
}

function main(): number {
  let values;
  try {
    ({ values } = parseArgs({
      options: { language: { type: "string" }, report: { type: "string" } },
      strict: true,
    }));
  } catch (error) {
    console.error(`${String(error)}\n${USAGE}`);
    return 2;
  }
  if (values.report === undefined || values.language === undefined) {
    console.error(USAGE);
    return 2;
  }
  const result = checkLanguagesReport(readFileSync(values.report, "utf8"), values.language);
  if (result.kind !== "ok") {
    console.error(describeFailure(result));
    return 1;
  }
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
