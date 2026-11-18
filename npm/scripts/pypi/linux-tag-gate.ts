import { z } from "zod";
import { linuxPolicyTag, type LinuxWheelPolicy } from "./wheel-platforms.ts";

/**
 * The fields of `auditwheel show --json` that the gate reads. `overall_tag` is the oldest policy
 * that the wheel's binaries and the libraries they link satisfy, or a `linux_*` tag when they
 * satisfy none.
 */
const AUDITWHEEL_REPORT_SCHEMA = z.object({
  version: z.literal(1),
  pure: z.boolean(),
  overall_tag: z.string(),
});

/**
 * Matches the policy tags that the release's Linux wheels can carry. auditwheel reports the tag
 * only as a string, and nothing in this workspace parses wheel tags, so the gate matches the few
 * forms it accepts and rejects everything else.
 */
const LINUX_POLICY_TAG_PATTERN =
  /^(?<family>manylinux|musllinux)_(?<major>\d+)_(?<minor>\d+)_(?<arch>aarch64|x86_64)$/u;

/** Converts the named groups of {@link LINUX_POLICY_TAG_PATTERN} to a policy. */
const LINUX_POLICY_GROUPS_SCHEMA = z.object({
  family: z.enum(["manylinux", "musllinux"]),
  major: z.coerce.number().int(),
  minor: z.coerce.number().int(),
  arch: z.enum(["aarch64", "x86_64"]),
}) satisfies z.ZodType<LinuxWheelPolicy>;

/** Whether a wheel's declared Linux tag is one that its binary supports. */
export type LinuxTagGateResult =
  | { readonly kind: "ok"; readonly inferred: LinuxWheelPolicy }
  | { readonly kind: "invalid_report"; readonly detail: string }
  | { readonly kind: "pure_wheel" }
  | { readonly kind: "unrecognized_policy"; readonly overallTag: string }
  | {
      readonly kind: "policy_mismatch" | "declared_too_old";
      readonly declared: LinuxWheelPolicy;
      readonly inferred: LinuxWheelPolicy;
    };

/**
 * Compares a wheel's declared Linux policy with the one auditwheel inferred for it.
 *
 * The declared policy passes when it has the same libc family and architecture and is no older
 * than the inferred one. A wheel that claims an older system than its binary supports would
 * install there and then fail at startup.
 */
export function checkLinuxTag(declared: LinuxWheelPolicy, reportJson: string): LinuxTagGateResult {
  let report: unknown;
  try {
    report = JSON.parse(reportJson) as unknown;
  } catch (error) {
    return { kind: "invalid_report", detail: String(error) };
  }
  const parsed = AUDITWHEEL_REPORT_SCHEMA.safeParse(report);
  if (!parsed.success) {
    return { kind: "invalid_report", detail: z.prettifyError(parsed.error) };
  }
  if (parsed.data.pure) {
    return { kind: "pure_wheel" };
  }
  const inferred = parseLinuxPolicy(parsed.data.overall_tag);
  if (inferred === undefined) {
    return { kind: "unrecognized_policy", overallTag: parsed.data.overall_tag };
  }
  if (inferred.family !== declared.family || inferred.arch !== declared.arch) {
    return { kind: "policy_mismatch", declared, inferred };
  }
  const declaredIsOlder =
    declared.major < inferred.major ||
    (declared.major === inferred.major && declared.minor < inferred.minor);
  return declaredIsOlder ? { kind: "declared_too_old", declared, inferred } : { kind: "ok", inferred };
}

/** Explains a failed gate in one line. */
export function describeLinuxTagGateFailure(
  failure: Exclude<LinuxTagGateResult, { readonly kind: "ok" }>,
): string {
  switch (failure.kind) {
    case "invalid_report":
      return `auditwheel printed an unexpected report: ${failure.detail}`;
    case "pure_wheel":
      return "auditwheel found no platform-specific binary in the wheel";
    case "unrecognized_policy":
      return `auditwheel inferred ${failure.overallTag}, which is not a manylinux or musllinux policy`;
    case "policy_mismatch":
      return `the wheel declares ${linuxPolicyTag(failure.declared)}, but its binary is ${linuxPolicyTag(failure.inferred)}`;
    case "declared_too_old":
      return `the wheel declares ${linuxPolicyTag(failure.declared)}, but its binary needs ${linuxPolicyTag(failure.inferred)}`;
  }
}

function parseLinuxPolicy(tag: string): LinuxWheelPolicy | undefined {
  const groups = LINUX_POLICY_TAG_PATTERN.exec(tag)?.groups;
  return groups === undefined ? undefined : LINUX_POLICY_GROUPS_SCHEMA.parse(groups);
}
